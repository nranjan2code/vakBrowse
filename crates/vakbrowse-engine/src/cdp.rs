//! CDP backend: drives chrome-headless-shell (or any Chromium) over the
//! Chrome DevTools Protocol via `chromiumoxide`.
//!
//! Sessions own a tab registry; every operation applies to the active tab.
//! Snapshots merge accessibility trees across the frame tree (same-process
//! frames), prefixing AX node ids with the frame id so refs stay unique.

use crate::cft::{self, CftConfig};
use crate::{
    ClickResult, EngineLauncher, LaunchOptions, Navigated, PageOps, SessionState, validate_url,
};
use chromiumoxide::Page;
use chromiumoxide::browser::{Browser, BrowserConfig};
use chromiumoxide::cdp::browser_protocol::accessibility::{
    AxNode, AxProperty, AxPropertyName, GetFullAxTreeParams,
};
use chromiumoxide::cdp::browser_protocol::browser::{
    SetDownloadBehaviorBehavior, SetDownloadBehaviorParams,
};
use chromiumoxide::cdp::browser_protocol::dom::{
    BackendNodeId, GetBoxModelParams, ResolveNodeParams, SetFileInputFilesParams,
};
use chromiumoxide::cdp::browser_protocol::emulation::SetTimezoneOverrideParams;
use chromiumoxide::cdp::browser_protocol::input::{
    DispatchKeyEventParams, DispatchKeyEventType, DispatchMouseEventParams, DispatchMouseEventType,
    MouseButton,
};
use chromiumoxide::cdp::browser_protocol::network::{
    ClearBrowserCookiesParams, CookieParam, SetCookiesParams, TimeSinceEpoch,
};
use chromiumoxide::cdp::browser_protocol::storage::GetCookiesParams;
use chromiumoxide::cdp::browser_protocol::page::{
    AddScriptToEvaluateOnNewDocumentParams, CaptureScreenshotFormat, CaptureScreenshotParams,
    FrameId, GetFrameTreeParams,
};
use chromiumoxide::cdp::js_protocol::runtime::{CallArgument, CallFunctionOnParams};
use chromiumoxide::cdp::js_protocol::runtime::{EvaluateParams, RemoteObjectType};
use chromiumoxide::handler::viewport::Viewport;
use futures::StreamExt;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tokio::task::JoinHandle;
use vakbrowse_core::{
    Cookie, CookieInput, DownloadInfo, ElementRef, ExtractWindow, Extracted, Result, Snapshot,
    TabId, TabInfo, VakError, WebMcpTool,
};

use base64::Engine as _;
const BASE64: base64::engine::GeneralPurpose = base64::engine::general_purpose::STANDARD;

fn proto_err(e: impl std::fmt::Display) -> VakError {
    VakError::Protocol(e.to_string())
}

/// Coerce a JS `Runtime.evaluate` result value into a display string.
/// Mirrors what a JS REPL prints: strings pass through, primitives stringify,
/// objects/arrays become JSON. Never fails (unlike deserializing straight
/// into `String`, which rejects booleans/numbers).
fn stringify_js(v: serde_json::Value) -> String {
    match v {
        serde_json::Value::String(s) => s,
        serde_json::Value::Null => "null".to_string(),
        serde_json::Value::Bool(b) => b.to_string(),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                i.to_string()
            } else if let Some(f) = n.as_f64() {
                format!("{}", f)
            } else {
                n.to_string()
            }
        }
        serde_json::Value::Array(_) | serde_json::Value::Object(_) => {
            serde_json::to_string(&v).unwrap_or_else(|e| format!("<unserializable value: {e}>"))
        }
    }
}

fn ax_truthy(v: &serde_json::Value) -> bool {
    v.as_bool().unwrap_or_else(|| v.as_str() == Some("true"))
}

/// Widget state tokens + heading level from an AX node's properties.
fn ax_state(props: &[AxProperty]) -> (Vec<String>, Option<u8>) {
    let mut state = Vec::new();
    let mut level = None;
    for p in props {
        let Some(v) = p.value.value.as_ref() else {
            continue;
        };
        match p.name {
            AxPropertyName::Checked => state.push(
                match v.as_str().unwrap_or(if ax_truthy(v) { "true" } else { "false" }) {
                    "true" => "checked",
                    "mixed" => "mixed",
                    _ => "unchecked",
                }
                .to_string(),
            ),
            AxPropertyName::Disabled if ax_truthy(v) => state.push("disabled".into()),
            AxPropertyName::Expanded => state.push(
                if ax_truthy(v) { "expanded" } else { "collapsed" }.to_string(),
            ),
            AxPropertyName::Selected if ax_truthy(v) => state.push("selected".into()),
            AxPropertyName::Required if ax_truthy(v) => state.push("required".into()),
            AxPropertyName::Level => level = v.as_u64().map(|n| n.min(6) as u8),
            _ => {}
        }
    }
    (state, level)
}

const MOD_ALT: i64 = 1;
const MOD_CTRL: i64 = 2;
const MOD_META: i64 = 4;
const MOD_SHIFT: i64 = 8;

/// Split `"Control+Shift+a"` into (CDP modifier bitmask, base key).
/// A lone `"+"` is the plus key; unknown modifier names are an error rather
/// than being silently dropped.
fn parse_key_combo(spec: &str) -> Result<(i64, String)> {
    if spec.chars().count() <= 1 || !spec.contains('+') {
        return Ok((0, spec.to_string()));
    }
    let (mods_part, base) = match spec.strip_suffix("++") {
        Some(head) => (head, "+"),
        None => spec.rsplit_once('+').unwrap_or(("", spec)),
    };
    let mut mods = 0;
    for m in mods_part.split('+').filter(|m| !m.is_empty()) {
        mods |= match m.to_ascii_lowercase().as_str() {
            "alt" | "option" => MOD_ALT,
            "control" | "ctrl" => MOD_CTRL,
            "meta" | "cmd" | "command" => MOD_META,
            "shift" => MOD_SHIFT,
            other => {
                return Err(VakError::Unsupported(format!(
                    "unknown modifier {other:?} in key {spec:?}"
                )));
            }
        };
    }
    Ok((mods, base.to_string()))
}

#[cfg(unix)]
fn is_root() -> bool {
    // SAFETY: geteuid is always safe to call.
    (unsafe { libc::geteuid() }) == 0
}

