//! CDP backend: drives chrome-headless-shell (or any Chromium) over the
//! Chrome DevTools Protocol via `chromiumoxide`.

use crate::cft::{self, CftConfig};
use crate::{
    EngineLauncher, LaunchOptions, Navigated, PageOps, validate_url,
};
use chromiumoxide::Page;
use chromiumoxide::browser::{Browser, BrowserConfig};
use chromiumoxide::cdp::browser_protocol::accessibility::{
    AxNode, GetFullAxTreeParams,
};
use chromiumoxide::cdp::browser_protocol::browser::{
    SetDownloadBehaviorBehavior, SetDownloadBehaviorParams,
};
use chromiumoxide::cdp::browser_protocol::dom::{BackendNodeId, GetBoxModelParams, ResolveNodeParams};
use chromiumoxide::cdp::browser_protocol::input::{
    DispatchKeyEventParams, DispatchKeyEventType, DispatchMouseEventParams,
    DispatchMouseEventType, MouseButton,
};
use chromiumoxide::cdp::browser_protocol::network::{
    ClearBrowserCookiesParams, CookieParam, GetCookiesParams, SetCookiesParams,
};
use chromiumoxide::cdp::browser_protocol::emulation::SetTimezoneOverrideParams;
use chromiumoxide::cdp::browser_protocol::page::{
    AddScriptToEvaluateOnNewDocumentParams, CaptureScreenshotFormat, CaptureScreenshotParams,
};
use chromiumoxide::cdp::js_protocol::runtime::EvaluateParams;
use chromiumoxide::cdp::js_protocol::runtime::{CallArgument, CallFunctionOnParams};
use chromiumoxide::handler::viewport::Viewport;
use futures::StreamExt;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tokio::task::JoinHandle;
use vakbrowse_core::{
    Cookie, CookieInput, ElementRef, Result, Snapshot, WebMcpTool, VakError,
};

use base64::Engine as _;
const BASE64: base64::engine::GeneralPurpose = base64::engine::general_purpose::STANDARD;

fn proto_err(e: impl std::fmt::Display) -> VakError {
    VakError::Protocol(e.to_string())
}

/// Default hardening flags applied to every launch. Deliberately NOT
/// including `--no-sandbox`; sandboxing stays on and is the caller's
/// environment responsibility.
const DEFAULT_ARGS: &[&str] = &[
    "--no-first-run",
    "--no-default-browser-check",
    "--disable-background-networking",
    "--disable-component-update",
    "--disable-sync",
    "--metrics-recording-only",
    "--mute-audio",
    "--password-store=basic",
    "--disable-dev-shm-usage",
];

#[derive(Debug, Clone, Default)]
pub struct CdpLauncher {
    /// Configuration for managed chrome-headless-shell acquisition.
    pub cft: CftConfig,
}

impl CdpLauncher {
    async fn resolve_executable(&self, explicit: Option<&PathBuf>) -> Result<PathBuf> {
        if let Some(path) = explicit {
            return Ok(path.clone());
        }
        // Managed shell first (reproducible), system chrome as offline fallback.
        match cft::ensure_headless_shell(&self.cft).await {
            Ok(artifact) => Ok(artifact.executable),
            Err(download_err) => {
                if let Some(system) = cft::find_system_chrome() {
                    tracing::warn!(
                        "managed engine unavailable ({download_err}); using system chrome"
                    );
                    Ok(system)
                } else {
                    Err(download_err)
                }
            }
        }
    }

    fn looks_like_shell(path: &std::path::Path) -> bool {
        path.file_name()
            .and_then(|n| n.to_str())
            .map(|n| n.contains("headless-shell"))
            .unwrap_or(false)
    }
}

