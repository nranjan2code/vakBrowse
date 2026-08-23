//! CDP backend: drives chrome-headless-shell (or any Chromium) over the
//! Chrome DevTools Protocol via `chromiumoxide`.
//!
//! Sessions own a tab registry; every operation applies to the active tab.
//! Snapshots merge accessibility trees across the frame tree (same-process
//! frames), prefixing AX node ids with the frame id so refs stay unique.

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
use chromiumoxide::cdp::js_protocol::runtime::{CallArgument, CallFunctionOnParams};
use chromiumoxide::cdp::browser_protocol::emulation::SetTimezoneOverrideParams;
use chromiumoxide::cdp::browser_protocol::network::{
    ClearBrowserCookiesParams, CookieParam, GetCookiesParams, SetCookiesParams,
};
use chromiumoxide::cdp::browser_protocol::page::{
    AddScriptToEvaluateOnNewDocumentParams, CaptureScreenshotFormat, CaptureScreenshotParams,
    FrameId, GetFrameTreeParams,
};
use chromiumoxide::cdp::js_protocol::runtime::EvaluateParams;
use chromiumoxide::handler::viewport::Viewport;
use futures::StreamExt;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tokio::task::JoinHandle;
use vakbrowse_core::{
    Cookie, CookieInput, ElementRef, Extracted, Result, Snapshot, TabId, TabInfo, WebMcpTool,
    VakError,
};

use base64::Engine as _;
const BASE64: base64::engine::GeneralPurpose = base64::engine::general_purpose::STANDARD;

fn proto_err(e: impl std::fmt::Display) -> VakError {
    VakError::Protocol(e.to_string())
}

#[cfg(unix)]
fn is_root() -> bool {
    // SAFETY: geteuid is always safe to call.
    (unsafe { libc::geteuid() }) == 0
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

        // chromiumoxide's ArgsBuilder prepends `--` itself; passing keys with
        // dashes yields `----flag`, which Chromium silently ignores. Feed
        // bare keys (and `key=value` pairs) instead.
        fn push_arg(
            builder: chromiumoxide::browser::BrowserConfigBuilder,
            arg: &str,
        ) -> chromiumoxide::browser::BrowserConfigBuilder {
            let trimmed = arg.trim_start_matches('-');
            match trimmed.split_once('=') {
                Some((k, v)) => builder.arg((k, v)),
                None => builder.arg(trimmed),
            }
        }

        let mut builder = BrowserConfig::builder();
        builder = builder.chrome_executable(executable.clone());

        let mut args: Vec<String> = DEFAULT_ARGS.iter().map(|s| s.to_string()).collect();

        let is_shell = Self::looks_like_shell(&executable);
        if !is_shell && options.headless {
            args.push("--headless=new".into());
        }

        // Chromium's sandbox cannot operate as root (CI/Docker containers).
        // Opt out only in that impossible case rather than by configuration.
        #[cfg(unix)]
        if is_root() {
            args.push("--no-sandbox".into());
            tracing::warn!("running as root: browser sandbox disabled (unsupported by Chromium)");
        }

        if let Some(stealth) = &options.stealth {
            for arg in vakbrowse_stealth::STEALTH_ARGS {
                args.push(arg.to_string());
            }
            args.push(format!("--user-agent={}", stealth.user_agent));
            // Drop chromiumoxide's default set: it includes
            // `--enable-automation`, which is exactly the tell stealth must
            // hide. Our curated list above covers what matters.
            builder = builder.disable_default_args();
        }

        if let Some(proxy) = &options.proxy_server {
            args.push(format!("--proxy-server={proxy}"));
        }

        for arg in &args {
            builder = push_arg(builder, arg);
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
            builder = push_arg(builder, arg);
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
            apply_stealth(&page, stealth).await?;
        }

        let first = TabState {
            page,
            current_url: "about:blank".to_string(),
            refs: vakbrowse_perception::RefBook::new(),
            ref_to_ax: HashMap::new(),
            ax_to_backend: HashMap::new(),
        };
        let mut tabs = HashMap::new();
        tabs.insert(TabId("t1".into()), first);

        Ok(Box::new(CdpSession {
            browser,
            _handler_task: handler_task,
            tabs,
            active: TabId("t1".into()),
            next_tab: 2,
            stealth: options.stealth.clone(),
            pointer: (0.0, 0.0),
        }))
    }
}