/// Heuristic: did Chromium fail to launch *because of its sandbox*?
///
/// Root cause is always one of: an explicit sandbox/zygote/namespace
/// diagnostic, or chrome exiting before the WebSocket URL prints. The
/// non-root case is subtle: Chromium fires `FATAL: ... No usable sandbox!
/// ...` and exits, but when captured through chromiumoxide's pipe that
/// stderr is often empty — so the error string is just "Browser process
/// exited ... before websocket URL could be resolved, stderr: BrowserStderr(\"\")".
///
/// We only classify the *silent* early-exit (empty stderr) as sandbox-suspect
/// because genuine failures still leave a trace: a missing binary yields
/// `No such file or directory`, a missing shared library yields the loader's
/// `error while loading shared libraries`. Those are excluded so we never
/// mask a real misconfiguration behind a `--no-sandbox` retry.
fn is_sandbox_launch_failure(msg: &str) -> bool {
    let lower = msg.to_lowercase();
    if lower.contains("sandbox") || lower.contains("zygote") || lower.contains("namespace") {
        return true;
    }
    if msg.contains("BrowserStderr(\"\")")
        && (msg.contains("before websocket URL could be resolved")
            || msg.contains("resolving websocket URL from browser process"))
        && !lower.contains("no such file or directory")
        && !lower.contains("error while loading shared libraries")
    {
        return true;
    }
    false
}

/// Default hardening flags applied to every launch. Deliberately NOT
/// including `--no-sandbox`; sandboxing stays on and is the caller's
/// environment responsibility.
/// Lean rendering: images, web fonts and autoplaying media are the bulk of a
/// page's memory and CPU, and agents read text and the a11y tree, not pixels.
const LEAN_ARGS: &[&str] = &[
    "--blink-settings=imagesEnabled=false",
    "--disable-remote-fonts",
    "--autoplay-policy=user-gesture-required",
];

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