impl EngineLauncher for CdpLauncher {
    fn name(&self) -> &'static str {
        "chrome-headless-shell/cdp"
    }

    async fn ensure_executable(&self) -> Result<PathBuf> {
        let artifact = cft::ensure_headless_shell(&self.cft).await?;
        Ok(artifact.executable)
    }

    async fn launch(&self, options: &LaunchOptions) -> Result<Box<dyn PageOps>> {
        let executable = self.resolve_executable(options.executable.as_ref()).await?;
        tracing::info!(exe = %executable.display(), "launching browser");

        let mut builder = BrowserConfig::builder();
        builder = builder.chrome_executable(executable.clone());

        for arg in DEFAULT_ARGS {
            builder = builder.arg(*arg);
        }

        let is_shell = Self::looks_like_shell(&executable);
        if !is_shell && options.headless {
            builder = builder.arg("--headless=new");
        }

        if let Some(stealth) = &options.stealth {
            for arg in vakbrowse_stealth::STEALTH_ARGS {
                builder = builder.arg(*arg);
            }
            builder = builder.arg(format!("--user-agent={}", stealth.user_agent));
        }

        if let Some(dir) = &options.user_data_dir {
            builder = builder.user_data_dir(dir);
        }

        builder = builder.window_size(options.window_size.0, options.window_size.1);
        builder = builder.viewport(Some(Viewport {
            width: options.window_size.0,
            height: options.window_size.1,
            ..Viewport::default()
        }));

        for arg in &options.extra_args {
            builder = builder.arg(arg.as_str());
        }

        let config = builder.build().map_err(proto_err)?;
        let (browser, mut handler) = Browser::launch(config).await.map_err(proto_err)?;

        let handler_task = tokio::spawn(async move {
            while let Some(event) = handler.next().await {
                if let Err(e) = event {
                    tracing::debug!("cdp handler event error: {e}");
                }
            }
        });

        let page = browser.new_page("about:blank").await.map_err(proto_err)?;

        if let Some(stealth) = &options.stealth {
            page.execute(AddScriptToEvaluateOnNewDocumentParams {
                source: stealth.init_script(),
                world_name: None,
                include_command_line_api: None,
                run_immediately: None,
            })
            .await
            .map_err(proto_err)?;
            page.execute(SetTimezoneOverrideParams {
                timezone_id: stealth.timezone_id.clone(),
            })
            .await
            .map_err(proto_err)?;
            tracing::info!(seed = %stealth.seed, "stealth profile applied");
        }

        Ok(Box::new(CdpSession {
            browser,
            _handler_task: handler_task,
            page,
            current_url: "about:blank".to_string(),
            refs: vakbrowse_perception::RefBook::new(),
            ref_to_ax: HashMap::new(),
            ax_to_backend: HashMap::new(),
            stealth: options.stealth.clone(),
            pointer: (0.0, 0.0),
        }))
    }
}

/// One browser process plus its current page. Dropping it tears the
/// browser process down.
pub struct CdpSession {
    /// Kept for ownership: dropping the browser tears down the process.
    #[allow(dead_code)]
    browser: Browser,
    _handler_task: JoinHandle<()>,
    page: Page,
    current_url: String,
    refs: vakbrowse_perception::RefBook,
    ref_to_ax: HashMap<ElementRef, String>,
    ax_to_backend: HashMap<String, BackendNodeId>,
    stealth: Option<vakbrowse_stealth::StealthProfile>,
    pointer: (f64, f64),
}

impl CdpSession {
    fn backend_for(&self, r: &ElementRef) -> Result<BackendNodeId> {
        let ax_id = self
            .ref_to_ax
            .get(r)
            .ok_or_else(|| VakError::NotFound(format!("stale element ref {r}")))?;
        self.ax_to_backend
            .get(ax_id)
            .copied()
            .ok_or_else(|| VakError::NotFound(format!("no backing node for {r}")))
    }

    fn flatten(nodes: Vec<AxNode>) -> (Vec<vakbrowse_perception::FlatAxNode>, HashMap<String, BackendNodeId>) {
        let mut flat = Vec::with_capacity(nodes.len());
        let mut backends = HashMap::new();
        for n in nodes {
            if !n.ignored && let Some(b) = n.backend_dom_node_id {
                backends.insert(n.node_id.as_ref().to_string(), b);
            }
            let s = |v: Option<chromiumoxide::cdp::browser_protocol::accessibility::AxValue>| {
                v.and_then(|v| v.value)
                    .and_then(|j| j.as_str().map(str::to_string))
            };
            flat.push(vakbrowse_perception::FlatAxNode {
                id: n.node_id.as_ref().to_string(),
                ignored: n.ignored,
                role: s(n.role),
                name: s(n.name),
                value: s(n.value),
                child_ids: n
                    .child_ids
                    .unwrap_or_default()
                    .iter()
                    .map(|c| c.as_ref().to_string())
                    .collect(),
            });
        }
        (flat, backends)
    }