async fn apply_stealth(page: &Page, stealth: &vakbrowse_stealth::StealthProfile) -> Result<()> {
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
    // New tabs inherit document-level patches because the script is
    // registered per target; register it on each new tab too (see new_tab).
    tracing::info!(seed = %stealth.seed, "stealth profile applied");
    Ok(())
}

struct TabState {
    page: Page,
    current_url: String,
    refs: vakbrowse_perception::RefBook,
    ref_to_ax: HashMap<ElementRef, String>,
    ax_to_backend: HashMap<String, BackendNodeId>,
}

/// One browser process with a tab registry. Dropping it tears the process
/// down along with every tab.
pub struct CdpSession {
    /// Kept for ownership: dropping the browser tears down the process.
    #[allow(dead_code)]
    browser: Browser,
    _handler_task: JoinHandle<()>,
    tabs: HashMap<TabId, TabState>,
    active: TabId,
    next_tab: u64,
    stealth: Option<vakbrowse_stealth::StealthProfile>,
    pointer: (f64, f64),
}

impl CdpSession {
    fn tab(&self) -> &TabState {
        self.tabs.get(&self.active).expect("active tab exists")
    }

    fn tab_mut(&mut self) -> &mut TabState {
        self.tabs.get_mut(&self.active).expect("active tab exists")
    }

    fn backend_for(&self, r: &ElementRef) -> Result<BackendNodeId> {
        let ax_id = self
            .tab()
            .ref_to_ax
            .get(r)
            .ok_or_else(|| VakError::NotFound(format!("stale element ref {r}")))?;
        self.tab()
            .ax_to_backend
            .get(ax_id)
            .copied()
            .ok_or_else(|| VakError::NotFound(format!("no backing node for {r}")))
    }

    fn flatten(prefix: &str, nodes: Vec<AxNode>) -> (Vec<vakbrowse_perception::FlatAxNode>, HashMap<String, BackendNodeId>) {
        let mut flat = Vec::with_capacity(nodes.len());
        let mut backends = HashMap::new();
        for n in nodes {
            let id = format!("{prefix}{}", n.node_id.as_ref());
            if !n.ignored && let Some(b) = n.backend_dom_node_id {
                backends.insert(id.clone(), b);
            }
            let s = |v: Option<chromiumoxide::cdp::browser_protocol::accessibility::AxValue>| {
                v.and_then(|v| v.value)
                    .and_then(|j| j.as_str().map(str::to_string))
            };
            flat.push(vakbrowse_perception::FlatAxNode {
                child_ids: n
                    .child_ids
                    .unwrap_or_default()
                    .iter()
                    .map(|c| format!("{prefix}{}", c.as_ref()))
                    .collect(),
                id,
                ignored: n.ignored,
                role: s(n.role),
                name: s(n.name),
                value: s(n.value),
            });
        }
        (flat, backends)
    }

    /// Collect flat AX nodes from every frame in the frame tree
    /// (root document first, then children depth-first).
    async fn collect_frames(tab: &TabState) -> Result<(Vec<vakbrowse_perception::FlatAxNode>, HashMap<String, BackendNodeId>)> {
        let page = &tab.page;
        let tree = page
            .execute(GetFrameTreeParams {})
            .await
            .map_err(proto_err)?
            .result
            .frame_tree;

        let mut frame_ids: Vec<FrameId> = Vec::new();
        let mut stack = vec![tree];
        while let Some(node) = stack.pop() {
            frame_ids.push(node.frame.id);
            if let Some(children) = node.child_frames {
                for c in children {
                    stack.push(c);
                }
            }
        }

        let mut all_flat = Vec::new();
        let mut all_backends = HashMap::new();
        for (i, frame_id) in frame_ids.iter().enumerate() {
            let resp = match page
                .execute(GetFullAxTreeParams::builder().frame_id(frame_id.clone()).build())
                .await
            {
                Ok(r) => r,
                Err(e) => {
                    // Cross-origin/OOPIF frames reject frame-scoped commands
                    // on this session; skip rather than fail the snapshot.
                    tracing::debug!("ax tree for frame #{i} unavailable: {e}");
                    continue;
                }
            };
            let prefix = format!("f{i}:");
            let (flat, backends) = Self::flatten(&prefix, resp.result.nodes);
            all_flat.extend(flat);
            all_backends.extend(backends);
        }
        Ok((all_flat, all_backends))
    }