#[async_trait::async_trait]
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

        if options.lean {
            args.extend(LEAN_ARGS.iter().map(|a| a.to_string()));
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

        let config = builder.clone().build().map_err(proto_err)?;
        let launched = tokio::time::timeout(
            std::time::Duration::from_secs(30),
            Browser::launch(config),
        )
        .await;
        let (browser, mut handler) = match launched {
            Ok(result) => match result {
                Ok(ok) => ok,
                Err(e) => {
                    // Hardened environments (CI runners, Docker containers) often
                    // cannot run Chromium's setuid/userns sandbox at all. Retry
                    // once without the sandbox rather than failing the session.
                    let msg = e.to_string();
                    if !is_sandbox_launch_failure(&msg) {
                        return Err(proto_err(e));
                    }
                    tracing::warn!("sandboxed launch failed ({msg}); retrying with --no-sandbox");
                    let retry_config = builder
                        .arg("no-sandbox")
                        .arg("disable-setuid-sandbox")
                        .build()
                        .map_err(proto_err)?;
                    tokio::time::timeout(
                        std::time::Duration::from_secs(30),
                        Browser::launch(retry_config),
                    )
                    .await
                    .map_err(|_| VakError::Engine("browser launch timed out (30s)".into()))?
                    .map_err(proto_err)?
                }
            },
            Err(_) => {
                return Err(VakError::Engine(
                    "browser launch timed out (30s)".into(),
                ));
            }
        };

        let handler_task = tokio::spawn(async move {
            while let Some(event) = handler.next().await {
                if let Err(e) = event {
                    tracing::debug!("cdp handler event error: {e}");
                }
            }
        });

        let page = browser.new_page("about:blank").await.map_err(proto_err)?;
        // Chrome may open its own startup tab (chrome://new-tab-page/ under
        // Linux chrome-headless-shell); left alone it is adopted as a phantom
        // tab that costs memory and multiplies across hibernate/restore.
        if let Ok(pages) = browser.pages().await {
            for p in pages {
                if p.target_id() != page.target_id() {
                    let _ = p.close().await;
                }
            }
        }

        if let Some(stealth) = &options.stealth {
            apply_stealth(&page, stealth).await?;
        }

        let first = TabState {
            page,
            current_url: "about:blank".to_string(),
            refs: vakbrowse_perception::RefBook::new(),
            ref_to_ax: HashMap::new(),
            ax_to_backend: HashMap::new(),
            frame_index: HashMap::new(),
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
            human_timing: options.human_timing,
            click_recovery: options.click_recovery,
            download_dir: None,
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
    /// Frame id -> `fN:` prefix index, assigned on first sight so a frame
    /// keeps its ref prefix when siblings appear or disappear. The root
    /// frame is always `f0` (clicks route on that prefix).
    frame_index: HashMap<String, usize>,
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
    human_timing: bool,
    click_recovery: bool,
    download_dir: Option<PathBuf>,
}

impl Drop for CdpSession {
    fn drop(&mut self) {
        // Abort the CDP event-handler task so it doesn't linger if the browser
        // didn't close cleanly. If the handler already completed, this is a
        // no-op. Panics inside the handler (caught by the spawn loop) won't
        // prevent this — the JoinHandle is dropped after the abort.
        self._handler_task.abort();
    }
}

impl CdpSession {
    fn tab(&self) -> &TabState {
        self.tabs.get(&self.active).expect("active tab exists")
    }

    fn tab_mut(&mut self) -> &mut TabState {
        self.tabs.get_mut(&self.active).expect("active tab exists")
    }

    /// Break robotic input cadence when `human_timing` is enabled. Off by
    /// default so non-agent callers see no slowdown. Uses a splitmix64-mixed
    /// timestamp + per-thread counter for de-correlated jitter — timing, not
    /// secrets.
    async fn maybe_human_jitter(&self) {
        if self.human_timing {
            tokio::time::sleep(human_jitter()).await;
        }
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

    fn flatten(
        prefix: &str,
        nodes: Vec<AxNode>,
    ) -> (
        Vec<vakbrowse_perception::FlatAxNode>,
        HashMap<String, BackendNodeId>,
    ) {
        let mut flat = Vec::with_capacity(nodes.len());
        let mut backends = HashMap::new();
        for n in nodes {
            let (state, level) = ax_state(n.properties.as_deref().unwrap_or(&[]));
            let id = format!("{prefix}{}", n.node_id.as_ref());
            if !n.ignored
                && let Some(b) = n.backend_dom_node_id
            {
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
                state,
                level,
            });
        }
        (flat, backends)
    }

    /// Collect flat AX nodes from every frame in the frame tree
    /// (root document first, then children depth-first).
    async fn collect_frames(
        tab: &mut TabState,
    ) -> Result<(
        Vec<vakbrowse_perception::FlatAxNode>,
        HashMap<String, BackendNodeId>,
    )> {
        let page = tab.page.clone();
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
                // Reversed so children pop in document order.
                for c in children.into_iter().rev() {
                    stack.push(c);
                }
            }
        }

        let mut all_flat = Vec::new();
        let mut all_backends = HashMap::new();
        for (i, frame_id) in frame_ids.iter().enumerate() {
            let resp = match page
                .execute(
                    GetFullAxTreeParams::builder()
                        .frame_id(frame_id.clone())
                        .build(),
                )
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
            let idx = if i == 0 {
                0
            } else {
                let next = tab.frame_index.len() + 1;
                *tab
                    .frame_index
                    .entry(frame_id.as_ref().to_string())
                    .or_insert(next)
            };
            let prefix = format!("f{idx}:");
            let (flat, backends) = Self::flatten(&prefix, resp.result.nodes);
            all_flat.extend(flat);
            all_backends.extend(backends);
        }
        Ok((all_flat, all_backends))
    }

    async fn box_center(&mut self, backend: BackendNodeId) -> Result<(f64, f64)> {
        let page = &self.tab().page;
        let resp = page
            .execute(
                GetBoxModelParams::builder()
                    .backend_node_id(backend)
                    .build(),
            )
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
            .execute(
                ResolveNodeParams::builder()
                    .backend_node_id(backend)
                    .build(),
            )
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
            let path = vakbrowse_stealth::mouse_path(
                self.pointer,
                (x, y),
                0x5EED_u64.wrapping_add(x as u64),
                18,
            );
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

    async fn extract_inner(&mut self, window: ExtractWindow) -> Result<Extracted> {
        const EXTRACT_JS: &str = r#"
(() => {
  // Candidate containers. Score = prose length discounted by link density:
  // link-dense blocks (nav lists, sidebars, link walls) are chrome, not
  // reading material, so a longer but link-heavy block must NOT win over a
  // shorter genuine article.
  const candidates = Array.from(document.querySelectorAll(
    'article, main, [role=main], .post, .entry-content, #content, #main,' +
    '.articlebody, .pagecontent, section'));
  const linkTextLen = (el) => {
    let n = 0;
    for (const a of (el.querySelectorAll ? el.querySelectorAll('a') : [])) {
      n += (a.textContent || '').length;
    }
    return n;
  };
  const score = (el) => {
    const txt = (el.innerText || '').length;
    if (txt === 0) return 0;
    const density = Math.min(1, linkTextLen(el) / txt);
    return txt * (1 - density);
  };
  let root = document.body;
  let bestScore = -1;
  for (const c of candidates) {
    const s = score(c);
    if (s > bestScore) { bestScore = s; root = c; }
  }
  // Card/div-layout pages often have no semantic container at all, or only a
  // small one (a promo <section>) beside the real content. If the winner
  // holds a small fraction of the page's prose, read the whole body instead —
  // its chrome is skipped below, so nav/link-lists still don't leak.
  if (root !== document.body && bestScore < 0.25 * score(document.body)) {
    root = document.body;
  }

  // Walk the LIVE tree (not a detached clone) so innerText and computed
  // display/visibility are layout-aware: hidden nodes are skipped, and CSS
  // block boxes split lines even when the markup is all div/span.
  const CHROME = 'script,style,noscript,template,nav,header,footer,aside,form,' +
    'button,[aria-hidden=true],[role=navigation],[role=banner],' +
    '[role=contentinfo],.mw-editsection';
  const LEAF = new Set(['p','li','blockquote','pre','td','th','dt','dd',
    'figcaption','caption']);
  const NESTED = 'table,h1,h2,h3,h4';
  const HAS_BLOCK = 'div,p,li,ul,ol,dl,table,section,article,main,' +
    'blockquote,pre,figure,h1,h2,h3,h4,h5,h6';
  const shown = (el) => !el.checkVisibility ||
    el.checkVisibility({ visibilityProperty: true });

  const lines = [];
  const push = (t) => { const x = t.replace(/\s+/g, ' ').trim(); if (x) lines.push(x); };
  // Inline run (text nodes + inline elements) accumulated until the next
  // block boundary, so `<div><span>quote</span> by <small>A</small></div>`
  // reads as one line instead of being dropped.
  let run = '';
  const flush = () => { push(run); run = ''; };
  const walk = (node) => {
    for (const child of node.childNodes) {
      if (child.nodeType === Node.TEXT_NODE) { run += child.data; continue; }
      if (child.nodeType !== Node.ELEMENT_NODE) continue;
      if (child.matches(CHROME) || !shown(child)) continue;
      const tag = child.tagName.toLowerCase();
      if (tag === 'br') {
        flush();
      } else if (/^h[1-6]$/.test(tag)) {
        flush();
        lines.push('');
        lines.push('#'.repeat(+tag[1]) + ' ' + child.innerText.trim());
        lines.push('');
      } else if (tag === 'tr' && !child.querySelector(NESTED)) {
        // One line per row (`Developer | The Rust Team`) keeps infobox labels
        // next to their values; layout tables recurse via NESTED instead.
        flush();
        push(Array.from(child.children)
          .filter(c => !c.matches(CHROME) && shown(c))
          .map(c => c.innerText.replace(/\s+/g, ' ').trim())
          .filter(Boolean).join(' | '));
      } else if (LEAF.has(tag) && !child.querySelector(NESTED)) {
        flush();
        push(child.innerText);
      } else {
        const display = getComputedStyle(child).display;
        if (display === 'contents') {
          walk(child);
        } else if (!display.startsWith('inline') || child.querySelector(HAS_BLOCK)) {
          flush();
          walk(child);
          flush();
        } else {
          // innerText is undefined on SVG/MathML; their text is not prose.
          run += child.innerText ?? '';
        }
      }
    }
  };
  walk(root);
  flush();
  // Full text; the requested window is cut in Rust (Extracted::windowed).
  return JSON.stringify({
    title: document.title,
    url: location.href,
    text: lines.join('\n').replace(/\n{3,}/g, '\n\n').trim()
  });
})()"#;

        let value = self.evaluate_json_on(&self.tab().page, EXTRACT_JS).await?;
        let parsed: serde_json::Value = match value {
            serde_json::Value::String(s) => serde_json::from_str(&s)
                .map_err(|e| VakError::Protocol(format!("extract shape: {e}")))?,
            other => other,
        };
        #[derive(serde::Deserialize)]
        struct Full {
            title: String,
            url: String,
            text: String,
        }
        let full: Full = serde_json::from_value(parsed)
            .map_err(|e| VakError::Protocol(format!("extract: {e}")))?;
        Ok(Extracted::windowed(
            full.title, full.url, &full.text, window,
        ))
    }

    async fn history_go(&mut self, expr: &str) -> Result<Navigated> {
        let page = self.tab().page.clone();
        page.evaluate(expr).await.map_err(proto_err)?;
        if let Err(e) = page.wait_for_navigation().await {
            // Reload/back destroys the inspected target mid-wait; that IS
            // the navigation succeeding.
            let msg = e.to_string();
            if !msg.contains("navigated or closed") {
                return Err(proto_err(e));
            }
        }
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
                && live != url
            {
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

    /// Call `function_declaration(this, ...args)` on the global object with
    /// structured JSON arguments — never string-interpolating caller data into
    /// JS source. Used for WebMCP invocation; resolves the global object's
    /// remote id and dispatches via `Runtime.callFunctionOn`.
    async fn call_on_global(
        &self,
        function_declaration: &str,
        args: Vec<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let page = &self.tab().page;
        let win = page
            .execute(
                EvaluateParams::builder()
                    .expression("globalThis")
                    .return_by_value(false)
                    .build()
                    .map_err(proto_err)?,
            )
            .await
            .map_err(proto_err)?;
        let object_id = win
            .result
            .result
            .object_id
            .ok_or_else(|| VakError::Protocol("globalThis has no objectId".into()))?;
        let call_args: Vec<CallArgument> = args
            .into_iter()
            .map(|v| CallArgument {
                value: Some(v),
                ..Default::default()
            })
            .collect();
        let resp = page
            .execute(
                CallFunctionOnParams::builder()
                    .function_declaration(function_declaration)
                    .object_id(object_id)
                    .arguments(call_args)
                    .return_by_value(true)
                    .await_promise(true)
                    .build()
                    .map_err(proto_err)?,
            )
            .await
            .map_err(proto_err)?;
        if resp.result.exception_details.is_some() {
            return Err(VakError::Protocol("callFunctionOn threw".into()));
        }
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
    const want = String(val);
    const opts = Array.from(el.options);
    // Exact value first, then the visible label (agents usually know the label).
    let opt = opts.find(o => o.value === want);
    if (!opt) {
      const w = want.trim().toLowerCase();
      opt = opts.find(o => o.label.trim().toLowerCase() === w || o.text.trim().toLowerCase() === w);
    }
    if (!opt) return false;
    el.value = opt.value;
    el.dispatchEvent(new Event('input', { bubbles: true }));
    el.dispatchEvent(new Event('change', { bubbles: true }));
    return el.value === opt.value;
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

/// Return the `href` of `this` only when it is a navigating anchor — i.e.
/// `<a href=...>` whose href is neither empty, a pure fragment (`#...`), nor a
/// `javascript:`/`mailto:`/`tel:` scheme. Everything else yields `""`, so the
/// post-click navigation probe is skipped (no latency cost on non-anchor or
/// non-navigating clicks).
const NAVIGATING_ANCHOR_HREF_JS: &str = r#"
function() {
  if ((this.tagName || '').toUpperCase() !== 'A') return '';
  const h = this.getAttribute('href');
  if (!h) return '';
  if (h.startsWith('#')) return '';
  try { const u = new URL(h, location.href); if (['javascript:','mailto:','tel:'].includes(u.protocol)) return ''; }
  catch { return ''; }
  return h;
}
"#;

/// How long a root-frame anchor click waits for the navigation it *should*
/// have triggered before being reported as not-navigated. Only paid by
/// navigating-anchor clicks; the bot-wall shape (Bing/DDG click the link but
/// never fire a navigation) pays this once per recovery attempt. Tuned small
/// so agent click loops don't stall on non-navigating anchors.
const CLICK_NAV_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(1);
/// Per-attempt budget for the recovery fallbacks (DOM `.click()` then forced
/// `location.href =`) before we give up on a bot-walled anchor click.
const CLICK_RETRY_TIMEOUT: std::time::Duration = std::time::Duration::from_millis(600);

/// How long a click-driven navigation waits for the landed document's
/// `readyState === 'complete'` before returning anyway (slow third-party
/// subresources must not stall the agent indefinitely).
const CLICK_LOAD_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);

/// Dispatch input + change events on a file input after setting its files.
const SET_FILES_EVENTS_JS: &str = r#"() => {
    this.dispatchEvent(new Event('input', { bubbles: true }));
    this.dispatchEvent(new Event('change', { bubbles: true }));
    return true;
}"#;

#[async_trait::async_trait]
impl PageOps for CdpSession {
    async fn navigate(&mut self, url: &str) -> Result<Navigated> {
        let validated = validate_url(url)?;
        self.maybe_human_jitter().await;
        self.tab()
            .page
            .goto(validated.as_str())
            .await
            .map_err(proto_err)?;

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
        self.maybe_human_jitter().await;
        // Mirror `evaluate_json_on`: ask for the value by-value and await
        // promises, then stringify. The convenience `page.evaluate().into_value()`
        // both (a) crashes on non-string primitives ("invalid type: boolean …")
        // and (b) errors "No value found" for results whose remote object has no
        // inline value — which makes the canonical `navigator.webdriver` stealth
        // check unobservable. Stringifying the raw JSON value fixes both.
        let resp = self
            .tab()
            .page
            .execute(
                EvaluateParams::builder()
                    .expression(expression)
                    .return_by_value(true)
                    .await_promise(true)
                    .build()
                    .map_err(proto_err)?,
            )
            .await
            .map_err(proto_err)?;
        let remote = resp.result.result;
        // Inline JSON value covers primitives and JSON-serializable
        // objects/arrays (requested via `return_by_value`). Unserializable
        // primitives (BigInt, -0, NaN, Infinity) arrive under
        // `unserializableValue` instead — JSON cannot represent them, so we
        // surface the CDP string form rather than collapsing them to "null".
        if let Some(value) = remote.value {
            return Ok(stringify_js(value));
        }
        if let Some(unserializable) = remote.unserializable_value {
            return Ok(unserializable.as_ref().to_string());
        }
        // No inline value at all: undefined / function / symbol, or an object
        // handed back only by identity (objectId). Mirror a JS REPL rather than
        // mis-reporting `undefined` as "null".
        Ok(match remote.r#type {
            RemoteObjectType::Undefined => "undefined".to_string(),
            RemoteObjectType::Symbol => remote
                .description
                .clone()
                .unwrap_or_else(|| "[symbol]".to_string()),
            RemoteObjectType::Function => remote
                .description
                .clone()
                .unwrap_or_else(|| "[function]".to_string()),
            // CDP models JS `null` as type "object" with no inline value and no
            // objectId. A real object is inlined when return_by_value is set; the
            // only object left with value=None is therefore `null`.
            RemoteObjectType::Object if remote.object_id.is_none() => "null".to_string(),
            _ => "[object Object]".to_string(),
        })
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
        let (flat, backends) = Self::collect_frames(self.tab_mut()).await?;
        let build =
            vakbrowse_perception::build_snapshot(&url, &title, &flat, &mut self.tab_mut().refs);
        let state = self.tab_mut();
        state.ref_to_ax = build.ref_to_ax;
        state.ax_to_backend = backends;
        Ok(build.snapshot)
    }

    async fn find_by_css(&mut self, selector: &str) -> Result<Vec<ElementRef>> {
        self.maybe_human_jitter().await;
        // Snapshot has the side effect of populating `ref_to_ax` + `ax_to_backend`
        // for the current document (and reconciling a click-driven navigation
        // that bypassed navigate()). We reuse that map to turn querySelectorAll
        // results into the SAME `@eN` refs `click`/`fill` resolve.
        self.snapshot().await?;
        // Invert ref→ax→backend into backend→ref so each matched DOM node can
        // be looked up by its BackendNodeId.
        let backend_to_ref = {
            let tab = self.tab();
            let mut m = HashMap::with_capacity(tab.ref_to_ax.len());
            for (ref_, ax) in &tab.ref_to_ax {
                if let Some(b) = tab.ax_to_backend.get(ax).copied() {
                    m.insert(b, ref_.clone());
                }
            }
            m
        };
        let page = &self.tab().page;
        let elems = page.find_elements(selector).await.map_err(proto_err)?;
        let mut out = Vec::with_capacity(elems.len());
        for e in elems {
            if let Some(r) = backend_to_ref.get(&e.backend_node_id) {
                out.push(r.clone());
            }
        }
        // `find_elements` returns root-document matches in DOM order, but the
        // snapshot's `@eN` are numbered over the AX tree (interactive + non-
        // ignored), so the two orderings can diverge. We return whatever refs
        // resolve; agents index by ref, not position.
        Ok(out)
    }

    async fn click(&mut self, r: &ElementRef) -> Result<ClickResult> {
        let mut out = self.click_inner(r).await?;
        if let Some(tab) = self.sync_tabs().await.into_iter().next() {
            out.opened_tab = Some(tab.0);
        }
        Ok(out)
    }

    async fn fill(&mut self, r: &ElementRef, text: &str) -> Result<()> {
        self.maybe_human_jitter().await;
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
            serde_json::Value::String(s) => {
                if s == "no-element" {
                    Err(VakError::NotFound(format!("select_option: stale ref {r}")))
                } else if let Some(tag) = s.strip_prefix("wrong-tag:") {
                    Err(VakError::Unsupported(format!(
                        "select_option expects a <select>, got <{tag}> (ref {r})"
                    )))
                } else if let Some(msg) = s.strip_prefix("error:") {
                    Err(VakError::Engine(format!("select_option: JS error: {msg}")))
                } else {
                    Err(VakError::Engine(format!(
                        "select_option: unexpected JS return: {s}"
                    )))
                }
            }
            other => Err(VakError::Engine(format!(
                "select_option: unexpected JS return type: {other}"
            ))),
        }
    }

    async fn press_key(&mut self, key: &str) -> Result<()> {
        self.maybe_human_jitter().await;
        let (mods, base) = parse_key_combo(key)?;
        let (code, key_name, vk, text): (String, String, i64, Option<String>) = match base.as_str()
        {
            "Enter" => ("Enter".into(), "Enter".into(), 13, Some("\r".into())),
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
                let code = match c {
                    ' ' => "Space".to_string(),
                    c if c.is_ascii_alphabetic() => format!("Key{}", c.to_ascii_uppercase()),
                    c if c.is_ascii_digit() => format!("Digit{c}"),
                    other => other.to_string(),
                };
                let shown = if mods & MOD_SHIFT != 0 {
                    c.to_uppercase().to_string()
                } else {
                    k.to_string()
                };
                // Ctrl/Alt/Meta chords are commands, not typed characters.
                let typed = (mods & (MOD_CTRL | MOD_ALT | MOD_META) == 0).then(|| shown.clone());
                (code, shown, c.to_ascii_uppercase() as i64, typed)
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
        // Headless has no OS key bindings: editing shortcuts must be named.
        let commands: Vec<&str> = if mods & (MOD_CTRL | MOD_META) != 0 {
            match (base.to_ascii_lowercase().as_str(), mods & MOD_SHIFT != 0) {
                ("a", _) => vec!["selectAll"],
                ("c", _) => vec!["copy"],
                ("x", _) => vec!["cut"],
                ("v", _) => vec!["paste"],
                ("z", false) => vec!["undo"],
                ("z", true) | ("y", _) => vec!["redo"],
                _ => vec![],
            }
        } else {
            vec![]
        };

        let mut down = DispatchKeyEventParams::builder()
            .r#type(key_type)
            .modifiers(mods)
            .key(key_name.clone())
            .code(code.clone())
            .windows_virtual_key_code(vk)
            .text(text.clone().unwrap_or_default());
        if !commands.is_empty() {
            down = down.commands(commands);
        }
        self.tab()
            .page
            .execute(down.build().map_err(proto_err)?)
            .await
            .map_err(proto_err)?;

        let up = DispatchKeyEventParams::builder()
            .r#type(DispatchKeyEventType::KeyUp)
            .modifiers(mods)
            .key(key_name)
            .code(code)
            .windows_virtual_key_code(vk)
            .build()
            .map_err(proto_err)?;
        self.tab().page.execute(up).await.map_err(proto_err)?;
        Ok(())
    }

    async fn scroll(&mut self, dx: f64, dy: f64) -> Result<()> {
        self.maybe_human_jitter().await;
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

    async fn wait_for_url(&self, pattern: &str, timeout_ms: u64) -> Result<()> {
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_millis(timeout_ms);
        loop {
            // location.href is a string; poll it (not readyState — which stays
            // 'complete' across SPA navigations) so the URL match is reliable.
            let href = self
                .tab()
                .page
                .evaluate("location.href")
                .await
                .ok()
                .and_then(|r| r.into_value::<String>().ok())
                .unwrap_or_default();
            if href.contains(pattern) {
                return Ok(());
            }
            if tokio::time::Instant::now() >= deadline {
                return Err(VakError::Timeout(format!(
                    "wait_for_url: {pattern} (href={href})"
                )));
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
                same_site: c.same_site.map(|s| s.as_ref().to_string()),
                // CDP reports session cookies with expires == -1.
                expires: (c.expires > 0.0).then_some(c.expires),
            })
            .collect())
    }

    async fn set_cookie(&mut self, cookie: &CookieInput) -> Result<()> {
        let mut builder = CookieParam::builder()
            .name(cookie.name.clone())
            .value(cookie.value.clone())
            .domain(cookie.domain.clone())
            .path(cookie.path.clone())
            .secure(cookie.secure)
            .http_only(cookie.http_only);
        if let Some(exp) = cookie.expires {
            builder = builder.expires(TimeSinceEpoch::new(exp));
        }
        if let Some(ss) = &cookie.same_site {
            builder =
                match ss.parse::<chromiumoxide::cdp::browser_protocol::network::CookieSameSite>() {
                    Ok(v) => builder.same_site(v),
                    Err(_) => {
                        tracing::warn!(same_site = %ss, "unrecognized SameSite value; ignoring");
                        builder
                    }
                };
        }
        let param = builder.build().map_err(proto_err)?;
        self.tab()
            .page
            .execute(
                SetCookiesParams::builder()
                    .cookies(vec![param])
                    .build()
                    .map_err(proto_err)?,
            )
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

    async fn set_download_dir(&mut self, dir: &Path) -> Result<()> {
        self.download_dir = Some(dir.to_path_buf());
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
        self.maybe_human_jitter().await;
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
        let args = if args.is_null() {
            serde_json::json!({})
        } else {
            args
        };

        // The function is a fixed template; `name` and `args` are passed as
        // structured CDP call arguments (never interpolated into JS source),
        // so page/agent-supplied tool names cannot inject JS.
        let v = match self
            .call_on_global(
                "function(name, args) { \
                  const mc = navigator.modelContext; \
                  if (!mc || typeof mc.callTool !== 'function') \
                    throw new Error('webmcp unavailable on this page'); \
                  return Promise.resolve(mc.callTool(name, args)).then(v => JSON.stringify(v)); \
                }",
                vec![serde_json::json!(name), args],
            )
            .await
        {
            Ok(v) => v,
            Err(e) => {
                return Err(VakError::Unsupported(format!(
                    "WebMCP tool {name:?} not invokable ({e})"
                )));
            }
        };
        Ok(match v {
            serde_json::Value::String(s) => s,
            other => other.to_string(),
        })
    }

    async fn extract(&mut self, window: ExtractWindow) -> Result<Extracted> {
        self.extract_inner(window).await
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

    async fn tabs(&mut self) -> Result<Vec<TabInfo>> {
        self.sync_tabs().await;
        for t in self.tabs.values_mut() {
            if let Ok(Some(url)) = t.page.url().await {
                t.current_url = url;
            }
        }
        let mut list: Vec<TabInfo> = self
            .tabs
            .iter()
            .map(|(id, t)| TabInfo {
                id: id.clone(),
                url: t.current_url.clone(),
            })
            .collect();
        // Numeric order (t2 before t10), active tab first.
        list.sort_by_key(|t| t.id.0.trim_start_matches('t').parse::<u64>().unwrap_or(u64::MAX));
        if let Some(pos) = list.iter().position(|t| t.id == self.active) {
            let active = list.remove(pos);
            list.insert(0, active);
        }
        Ok(list)
    }

    async fn export_state(&mut self) -> Result<SessionState> {
        // Browser-internal pages are never agent-opened (validate_url refuses
        // chrome:), so they are not worth rebuilding.
        let tabs: Vec<TabInfo> = self
            .tabs()
            .await?
            .into_iter()
            .filter(|t| !t.url.starts_with("chrome://") && !t.url.starts_with("chrome-error://"))
            .collect();
        Ok(SessionState {
            tabs,
            active: Some(self.active.clone()),
            cookies: self.cookies().await?,
        })
    }

    async fn import_state(&mut self, state: &SessionState) -> Result<()> {
        for c in &state.cookies {
            let input = CookieInput {
                name: c.name.clone(),
                value: c.value.clone(),
                domain: c.domain.clone(),
                path: c.path.clone(),
                secure: c.secure,
                http_only: c.http_only,
                same_site: c.same_site.clone(),
                expires: c.expires,
            };
            if let Err(e) = self.set_cookie(&input).await {
                tracing::warn!(cookie = %c.name, error = %e, "cookie not restored");
            }
        }
        // Restore under the original ids so tab ids an agent holds stay valid.
        let mut restored: HashMap<TabId, TabState> = HashMap::new();
        let mut max_n = 1;
        for (i, tab) in state.tabs.iter().enumerate() {
            let fresh = if i == 0 {
                self.active.clone()
            } else {
                self.new_tab(None).await?.id
            };
            let mut ts = self
                .tabs
                .remove(&fresh)
                .ok_or_else(|| VakError::Engine("restore: tab vanished".into()))?;
            if tab.url != "about:blank" && !tab.url.is_empty() {
                match ts.page.goto(tab.url.as_str()).await {
                    Ok(_) => ts.current_url = tab.url.clone(),
                    Err(e) => tracing::warn!(url = %tab.url, error = %e, "tab not restored"),
                }
            }
            if let Some(n) = tab.id.0.strip_prefix('t').and_then(|n| n.parse::<u64>().ok()) {
                max_n = max_n.max(n);
            }
            restored.insert(tab.id.clone(), ts);
        }
        if restored.is_empty() {
            return Ok(());
        }
        // Any tab the fresh browser had beyond the restored set is dropped.
        self.tabs = restored;
        self.next_tab = max_n + 1;
        self.active = state
            .active
            .clone()
            .filter(|a| self.tabs.contains_key(a))
            .or_else(|| self.tabs.keys().next().cloned())
            .unwrap_or_else(|| TabId("t1".into()));
        self.pointer = (0.0, 0.0);
        Ok(())
    }

    async fn new_tab(&mut self, url: Option<&str>) -> Result<TabInfo> {
        let id = TabId(format!("t{}", self.next_tab));
        self.next_tab += 1;

        let page = self
            .browser
            .new_page("about:blank")
            .await
            .map_err(proto_err)?;
        if let Some(stealth) = &self.stealth {
            apply_stealth(&page, stealth).await?;
        }

        let mut state = TabState {
            page,
            current_url: "about:blank".to_string(),
            refs: vakbrowse_perception::RefBook::new(),
            ref_to_ax: HashMap::new(),
            ax_to_backend: HashMap::new(),
            frame_index: HashMap::new(),
        };

        if let Some(url) = url {
            let validated = validate_url(url)?;
            state
                .page
                .goto(validated.as_str())
                .await
                .map_err(proto_err)?;
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
            self.sync_tabs().await;
        }
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

    async fn set_file_chooser(&mut self, r: &ElementRef, paths: &[String]) -> Result<bool> {
        let backend = match self.backend_for(r) {
            Ok(b) => b,
            Err(VakError::NotFound(_)) => return Ok(false),
            Err(e) => return Err(e),
        };
        // DOM.setFileInputFiles bypasses browser security that would block
        // assigning to <input type=file>.files directly. This works even
        // when the element is in a detached/hidden state.
        self.tab().page.execute(
            SetFileInputFilesParams::builder()
                .files(paths.to_vec())
                .backend_node_id(backend)
                .build()
                .map_err(proto_err)?,
        )
        .await
        .map_err(proto_err)?;
        // Dispatch input + change events so framework listeners react.
        self.call_on_element(backend, SET_FILES_EVENTS_JS, vec![])
            .await?;
        Ok(true)
    }

    async fn source(&self) -> Result<String> {
        let html = self
            .tab()
            .page
            .evaluate("document.documentElement.outerHTML")
            .await
            .map_err(proto_err)?;
        let val: String = html.into_value().map_err(proto_err)?;
        Ok(val)
    }

    async fn downloads(&mut self) -> Result<Vec<DownloadInfo>> {
        let dir = match &self.download_dir {
            Some(d) => d.clone(),
            None => return Ok(Vec::new()),
        };
        let mut out = Vec::new();
        if let Ok(entries) = std::fs::read_dir(&dir) {
            for entry in entries.flatten() {
                if let Ok(meta) = entry.metadata()
                    && meta.is_file()
                {
                    out.push(DownloadInfo {
                        path: entry.path().to_string_lossy().to_string(),
                        bytes: meta.len(),
                    });
                }
            }
        }
        Ok(out)
    }
}

/// Wall-clock jitter in [20, 150] ms used by `human_timing` to break the
/// robotic cadence agents otherwise expose.
///
/// The old implementation derived the delay from `subsec_nanos() % 130`, which
/// is wall-clock-derived and highly correlated for consecutive calls in a tight
/// agent loop (the nanosecond field advances linearly, so the modulo cycles
/// through a predictable sequence). This version mixes a high-resolution
/// timestamp with a thread-local call counter through a splitmix-style
/// finalizer, producing well-distributed, less-correlated delays — no RNG
/// dependency required. Timing is not cryptographically secret.
fn human_jitter() -> std::time::Duration {
    use std::time::{SystemTime, UNIX_EPOCH};
    thread_local! {
        static COUNTER: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
    }
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0);
    let counter = COUNTER.with(|c| {
        let v = c.get();
        c.set(v.wrapping_add(1));
        v
    });
    // Splitmix64 finalizer: spreads input entropy across all 64 bits.
    let mut s = now.wrapping_add(counter);
    s = s.wrapping_mul(0x9e3779b97f4a7c15);
    s ^= s >> 30;
    s = s.wrapping_mul(0xbf58476d1ce4e5b9);
    s ^= s >> 27;
    s = s.wrapping_mul(0x94d049bb133111eb);
    s ^= s >> 31;
    let ms = 30 + (s % 171); // [30, 200]
    std::time::Duration::from_millis(ms)
}

impl CdpSession {
    /// Reconcile the tab registry with the browser's real targets: adopt tabs
    /// the page opened (returned, in order) and drop tabs that were closed.
    /// A failed target listing leaves the registry untouched.
    async fn sync_tabs(&mut self) -> Vec<TabId> {
        let Ok(pages) = self.browser.pages().await else {
            return Vec::new();
        };
        if pages.is_empty() {
            return Vec::new();
        }
        let live: std::collections::HashSet<String> = pages
            .iter()
            .map(|p| p.target_id().as_ref().to_string())
            .collect();
        let known: std::collections::HashSet<String> = self
            .tabs
            .values()
            .map(|t| t.page.target_id().as_ref().to_string())
            .collect();

        let mut adopted = Vec::new();
        for page in pages {
            if known.contains(page.target_id().as_ref() as &str) {
                continue;
            }
            if let Some(stealth) = &self.stealth {
                let _ = apply_stealth(&page, stealth).await;
            }
            let url = page
                .url()
                .await
                .ok()
                .flatten()
                .unwrap_or_else(|| "about:blank".into());
            let id = TabId(format!("t{}", self.next_tab));
            self.next_tab += 1;
            self.tabs.insert(
                id.clone(),
                TabState {
                    page,
                    current_url: url,
                    refs: vakbrowse_perception::RefBook::new(),
                    ref_to_ax: HashMap::new(),
                    ax_to_backend: HashMap::new(),
                    frame_index: HashMap::new(),
                },
            );
            tracing::info!(tab = %id, "adopted page-opened tab");
            adopted.push(id);
        }

        if self.tabs.len() > 1 {
            let gone: Vec<TabId> = self
                .tabs
                .iter()
                .filter(|(_, t)| !live.contains(t.page.target_id().as_ref() as &str))
                .map(|(id, _)| id.clone())
                .collect();
            for id in gone {
                if self.tabs.len() > 1 {
                    self.tabs.remove(&id);
                    tracing::info!(tab = %id, "dropped tab closed by the page");
                }
            }
            if !self.tabs.contains_key(&self.active)
                && let Some(next) = self.tabs.keys().next().cloned()
            {
                self.active = next;
                self.pointer = (0.0, 0.0);
            }
        }
        adopted
    }

    async fn click_inner(&mut self, r: &ElementRef) -> Result<ClickResult> {
        self.maybe_human_jitter().await;
        let ax_id = self
            .tab()
            .ref_to_ax
            .get(r)
            .ok_or_else(|| VakError::NotFound(format!("stale element ref {r}")))?
            .clone();
        let backend = self.backend_for(r)?;

        // Root frame: trusted input at page-level coordinates.
        if ax_id.starts_with("f0:") {
            // Classify the target *before* dispatching input: a navigation that
            // the click triggers swaps the document and makes the resolved
            // `backend` (a BackendNodeId) invalid, so we must read the href
            // before the mouse events fire. Only `<a href>` with a real,
            // cross-document href is expected to navigate — everything else
            // (inputs, buttons, `javascript:`/fragment anchors) is treated as
            // non-navigating and returns immediately.
            let href = self.navigating_href(backend).await?;
            // Scroll the target into view first. DOM.getBoxModel returns
            // viewport-relative coords, so an element below the fold (e.g. a
            // Bing search result) would otherwise be clicked at a viewport
            // point where nothing is rendered and the click silently misses.
            // scrollIntoView updates the scroll offset so the fresh box-model
            // center lands on the visible element.
            let _ = self
                .call_on_element(
                    backend,
                    "function(){this.scrollIntoView({block:'center'});return true;}",
                    vec![],
                )
                .await;
            // Capture the URL *before* dispatching input: a navigation that
            // the click triggers swaps the document (invalidating `backend`)
            // and mutates the URL, so the before/after comparison is the only
            // reliable signal — `wait_for_navigation`'s result is not (it can
            // resolve spuriously on a non-navigating click).
            let before = self
                .tab()
                .page
                .url()
                .await
                .map_err(proto_err)?
                .unwrap_or_default();
            let (cx, cy) = self.box_center(backend).await?;
            self.click_at(cx, cy).await?;
            // Surface whether an anchor click navigated. Snapshot also self-heals
            // the URL on the next call, but reconciling now lets a following
            // wait_url/extract see the new page immediately — and, crucially,
            // lets the agent *observe* a silent bot wall (Bing/DDG accept the
            // click but never fire a navigation). `href` is `Some` only for
            // real `<a href>` anchors; everything else returns `stayed()` in O(1).
            return if let Some(href) = href {
                self.probe_click_navigation(backend, href, before).await
            } else {
                Ok(ClickResult::stayed())
            };
        }
        // Child frames: DOM click on the resolved element. We can't dispatch
        // trusted mouse events to child frames (box coords are frame-relative),
        // but a DOM `.click()` fires the element's handlers. After it fires,
        // check for a top-level URL change — this catches `_top`-targeting
        // anchors and JS-driven `location.href` assignments that originate in
        // the iframe. Same-frame iframe navigations (the iframe's URL changes
        // but the top-level URL does not) remain invisible — a documented
        // limitation, since detecting them would require per-frame URL queries.
        let page = self.tab().page.clone();
        let before = page.url().await.map_err(proto_err)?.unwrap_or_default();
        let ok = self
            .call_on_element(backend, FRAME_CLICK_JS, vec![])
            .await?;
        let _ = ok;
        if let Some(url) = Self::wait_url_change(&page, &before, CLICK_NAV_TIMEOUT).await {
            return self.reconcile_click_navigation(url).await;
        }
        Ok(ClickResult::stayed())
    }

    /// Wait (bounded) for the document a click navigated to to finish loading.
    /// The URL flips at commit, long before render-blocking stylesheets
    /// arrive; returning then hands the agent an unpainted, about-to-reflow
    /// document, and its next trusted click is dropped or misses. Mirrors `navigate()`, which waits for
    /// load via `goto`. Evaluation errors (context swapped mid-poll) retry.
    async fn wait_document_loaded(page: &Page, budget: std::time::Duration) {
        let deadline = tokio::time::Instant::now() + budget;
        loop {
            if let Ok(v) = page.evaluate("document.readyState").await
                && v.into_value::<String>().is_ok_and(|s| s == "complete")
            {
                return;
            }
            if tokio::time::Instant::now() >= deadline {
                tracing::debug!("click-navigated document still loading after {budget:?}");
                return;
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
    }

    /// The `href` of `backend` iff it is a navigating anchor (`<a href>` whose
    /// href is neither empty/fragment/`javascript:`/`mailto:`/`tel:`), else
    /// `None`. Cheap and read-only — safe to call before input is dispatched
    /// (calling it after a navigation swaps the document and invalidates the
    /// BackendNodeId).
    async fn navigating_href(&mut self, backend: BackendNodeId) -> Result<Option<String>> {
        let href = self
            .call_on_element(backend, NAVIGATING_ANCHOR_HREF_JS, vec![])
            .await?;
        match href {
            serde_json::Value::String(s) if !s.is_empty() => Ok(Some(s)),
            _ => Ok(None),
        }
    }

    /// Detect whether an anchor click navigated and, when `click_recovery` is
    /// enabled, escalate through progressively less-synthetic navigation on a
    /// bot wall. The agent always gets a `url`
    /// either way: `navigated:true` with the landed page, or `navigated:false`
    /// (after recovery is exhausted) so the branch is explicit.
    ///
    /// Recovery ladder (each attempt re-checks for a URL change — the only
    /// reliable signal; `wait_for_navigation` is ignored as it resolves
    /// spuriously on non-navigations):
    ///  1. the trusted mouse click already dispatched in `click`,
    ///  2. a ground-truth DOM `element.click()`,
    ///  3. a forced `location.href = <href>` assignment — the one move known to
    ///     defeat Bing/DDG's click-blocking (they intercept trusted-input
    ///     events but not a direct navigation assignment).
    async fn probe_click_navigation(
        &mut self,
        backend: BackendNodeId,
        href: String,
        before: String,
    ) -> Result<ClickResult> {
        // 1. Trust the initial mouse click.
        let page = self.tab().page.clone();
        if let Some(url) = Self::wait_url_change(&page, &before, CLICK_NAV_TIMEOUT).await {
            return self.reconcile_click_navigation(url).await;
        }

        if !self.click_recovery {
            // The trusted click already ran the element's handlers once;
            // re-firing them is only safe when the caller opted in.
            tracing::warn!(
                "click on <a href={href}> produced no navigation; recovery is off (open with click_recovery to force it)"
            );
            return Ok(ClickResult::stayed());
        }

        // 2. Ground-truth DOM click on the resolved element.
        tracing::warn!(
            "click on <a href={href}> produced no navigation within {:?}; escalating to DOM .click()",
            CLICK_NAV_TIMEOUT
        );
        let _ = self.call_on_element(backend, FRAME_CLICK_JS, vec![]).await;
        if let Some(url) = Self::wait_url_change(&page, &before, CLICK_RETRY_TIMEOUT).await {
            return self.reconcile_click_navigation(url).await;
        }

        // 3. Forced navigation: `location.href = <href>`. JSON-encodes the
        // href into the expression so a page-controlled attribute value cannot
        // inject JS (it is passed as a string literal, never interpolated raw).
        let json_href = serde_json::to_string(&href)
            .map_err(|e| VakError::Protocol(format!("can't encode href for nav: {e}")))?;
        let expr = format!("location.href = {json_href};");
        let _ = self
            .tab()
            .page
            .evaluate(expr.as_str())
            .await
            .map_err(proto_err);
        if let Some(url) = Self::wait_url_change(&page, &before, CLICK_RETRY_TIMEOUT).await {
            return self.reconcile_click_navigation(url).await;
        }

        // All three moves blocked (hard behavioral/TLS wall). Report honestly —
        // the agent must rotate proxy/profile, not spin retrying.
        tracing::warn!(
            "click on <a href={href}> produced no navigation after mouse + DOM .click() + location.href (bot wall / JS handler)"
        );
        Ok(ClickResult::stayed())
    }

    /// Poll `page.url()` until it differs from `before` (and is non-empty) or
    /// `budget` elapses. Returns the landed URL, if any.
    async fn wait_url_change(
        page: &Page,
        before: &str,
        budget: std::time::Duration,
    ) -> Option<String> {
        let deadline = tokio::time::Instant::now() + budget;
        loop {
            if let Ok(Some(after)) = page.url().await
                && after != before
            {
                return Some(after);
            }
            if tokio::time::Instant::now() >= deadline {
                return None;
            }
            tokio::time::sleep(std::time::Duration::from_millis(25)).await;
        }
    }

    /// Reconcile session state after an observed click-driven navigation:
    /// adopt the live URL and start a fresh ref turn (refs map to a document).
    async fn reconcile_click_navigation(&mut self, url: String) -> Result<ClickResult> {
        let page = self.tab().page.clone();
        Self::wait_document_loaded(&page, CLICK_LOAD_TIMEOUT).await;
        let url = match page.url().await {
            Ok(Some(live)) if !live.is_empty() => live,
            _ => url,
        };
        let state = self.tab_mut();
        state.current_url = url.clone();
        state.refs.reset();
        state.ref_to_ax.clear();
        state.ax_to_backend.clear();
        // A document change makes the last pointer position meaningless.
        self.pointer = (0.0, 0.0);
        Ok(ClickResult::navigated(url))
    }
}

#[cfg(test)]
mod key_tests {
    use super::*;

    #[test]
    fn key_combos() {
        assert_eq!(parse_key_combo("Enter").unwrap(), (0, "Enter".into()));
        assert_eq!(parse_key_combo("a").unwrap(), (0, "a".into()));
        assert_eq!(parse_key_combo("+").unwrap(), (0, "+".into()));
        assert_eq!(parse_key_combo("Control+a").unwrap(), (2, "a".into()));
        assert_eq!(parse_key_combo("ctrl+Shift+Tab").unwrap(), (10, "Tab".into()));
        assert_eq!(parse_key_combo("Meta++").unwrap(), (4, "+".into()));
        assert!(parse_key_combo("Hyper+a").is_err());
    }
}

#[cfg(test)]
mod jitter_tests {
    use super::human_jitter;
    #[test]
    fn human_jitter_stays_in_band() {
        for _ in 0..100 {
            let ms = human_jitter().as_millis();
            assert!((30..=200).contains(&ms), "jitter {ms}ms out of band");
        }
    }
    #[test]
    fn human_jitter_not_constant_under_load() {
        // The splitmix64 mixer should produce diverse values even when called
        // in a tight loop (the old `subsec_nanos % 130` would cycle
        // predictably). Check that we see at least 5 distinct values in 50
        // calls — a weak but hermetic signal of de-correlation.
        let mut distinct = std::collections::HashSet::new();
        for _ in 0..50 {
            distinct.insert(human_jitter().as_millis());
        }
        assert!(
            distinct.len() >= 5,
            "jitter too uniform: only {distinct:?} distinct values in 50 calls"
        );
    }
}