    async fn box_center(&mut self, backend: BackendNodeId) -> Result<(f64, f64)> {
        let resp = self
            .page
            .execute(GetBoxModelParams::builder().backend_node_id(backend).build())
            .await
            .map_err(proto_err)?;
        let q = resp.result.model.content.inner();
        if q.len() < 8 {
            return Err(VakError::Engine("box model missing corners".into()));
        }
        let cx = (q[0] + q[2] + q[4] + q[6]) / 4.0;
        let cy = (q[1] + q[3] + q[5] + q[7]) / 4.0;
        Ok((cx, cy))
    }

    async fn resolve_object_id(
        &mut self,
        backend: BackendNodeId,
    ) -> Result<String> {
        let resp = self
            .page
            .execute(ResolveNodeParams::builder().backend_node_id(backend).build())
            .await
            .map_err(proto_err)?;
        resp.result
            .object
            .object_id
            .map(|id| id.as_ref().to_string())
            .ok_or_else(|| VakError::Protocol("resolved node has no objectId".into()))
    }

    async fn call_on_element(
        &mut self,
        backend: BackendNodeId,
        function_declaration: &str,
        args: Vec<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let object_id = self.resolve_object_id(backend).await?;
        let arguments: Vec<CallArgument> = args
            .into_iter()
            .map(|v| CallArgument {
                value: Some(v),
                ..Default::default()
            })
            .collect();
        let resp = self
            .page
            .execute(
                CallFunctionOnParams::builder()
                    .function_declaration(function_declaration)
                    .object_id(object_id)
                    .arguments(arguments)
                    .build()
                    .map_err(proto_err)?,
            )
            .await
            .map_err(proto_err)?;
        Ok(resp.result.result.value.unwrap_or(serde_json::Value::Null))
    }

    async fn viewport_center(&mut self) -> Result<(f64, f64)> {
        let w = self.eval_text("window.innerWidth").await?;
        let h = self.eval_text("window.innerHeight").await?;
        let cx = w.trim().parse::<f64>().unwrap_or(640.0) / 2.0;
        let cy = h.trim().parse::<f64>().unwrap_or(400.0) / 2.0;
        Ok((cx, cy))
    }

    async fn mouse_move(&mut self, x: f64, y: f64) -> Result<()> {
        let dispatch = |x: f64, y: f64| {
            DispatchMouseEventParams::builder()
                .r#type(DispatchMouseEventType::MouseMoved)
                .x(x)
                .y(y)
                .build()
        };
        if self.stealth.is_some() {
            // Humanized: bezier path with eased steps instead of a teleport.
            let path =
                vakbrowse_stealth::mouse_path(self.pointer, (x, y), 0x5EED_u64.wrapping_add(x as u64), 18);
            for (px, py) in path {
                self.page
                    .execute(dispatch(px, py).map_err(proto_err)?)
                    .await
                    .map_err(proto_err)?;
                tokio::time::sleep(std::time::Duration::from_millis(4)).await;
            }
        } else {
            self.page
                .execute(dispatch(x, y).map_err(proto_err)?)
                .await
                .map_err(proto_err)?;
        }
        self.pointer = (x, y);
        Ok(())
    }

    async fn evaluate_json(&self, expression: &str) -> Result<serde_json::Value> {
        let resp = self
            .page
            .execute(
                EvaluateParams::builder()
                    .expression(expression)
                    .await_promise(true)
                    .return_by_value(true)
                    .build()
                    .map_err(proto_err)?,
            )
            .await
            .map_err(proto_err)?;
        Ok(resp.result.result.value.unwrap_or(serde_json::Value::Null))
    }
}