    async fn box_center(&mut self, backend: BackendNodeId) -> Result<(f64, f64)> {
        let page = &self.tab().page;
        let resp = page
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

    async fn resolve_object_id(&mut self, backend: BackendNodeId) -> Result<String> {
        let page = &self.tab().page;
        let resp = page
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
        let page = &self.tab().page;
        let resp = page
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
        let page = &self.tab().page;
        if self.stealth.is_some() {
            // Humanized: bezier path with eased steps instead of a teleport.
            let path =
                vakbrowse_stealth::mouse_path(self.pointer, (x, y), 0x5EED_u64.wrapping_add(x as u64), 18);
            for (px, py) in path {
                page.execute(dispatch(px, py).map_err(proto_err)?)
                    .await
                    .map_err(proto_err)?;
                tokio::time::sleep(std::time::Duration::from_millis(4)).await;
            }
        } else {
            page.execute(dispatch(x, y).map_err(proto_err)?)
                .await
                .map_err(proto_err)?;
        }
        self.pointer = (x, y);
        Ok(())
    }

    async fn extract_inner(&mut self) -> Result<Extracted> {
        const EXTRACT_JS: &str = r#"
(() => {
  const MAX = 20000;
  // Candidate containers scored by paragraph/text density.
  const candidates = Array.from(document.querySelectorAll(
    'article, main, [role=main], .post, .entry-content, #content, #main'));
  let root = candidates[0] || document.body;
  let bestScore = -1;
  const score = (el) => el.innerText.length;
  for (const c of candidates) {
    const s = score(c);
    if (s > bestScore) { bestScore = s; root = c; }
  }
  // Drop obvious chrome from the chosen root's copy.
  root = root.cloneNode(true);
  root.querySelectorAll('script,style,noscript,nav,header,footer,aside,form,' +
    '[aria-hidden=true],[role=navigation],[role=banner],[role=contentinfo]')
    .forEach(n => n.remove());

  const lines = [];
  const push = (t) => { const x = t.replace(/\s+/g, ' ').trim(); if (x) lines.push(x); };
  const walk = (node) => {
    for (const child of node.children || []) {
      const tag = child.tagName ? child.tagName.toLowerCase() : '';
      if (['p','li','blockquote','pre','td','figcaption'].includes(tag)) {
        const headingInside = child.querySelector && child.querySelector('h1,h2,h3,h4');
        if (headingInside) walk(child);
        else push(child.innerText);
      } else if (/^h[1-6]$/.test(tag)) {
        lines.push('');
        lines.push('#'.repeat(+tag[1]) + ' ' + child.innerText.trim());
        lines.push('');
      } else if (tag === 'br') {
        continue;
      } else {
        walk(child);
      }
    }
  };
  walk(root);
  let text = lines.join('\n').replace(/\n{3,}/g, '\n\n').trim();
  let truncated = false;
  if (text.length > MAX) { text = text.slice(0, MAX); truncated = true; }
  return JSON.stringify({
    title: document.title,
    url: location.href,
    text: text,
    truncated: truncated
  });
})()"#;

        let value = self.evaluate_json_on(&self.tab().page, EXTRACT_JS).await?;
        let parsed: serde_json::Value = match value {
            serde_json::Value::String(s) => serde_json::from_str(&s)
                .map_err(|e| VakError::Protocol(format!("extract shape: {e}")))?,
            other => other,
        };
        serde_json::from_value(parsed).map_err(|e| VakError::Protocol(format!("extract: {e}")))
    }

    async fn history_go(&mut self, expr: &str) -> Result<Navigated> {
        let page = self.tab().page.clone();
        page.evaluate(expr).await.map_err(proto_err)?;
        page.wait_for_navigation().await.map_err(proto_err)?;
        let mut url = page.url().await.map_err(proto_err)?.unwrap_or_default();
        let state = self.tab_mut();
        state.current_url = url.clone();
        state.refs.reset();
        state.ref_to_ax.clear();
        state.ax_to_backend.clear();
        // Pointer position is meaningless after document change.
        self.pointer = (0.0, 0.0);

        // The old execution context dies with the document; give the new
        // one a moment before reading the title.
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(3);
        let mut title = String::new();
        loop {
            match self.title().await {
                Ok(t) if !t.is_empty() => {
                    title = t;
                    break;
                }
                Ok(_) => {
                    if tokio::time::Instant::now() >= deadline {
                        break;
                    }
                }
                Err(_) => {
                    if tokio::time::Instant::now() >= deadline {
                        break;
                    }
                }
            }
            // Re-read URL too: reload/history may have landed elsewhere.
            if let Ok(Some(live)) = page.url().await
                && live != url {
                    url = live;
                    self.tab_mut().current_url = url.clone();
                }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
        Ok(Navigated { url, title })
    }

    async fn evaluate_json_on(&self, page: &Page, expression: &str) -> Result<serde_json::Value> {
        let resp = page
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
  el.focus();
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

/// Clicks inside non-root frames can't use page-level mouse coordinates
/// (child-frame boxes are frame-relative), so we fire a real DOM click on
/// the resolved element instead. Trusted-input limitation documented in
/// AGENTS.md.
const FRAME_CLICK_JS: &str = r#"
function() {
  this.scrollIntoView({ block: 'center' });
  this.click();
  return true;
}
"#;

#[async_trait::async_trait]
impl PageOps for CdpSession {
    async fn navigate(&mut self, url: &str) -> Result<Navigated> {
        let validated = validate_url(url)?;
        self.tab().page.goto(validated.as_str()).await.map_err(proto_err)?;

        let state = self.tab_mut();
        state.current_url = validated.to_string();
        // Refs describe a document; a new document starts a new turn.
        state.refs.reset();
        state.ref_to_ax.clear();
        state.ax_to_backend.clear();

        let title = self.title().await?;
        Ok(Navigated {
            url: validated.to_string(),
            title,
        })
    }

    async fn title(&self) -> Result<String> {
        Ok(self
            .tab()
            .page
            .get_title()
            .await
            .map_err(proto_err)?
            .unwrap_or_default())
    }

    async fn eval_text(&self, expression: &str) -> Result<String> {
        let result = self
            .tab()
            .page
            .evaluate(expression)
            .await
            .map_err(proto_err)?;
        result.into_value().map_err(proto_err)
    }

    async fn snapshot(&mut self) -> Result<Snapshot> {
        // Click-driven navigations bypass our navigate(); reconcile with
        // the live URL and start a fresh ref turn when the document changed.
        if let Ok(Some(live)) = self.tab().page.url().await
            && live != self.tab().current_url
        {
            let state = self.tab_mut();
            tracing::debug!(from = %state.current_url, to = %live, "detected navigation");
            state.current_url = live;
            state.refs.reset();
            state.ref_to_ax.clear();
            state.ax_to_backend.clear();
        }
        let title = self.title().await?;
        let url = self.tab().current_url.clone();
        let (flat, backends) = Self::collect_frames(self.tab()).await?;
        let build = vakbrowse_perception::build_snapshot(&url, &title, &flat, &mut self.tab_mut().refs);
        let state = self.tab_mut();
        state.ref_to_ax = build.ref_to_ax;
        state.ax_to_backend = backends;
        Ok(build.snapshot)
    }

    async fn click(&mut self, r: &ElementRef) -> Result<()> {
        let ax_id = self
            .tab()
            .ref_to_ax
            .get(r)
            .ok_or_else(|| VakError::NotFound(format!("stale element ref {r}")))?
            .clone();
        let backend = self.backend_for(r)?;

        // Root frame: trusted input at page-level coordinates.
        if ax_id.starts_with("f0:") {
            let (cx, cy) = self.box_center(backend).await?;
            return self.click_at(cx, cy).await;
        }
        // Child frames: DOM click on the resolved element.
        let ok = self
            .call_on_element(backend, FRAME_CLICK_JS, vec![])
            .await?;
        let _ = ok;
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
            .code(code.clone())
            .windows_virtual_key_code(vk)
            .text(text.unwrap_or_default().to_string())
            .build()
            .map_err(proto_err)?;
        self.tab().page.execute(down).await.map_err(proto_err)?;

        let up = DispatchKeyEventParams::builder()
            .r#type(DispatchKeyEventType::KeyUp)
            .key(key_name)
            .code(code)
            .windows_virtual_key_code(vk)
            .build()
            .map_err(proto_err)?;
        self.tab().page.execute(up).await.map_err(proto_err)?;
        Ok(())
    }

    async fn scroll(&mut self, dx: f64, dy: f64) -> Result<()> {
        let (cx, cy) = self.viewport_center().await?;
        self.mouse_move(cx, cy).await?;
        let page = &self.tab().page;
        page.execute(
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
                .tab()
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
            .tab()
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
        self.tab()
            .page
            .execute(SetCookiesParams::builder().cookies(vec![param]).build().map_err(proto_err)?)
            .await
            .map_err(proto_err)?;
        Ok(())
    }

    async fn clear_cookies(&self) -> Result<()> {
        self.tab()
            .page
            .execute(ClearBrowserCookiesParams {})
            .await
            .map_err(proto_err)?;
        Ok(())
    }

    async fn set_download_dir(&self, dir: &Path) -> Result<()> {
        self.tab()
            .page
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
            .tab()
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
        let page = &self.tab().page;
        page.execute(press).await.map_err(proto_err)?;
        page.execute(release).await.map_err(proto_err)?;
        Ok(())
    }

    async fn webmcp_tools(&self) -> Result<Vec<WebMcpTool>> {
        let value = self
            .evaluate_json_on(
                &self.tab().page,
                r#"(() => {
                  const mc = navigator.modelContext;
                  if (!mc || typeof mc.listTools !== 'function') return [];
                  return Promise.resolve(mc.listTools()).then(ts =>
                    (ts || []).map(t => ({ name: t.name, description: t.description || '' })));
                })()"#,
            )
            .await?;
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
        match self.evaluate_json_on(&self.tab().page, &js).await {
            Ok(v) => Ok(match v {
                serde_json::Value::String(s) => s,
                other => other.to_string(),
            }),
            Err(_) => Err(VakError::Unsupported(format!(
                "WebMCP tool {name:?} not invokable (feature or tool absent)"
            ))),
        }
    }

    async fn extract(&mut self) -> Result<Extracted> {
        self.extract_inner().await
    }

    async fn back(&mut self) -> Result<Navigated> {
        self.history_go("history.back()").await
    }

    async fn forward(&mut self) -> Result<Navigated> {
        self.history_go("history.forward()").await
    }

    async fn reload(&mut self) -> Result<Navigated> {
        self.history_go("location.reload()").await
    }

    async fn tabs(&self) -> Result<Vec<TabInfo>> {
        let mut list: Vec<TabInfo> = self
            .tabs
            .iter()
            .map(|(id, t)| TabInfo {
                id: id.clone(),
                url: t.current_url.clone(),
            })
            .collect();
        list.sort_by(|a, b| a.id.0.cmp(&b.id.0));
        // Active tab first.
        if let Some(pos) = list.iter().position(|t| t.id == self.active) {
            let active = list.remove(pos);
            list.insert(0, active);
        }
        Ok(list)
    }

    async fn new_tab(&mut self, url: Option<&str>) -> Result<TabInfo> {
        let id = TabId(format!("t{}", self.next_tab));
        self.next_tab += 1;

        let page = self.browser.new_page("about:blank").await.map_err(proto_err)?;
        if let Some(stealth) = &self.stealth {
            apply_stealth(&page, stealth).await?;
        }

        let mut state = TabState {
            page,
            current_url: "about:blank".to_string(),
            refs: vakbrowse_perception::RefBook::new(),
            ref_to_ax: HashMap::new(),
            ax_to_backend: HashMap::new(),
        };

        if let Some(url) = url {
            let validated = validate_url(url)?;
            state.page.goto(validated.as_str()).await.map_err(proto_err)?;
            state.current_url = validated.to_string();
        }

        let info = TabInfo {
            id: id.clone(),
            url: state.current_url.clone(),
        };
        self.tabs.insert(id, state);
        self.active = info.id.clone();
        self.pointer = (0.0, 0.0);
        tracing::info!(tab = %info.id, "tab opened");
        Ok(info)
    }

    async fn switch_tab(&mut self, tab: &TabId) -> Result<()> {
        if !self.tabs.contains_key(tab) {
            return Err(VakError::NotFound(format!("unknown tab {tab}")));
        }
        self.active = tab.clone();
        self.pointer = (0.0, 0.0);
        Ok(())
    }

    async fn close_tab(&mut self, tab: &TabId) -> Result<bool> {
        if !self.tabs.contains_key(tab) {
            return Ok(false);
        }
        if self.tabs.len() == 1 {
            return Err(VakError::Engine(
                "cannot close the last remaining tab".into(),
            ));
        }
        let state = self.tabs.remove(tab).expect("checked above");
        // Best-effort target close; dropping the Page also detaches.
        let _ = state.page.close().await;
        if self.active == *tab {
            // Pick any remaining tab deterministically.
            if let Some(next) = self.tabs.keys().next().cloned() {
                self.active = next;
            }
        }
        tracing::info!(%tab, "tab closed");
        Ok(true)
    }
}