const SET_VALUE_JS: &str = r#"
function(val) {
  const el = this;
  const proto = el instanceof HTMLTextAreaElement
    ? HTMLTextAreaElement.prototype
    : HTMLInputElement.prototype;
  if (proto && Object.getOwnPropertyDescriptor(proto, 'value')) {
    Object.getOwnPropertyDescriptor(proto, 'value').set.call(el, val);
  } else {
    el.innerText = val;
  }
  el.dispatchEvent(new Event('input', { bubbles: true }));
  el.dispatchEvent(new Event('change', { bubbles: true }));
  return String(el.value);
}
"#;

const SELECT_OPTION_JS: &str = r#"
function(val) {
  try {
    const el = this;
    if (!el) return 'no-element';
    if (el.tagName !== 'SELECT') return 'wrong-tag:' + el.tagName;
    el.value = val;
    el.dispatchEvent(new Event('input', { bubbles: true }));
    el.dispatchEvent(new Event('change', { bubbles: true }));
    return el.value === val;
  } catch (e) {
    return 'error:' + e.message;
  }
}
"#;

#[async_trait::async_trait]
impl PageOps for CdpSession {
    async fn navigate(&mut self, url: &str) -> Result<Navigated> {
        let validated = validate_url(url)?;
        self.page.goto(validated.as_str()).await.map_err(proto_err)?;

        self.current_url = validated.to_string();
        // Refs describe a document; a new document starts a new turn.
        self.refs.reset();
        self.ref_to_ax.clear();
        self.ax_to_backend.clear();

        let title = self.title().await?;
        Ok(Navigated {
            url: self.current_url.clone(),
            title,
        })
    }

    async fn title(&self) -> Result<String> {
        Ok(self
            .page
            .get_title()
            .await
            .map_err(proto_err)?
            .unwrap_or_default())
    }

    async fn eval_text(&self, expression: &str) -> Result<String> {
        let result = self.page.evaluate(expression).await.map_err(proto_err)?;
        result.into_value().map_err(proto_err)
    }

    async fn snapshot(&mut self) -> Result<Snapshot> {
        let title = self.title().await?;
        let resp = self
            .page
            .execute(GetFullAxTreeParams::default())
            .await
            .map_err(proto_err)?;
        let (flat, backends) = Self::flatten(resp.result.nodes);
        let build =
            vakbrowse_perception::build_snapshot(&self.current_url, &title, &flat, &mut self.refs);
        self.ref_to_ax = build.ref_to_ax;
        self.ax_to_backend = backends;
        Ok(build.snapshot)
    }

    async fn click(&mut self, r: &ElementRef) -> Result<()> {
        let backend = self.backend_for(r)?;
        let (cx, cy) = self.box_center(backend).await?;
        self.mouse_move(cx, cy).await?;

        let press = || {
            DispatchMouseEventParams::builder()
                .r#type(DispatchMouseEventType::MousePressed)
                .x(cx)
                .y(cy)
                .button(MouseButton::Left)
                .click_count(1)
                .build()
        };
        let release = || {
            DispatchMouseEventParams::builder()
                .r#type(DispatchMouseEventType::MouseReleased)
                .x(cx)
                .y(cy)
                .button(MouseButton::Left)
                .click_count(1)
                .build()
        };
        self.page.execute(press().map_err(proto_err)?).await.map_err(proto_err)?;
        self.page.execute(release().map_err(proto_err)?).await.map_err(proto_err)?;
        Ok(())
    }

    async fn fill(&mut self, r: &ElementRef, text: &str) -> Result<()> {
        let backend = self.backend_for(r)?;
        self.call_on_element(backend, SET_VALUE_JS, vec![serde_json::json!(text)])
            .await?;
        Ok(())
    }

    async fn select_option(&mut self, r: &ElementRef, value: &str) -> Result<bool> {
        let backend = self.backend_for(r)?;
        let out = self
            .call_on_element(backend, SELECT_OPTION_JS, vec![serde_json::json!(value)])
            .await?;
        match out {
            serde_json::Value::Bool(v) => Ok(v),
            other => Err(VakError::Engine(format!(
                "select_option diagnostic: {other}"
            ))),
        }
    }

    async fn press_key(&mut self, key: &str) -> Result<()> {
        let (code, key_name, vk, text): (String, String, i64, Option<&str>) = match key {
            "Enter" => ("Enter".into(), "Enter".into(), 13, Some("\r")),
            "Tab" => ("Tab".into(), "Tab".into(), 9, None),
            "Escape" => ("Escape".into(), "Escape".into(), 27, None),
            "Backspace" => ("Backspace".into(), "Backspace".into(), 8, None),
            "Delete" => ("Delete".into(), "Delete".into(), 46, None),
            "ArrowDown" => ("ArrowDown".into(), "ArrowDown".into(), 40, None),
            "ArrowUp" => ("ArrowUp".into(), "ArrowUp".into(), 38, None),
            "ArrowLeft" => ("ArrowLeft".into(), "ArrowLeft".into(), 37, None),
            "ArrowRight" => ("ArrowRight".into(), "ArrowRight".into(), 39, None),
            "Home" => ("Home".into(), "Home".into(), 36, None),
            "End" => ("End".into(), "End".into(), 35, None),
            "PageUp" => ("PageUp".into(), "PageUp".into(), 33, None),
            "PageDown" => ("PageDown".into(), "PageDown".into(), 34, None),
            k if k.chars().count() == 1 => {
                let c = k.chars().next().expect("len checked");
                let code = if c == ' ' {
                    "Space".to_string()
                } else {
                    c.to_ascii_uppercase().to_string()
                };
                (code, k.to_string(), c.to_ascii_uppercase() as i64, Some(k))
            }
            other => {
                return Err(VakError::Unsupported(format!("key {other:?}")));
            }
        };
        let key_type = if text.is_some() {
            DispatchKeyEventType::KeyDown
        } else {
            DispatchKeyEventType::RawKeyDown
        };

        let down = DispatchKeyEventParams::builder()
            .r#type(key_type)
            .key(key_name.clone())
            .code(code.to_string())
            .windows_virtual_key_code(vk)
            .text(text.unwrap_or_default().to_string())
            .build()
            .map_err(proto_err)?;
        self.page.execute(down).await.map_err(proto_err)?;

        let up = DispatchKeyEventParams::builder()
            .r#type(DispatchKeyEventType::KeyUp)
            .key(key_name)
            .code(code.to_string())
            .windows_virtual_key_code(vk)
            .build()
            .map_err(proto_err)?;
        self.page.execute(up).await.map_err(proto_err)?;
        Ok(())
    }

    async fn scroll(&mut self, dx: f64, dy: f64) -> Result<()> {
        let (cx, cy) = self.viewport_center().await?;
        self.mouse_move(cx, cy).await?;
        self.page
            .execute(
                DispatchMouseEventParams::builder()
                    .r#type(DispatchMouseEventType::MouseWheel)
                    .x(cx)
                    .y(cy)
                    .delta_x(dx)
                    .delta_y(dy)
                    .build()
                    .map_err(proto_err)?,
            )
            .await
            .map_err(proto_err)?;
        Ok(())
    }

    async fn wait_for_truthy(&self, expression: &str, timeout_ms: u64) -> Result<()> {
        let expr = format!("!!({expression})");
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_millis(timeout_ms);
        loop {
            let truthy = self
                .page
                .evaluate(expr.as_str())
                .await
                .ok()
                .and_then(|r| r.into_value::<bool>().ok())
                .unwrap_or(false);
            if truthy {
                return Ok(());
            }
            if tokio::time::Instant::now() >= deadline {
                return Err(VakError::Timeout(format!("wait_for_truthy: {expression}")));
            }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
    }

    async fn cookies(&self) -> Result<Vec<Cookie>> {
        let resp = self
            .page
            .execute(GetCookiesParams::default())
            .await
            .map_err(proto_err)?;
        Ok(resp
            .result
            .cookies
            .into_iter()
            .map(|c| Cookie {
                name: c.name,
                value: c.value,
                domain: c.domain,
                path: c.path,
                secure: c.secure,
                http_only: c.http_only,
                session: c.session,
            })
            .collect())
    }

    async fn set_cookie(&mut self, cookie: &CookieInput) -> Result<()> {
        let param = CookieParam::builder()
            .name(cookie.name.clone())
            .value(cookie.value.clone())
            .domain(cookie.domain.clone())
            .path(cookie.path.clone())
            .secure(cookie.secure)
            .http_only(cookie.http_only)
            .build()
            .map_err(proto_err)?;
        self.page
            .execute(SetCookiesParams::builder().cookies(vec![param]).build().map_err(proto_err)?)
            .await
            .map_err(proto_err)?;
        Ok(())
    }

    async fn clear_cookies(&self) -> Result<()> {
        self.page
            .execute(ClearBrowserCookiesParams {})
            .await
            .map_err(proto_err)?;
        Ok(())
    }

    async fn set_download_dir(&self, dir: &Path) -> Result<()> {
        self.page
            .execute(
                SetDownloadBehaviorParams::builder()
                    .behavior(SetDownloadBehaviorBehavior::AllowAndName)
                    .download_path(dir.to_string_lossy().to_string())
                    .build()
                    .map_err(proto_err)?,
            )
            .await
            .map_err(proto_err)?;
        Ok(())
    }

    async fn screenshot(&self, full_page: bool) -> Result<Vec<u8>> {
        let resp = self
            .page
            .execute(
                CaptureScreenshotParams::builder()
                    .format(CaptureScreenshotFormat::Png)
                    .capture_beyond_viewport(full_page)
                    .build(),
            )
            .await
            .map_err(proto_err)?;
        let b64: &str = resp.result.data.as_ref();
        BASE64
            .decode(b64)
            .map_err(|e| VakError::Engine(format!("screenshot base64: {e}")))
    }

    async fn click_at(&mut self, x: f64, y: f64) -> Result<()> {
        self.mouse_move(x, y).await?;
        let press = DispatchMouseEventParams::builder()
            .r#type(DispatchMouseEventType::MousePressed)
            .x(x)
            .y(y)
            .button(MouseButton::Left)
            .click_count(1)
            .build()
            .map_err(proto_err)?;
        let release = DispatchMouseEventParams::builder()
            .r#type(DispatchMouseEventType::MouseReleased)
            .x(x)
            .y(y)
            .button(MouseButton::Left)
            .click_count(1)
            .build()
            .map_err(proto_err)?;
        self.page.execute(press).await.map_err(proto_err)?;
        self.page.execute(release).await.map_err(proto_err)?;
        Ok(())
    }

    async fn webmcp_tools(&self) -> Result<Vec<WebMcpTool>> {
        let value = self
            .evaluate_json(
                r#"(() => {
                  const mc = navigator.modelContext;
                  if (!mc || typeof mc.listTools !== 'function') return [];
                  return Promise.resolve(mc.listTools()).then(ts =>
                    (ts || []).map(t => ({ name: t.name, description: t.description || '' })));
                })()"#,
            )
            .await?; // evaluate_json is &mut... see note below
        serde_json::from_value(value)
            .map_err(|e| VakError::Protocol(format!("webmcp tools shape: {e}")))
    }

    async fn webmcp_invoke(&self, name: &str, arguments_json: &str) -> Result<String> {
        // Arguments are parsed host-side so malformed JSON fails here, not in-page.
        let args: serde_json::Value = serde_json::from_str(arguments_json)
            .map_err(|e| VakError::Engine(format!("arguments_json: {e}")))?;
        let args = if args.is_null() { serde_json::json!({}) } else { args };
        let js = format!(
            r#"(() => {{
              const mc = navigator.modelContext;
              if (!mc || typeof mc.callTool !== 'function')
                throw new Error('webmcp unavailable on this page');
              return Promise.resolve(mc.callTool({name}, {args})).then(v => JSON.stringify(v));
            }})()"#,
            name = serde_json::json!(name),
            args = args,
        );
        match self.evaluate_json(&js).await {
            Ok(v) => Ok(match v {
                serde_json::Value::String(s) => s,
                other => other.to_string(),
            }),
            Err(_) => Err(VakError::Unsupported(format!(
                "WebMCP tool {name:?} not invokable (feature or tool absent)"
            ))),
        }
    }
}
