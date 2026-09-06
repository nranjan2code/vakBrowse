//! Session manager + command model shared by the daemon, CLI and MCP
//! server. Every surface speaks the same `Request`/`Response` language, so
//! behavior is identical whether an agent embeds the manager in-process or
//! talks to a running daemon over a socket.

pub mod render;
#[cfg(unix)]
pub mod uds;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;
use vakbrowse_core::{
    Cookie, CookieInput, ElementRef, Extracted, ProfileId, Result, SessionId, Snapshot, TabId,
    TabInfo, VakError, WebMcpTool,
};
#[cfg(feature = "dom-backend")]
use vakbrowse_dom::DomLauncher;
use vakbrowse_engine::{CdpLauncher, EngineLauncher, LaunchOptions, Navigated, PageOps};

/// Which engine backend opens a session. Default = CDP (Chrome); `dom` selects
/// the experimental pure-Rust QuickJS DOM backend (no Chrome process —
/// `backend = "dom"` requires the `dom-backend` feature to be compiled in).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum Backend {
    #[default]
    Cdp,
    Dom,
}

/// Options for opening a new session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionOptions {
    /// Persistent profile id; maps to a dedicated Chromium user-data dir so
    /// authenticated sessions survive restarts. None = throwaway profile.
    #[serde(default)]
    pub profile: Option<ProfileId>,
    #[serde(default = "default_true")]
    pub headless: bool,
    #[serde(default)]
    pub url: Option<String>,
    /// Engine backend: `"cdp"` (Chrome) or `"dom"` (experimental pure-Rust
    /// QuickJS DOM backend, no Chrome). Requires the `dom-backend` feature on
    /// the server crate. `None` => use the server's default (`VAKBROWSE_BACKEND`,
    /// CDP unless overridden) so operators can run a chrome-free server.
    #[serde(default)]
    pub backend: Option<Backend>,
    /// When set, launch with a deterministic stealth fingerprint derived
    /// from this seed (defeats navigator.webdriver exposure, missing
    /// language/plugin data, robotic pointer teleports).
    #[serde(default)]
    pub stealth_seed: Option<String>,
    /// Chromium proxy for this session (`http://user:pass@host:port`,
    /// `socks5://host:port`), answering IP-reputation walls like DDG's.
    #[serde(default)]
    pub proxy: Option<String>,
    /// Ordered proxy pool; `RotateProxy` re-launches the browser on the next
    /// endpoint in this list (cycling). The honest path around IP-reputation
    /// walls: rotate, retry, then fall back to a vision click — no TLS spoofing.
    #[serde(default)]
    pub proxies: Vec<String>,
    /// Inject small randomized delays before input actions so the agent's
    /// timing isn't robotic (cadence-based behavioral tell). Off by default.
    #[serde(default)]
    pub human_timing: bool,
}

fn default_true() -> bool {
    true
}

impl Default for SessionOptions {
    fn default() -> Self {
        Self {
            profile: None,
            headless: true,
            url: None,
            backend: None,
            stealth_seed: None,
            proxy: None,
            proxies: Vec::new(),
            human_timing: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionInfo {
    pub id: SessionId,
    pub profile: Option<ProfileId>,
    pub url: String,
}

/// One agent-issued browser action. Serde-tagged so it doubles as wire JSON.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Action {
    Navigate {
        url: String,
    },
    Snapshot,
    Click {
        r#ref: String,
    },
    Fill {
        r#ref: String,
        text: String,
    },
    SelectOption {
        r#ref: String,
        value: String,
    },
    PressKey {
        key: String,
    },
    Scroll {
        dx: f64,
        dy: f64,
    },
    EvalText {
        expression: String,
    },
    /// CSS selector → stable `@eN` refs against the current snapshot's element
    /// set. Agents find a clickable handle without an intermediate snapshot.
    FindByCss {
        selector: String,
    },
    WaitForTruthy {
        expression: String,
        timeout_ms: u64,
    },
    /// SPA-safe URL wait: poll `location.href` for a substring. Agents should
    /// use this instead of `wait_for_readyState`, which never fires on SPAs.
    WaitForUrl {
        pattern: String,
        timeout_ms: u64,
    },
    Cookies,
    SetCookie {
        cookie: CookieInput,
    },
    ClearCookies,
    SetDownloadDir {
        dir: String,
    },
    /// Vision fallback: PNG (base64 in the response).
    Screenshot {
        full_page: bool,
    },
    /// Vision fallback: click raw viewport coordinates.
    ClickAt {
        x: f64,
        y: f64,
    },
    /// Rotate this session to the next proxy in its `SessionOptions.proxies`
    /// pool (re-launches the browser, preserves profile + stealth). The
    /// honest path around IP-reputation walls; requires a pool of 2+ endpoints.
    RotateProxy,
    /// List tools the page declares via WebMCP (`navigator.modelContext`).
    WebMcpTools,
    /// Invoke a page-declared WebMCP tool.
    WebMcpInvoke {
        name: String,
        arguments_json: String,
    },

    Back,
    Forward,
    Reload,
    /// Readable main-content extraction (title + markdown-ish text).
    Extract,

    Tabs,
    NewTab {
        url: Option<String>,
    },
    SwitchTab {
        tab: TabId,
    },
    CloseTab {
        tab: TabId,
    },
}

/// What an action produced. All variants are struct-style because
/// internally-tagged enums cannot encode primitive/seq newtype payloads.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ActionResult {
    Navigated {
        url: String,
        title: String,
    },
    Snapshot {
        snapshot: Snapshot,
    },
    Text {
        text: String,
    },
    Flag {
        ok: bool,
    },
    Cookies {
        cookies: Vec<Cookie>,
    },
    Done,
    /// `click` outcome. `navigated:true` means the click triggered a
    /// (same-process) navigation and `url` is the landed page; `navigated:false`
    /// means no navigation was observed in the probe window (non-navigating
    /// element, JS-handler anchor, or a bot wall) — the agent can branch on
    /// this instead of probing the URL itself.
    Clicked {
        navigated: bool,
        url: Option<String>,
    },
    /// `find_by_css` result: stable `@eN` refs matching the selector, numbered
    /// to match `snapshot`'s ordering (immediately clickable).
    Elements {
        refs: Vec<ElementRef>,
    },
    Image {
        png_base64: String,
    },
    Tools {
        tools: Vec<WebMcpTool>,
    },
    Tabs {
        tabs: Vec<TabInfo>,
    },
    TabOpened {
        tab: TabInfo,
    },
}

/// Everything a surface can ask of the manager.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Request {
    Open {
        options: SessionOptions,
    },
    Close {
        session: SessionId,
    },
    ListSessions,
    Act {
        session: SessionId,
        action: Action,
    },
    /// Run a sequence of actions in one round-trip (cuts agent latency:
    /// open+extract+close, fill+press+wait_url, etc., as one request).
    /// Fails fast on the first error so the agent sees which step broke.
    Batch {
        session: SessionId,
        actions: Vec<Action>,
    },
}

/// Wire-level classification of an application error, preserving the
/// `VakError` discriminant so transports (HTTP/MCP/CLI) can map status
/// codes without string-matching an opaque message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ServiceError {
    Engine(String),
    Protocol(String),
    Perception(String),
    Policy(String),
    NotFound(String),
    Http(String),
    Unsupported(String),
    Timeout(String),
    Io(String),
}

impl std::fmt::Display for ServiceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let (kind, msg) = match self {
            ServiceError::Engine(m) => ("engine", m),
            ServiceError::Protocol(m) => ("protocol", m),
            ServiceError::Perception(m) => ("perception", m),
            ServiceError::Policy(m) => ("policy", m),
            ServiceError::NotFound(m) => ("not found", m),
            ServiceError::Http(m) => ("http", m),
            ServiceError::Unsupported(m) => ("unsupported", m),
            ServiceError::Timeout(m) => ("timeout", m),
            ServiceError::Io(m) => ("io", m),
        };
        write!(f, "{kind}: {msg}")
    }
}

impl std::error::Error for ServiceError {}

impl From<VakError> for ServiceError {
    fn from(e: VakError) -> Self {
        match e {
            VakError::Engine(s) => ServiceError::Engine(s),
            VakError::Protocol(s) => ServiceError::Protocol(s),
            VakError::Perception(s) => ServiceError::Perception(s),
            VakError::Policy(s) => ServiceError::Policy(s),
            VakError::NotFound(s) => ServiceError::NotFound(s),
            VakError::Http(s) => ServiceError::Http(s),
            VakError::Io(e) => ServiceError::Io(e.to_string()),
            VakError::Unsupported(s) => ServiceError::Unsupported(s),
            VakError::Timeout(s) => ServiceError::Timeout(s),
        }
    }
}

/// Externally tagged (default serde) — internally-tagged representation
/// cannot encode `Closed(bool)` (primitive newtype under a tag).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ResponsePayload {
    Opened(SessionInfo),
    Closed(bool),
    Sessions(Vec<SessionInfo>),
    Result(ActionResult),
    /// `Request::Batch`: one result per action, in order (fail-fast on first
    /// error — the whole batch surfaces that action's `ServiceError`).
    Results(Vec<ActionResult>),
    /// Application-level error (the request was well-formed but the action
    /// failed). Transport-level failures (panic in handler, encode errors)
    /// stay on the `Err(String)` arm of `Response`.
    Error(ServiceError),
}

pub type Response = std::result::Result<ResponsePayload, String>;

/// Static policy applied to every navigation. Empty prefix list = allow all.
/// This is the architectural defense against page-driven prompt injection:
/// scope what an agent's browser may reach, independent of prompts.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Policy {
    #[serde(default)]
    pub url_allow_prefixes: Vec<String>,
}

impl Policy {
    fn allows(&self, url: &str) -> bool {
        if self.url_allow_prefixes.is_empty() {
            return true;
        }
        self.url_allow_prefixes.iter().any(|p| url.starts_with(p))
    }

    fn check(&self, url: &str) -> Result<()> {
        if self.allows(url) {
            Ok(())
        } else {
            Err(VakError::Policy(format!("url blocked by allowlist: {url}")))
        }
    }
}

struct ManagedSession {
    id: SessionId,
    profile: Option<ProfileId>,
    page: Arc<Mutex<Box<dyn PageOps>>>,
    current_url: String,
    last_active: std::time::Instant,
    /// Original launch options (re-used on `RotateProxy`, only the proxy
    /// endpoint changes — profile/stealth/headless are preserved).
    opts: LaunchOptions,
    /// Proxy rotation pool; `proxy_idx` tracks the currently-active endpoint.
    proxy_pool: Vec<String>,
    proxy_idx: usize,
}

/// Fleet controls: how many sessions may exist at once, and how long an
/// untouched session is allowed to live.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PoolConfig {
    /// Hard cap on concurrent sessions; opens beyond it fail with Policy.
    #[serde(default = "default_max_sessions")]
    pub max_sessions: usize,
    /// Idle seconds before the reaper closes a session; None disables.
    #[serde(default)]
    pub idle_timeout_secs: Option<u64>,
}

fn default_max_sessions() -> usize {
    32
}

impl Default for PoolConfig {
    fn default() -> Self {
        Self {
            max_sessions: default_max_sessions(),
            idle_timeout_secs: None,
        }
    }
}

/// Owns all live sessions; drives engine launches and action dispatch.
pub struct SessionManager {
    sessions: Mutex<HashMap<SessionId, ManagedSession>>,
    /// Trait object so the engine backend is a plug-in: a non-CDP backend
    /// injects itself via `SessionManager::new(policy, launcher)`. The CDP
    /// shell remains the zero-argument convenience default.
    launcher: Arc<dyn EngineLauncher>,
    /// Experimental DOM backend (QuickJS, no Chrome). `Some` only when the
    /// `dom-backend` feature is compiled in; otherwise `Backend::Dom` opens
    /// raise an explicit error so agents don't silently fall back to Chrome.
    dom_launcher: Option<Arc<dyn EngineLauncher>>,
    /// Server-wide default backend when a session's `options.backend` is `None`
    /// (client omitted it). Operators set this via `VAKBROWSE_BACKEND`.
    default_backend: Backend,
    policy: Policy,
    pool: PoolConfig,
    profiles_root: Option<PathBuf>,
    next_id: Mutex<u64>,
}

impl Default for SessionManager {
    fn default() -> Self {
        Self::new(Policy::default(), default_launcher())
    }
}

/// The out-of-the-box engine: a managed chrome-headless-shell via CDP.
fn default_launcher() -> Arc<dyn EngineLauncher> {
    Arc::new(CdpLauncher::default())
}

/// Select the launcher per-open request based on `SessionOptions.backend`.
/// `Backend::Dom` needs the `dom-backend` feature; otherwise it fails loud (no
/// silent Chrome fallback) so an agent knows its chrome-free session didn't
/// get what it asked for.
impl SessionManager {
    fn launcher_for(&self, backend: &Backend) -> Result<&Arc<dyn EngineLauncher>> {
        Ok(match backend {
            Backend::Cdp => &self.launcher,
            Backend::Dom => self.dom_launcher.as_ref().ok_or_else(|| {
                VakError::Engine(
                    "dom backend not compiled in; rebuild with --features \
                     vakbrowse-server/dom-backend"
                        .into(),
                )
            })?,
        })
    }
}

impl SessionManager {
    /// Construct with an explicit engine launcher (swap-in point for other
    /// backends). Use `SessionManager::default()` / `SessionManager::new(policy)`
    /// for the default CDP backend.
    pub fn new(policy: Policy, launcher: Arc<dyn EngineLauncher>) -> Self {
        Self {
            sessions: Mutex::new(HashMap::new()),
            launcher,
            dom_launcher: {
                #[cfg(feature = "dom-backend")]
                let d: Option<Arc<dyn EngineLauncher>> = Some(Arc::new(DomLauncher));
                #[cfg(not(feature = "dom-backend"))]
                let d: Option<Arc<dyn EngineLauncher>> = None;
                d
            },
            default_backend: Backend::Cdp,
            policy,
            pool: PoolConfig::default(),
            profiles_root: default_profiles_root(),
            next_id: Mutex::new(0),
        }
    }

    /// Convenience: `new` wired to the default CDP launcher.
    pub fn with_policy(policy: Policy) -> Self {
        Self::new(policy, default_launcher())
    }

    /// Convenience: `new` wired to the default CDP launcher AND a server-wide
    /// default backend (used when a session omits `options.backend`). This is
    /// the entry point for `vakd`/`vakd-rest` to honor `VAKBROWSE_BACKEND`.
    pub fn with_policy_and_backend(policy: Policy, default_backend: Backend) -> Self {
        let mut s = Self::new(policy, default_launcher());
        s.default_backend = default_backend;
        s
    }

    pub fn with_pool(mut self, pool: PoolConfig) -> Self {
        self.pool = pool;
        self
    }

    /// Spawn the background idle reaper. Call once per manager, from a
    /// long-lived runtime (daemon/REST/MCP mains).
    pub fn spawn_reaper(self: &Arc<Self>) -> tokio::task::JoinHandle<()> {
        let this = Arc::clone(self);
        tokio::spawn(async move {
            let tick = std::time::Duration::from_secs(5);
            loop {
                tokio::time::sleep(tick).await;
                let Some(timeout) = this.pool.idle_timeout_secs else {
                    continue;
                };
                let expired: Vec<SessionId> = {
                    let map = this.sessions.lock().await;
                    map.values()
                        .filter(|s| {
                            s.last_active.elapsed() > std::time::Duration::from_secs(timeout)
                        })
                        .map(|s| s.id.clone())
                        .collect()
                };
                for id in expired {
                    tracing::info!(%id, "reaping idle session");
                    let _ = this.close(&id).await;
                }
            }
        })
    }

    pub async fn stats(&self) -> (usize, usize) {
        (self.sessions.lock().await.len(), self.pool.max_sessions)
    }

    /// Override where persistent profiles live (tests).
    pub fn with_profiles_root(mut self, root: PathBuf) -> Self {
        self.profiles_root = Some(root);
        self
    }

    async fn next_session_id(&self) -> SessionId {
        let mut n = self.next_id.lock().await;
        *n += 1;
        SessionId(format!("s{}", *n))
    }

    fn profile_dir(&self, profile: &ProfileId) -> Option<PathBuf> {
        // Keep ids filesystem-safe.
        if !profile
            .0
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        {
            return None;
        }
        self.profiles_root.as_ref().map(|r| r.join(&profile.0))
    }

    pub async fn open(
        &self,
        options: SessionOptions,
    ) -> Result<(SessionInfo, std::option::Option<Navigated>)> {
        if self.sessions.lock().await.len() >= self.pool.max_sessions {
            return Err(VakError::Policy(format!(
                "session cap reached ({})",
                self.pool.max_sessions
            )));
        }
        let mut launch = LaunchOptions {
            headless: options.headless,
            stealth: options
                .stealth_seed
                .as_deref()
                .map(vakbrowse_stealth::StealthProfile::generate),
            proxy_server: options
                .proxies
                .first()
                .cloned()
                .or_else(|| options.proxy.clone()),
            human_timing: options.human_timing,
            ..LaunchOptions::default()
        };
        if let Some(profile) = &options.profile {
            match self.profile_dir(profile) {
                Some(dir) => {
                    tokio::fs::create_dir_all(&dir).await?;
                    launch.user_data_dir = Some(dir);
                }
                None => {
                    return Err(VakError::Engine(format!(
                        "invalid profile id {:?} (use [A-Za-z0-9_-])",
                        profile.0
                    )));
                }
            }
        }

        // Client may override per-session; otherwise fall back to the server's
        // configured default (VAKBROWSE_BACKEND). `Backend::Dom` without the
        // `dom-backend` feature fails loud in `launcher_for`.
        let backend = options.backend.unwrap_or(self.default_backend);
        let page: Box<dyn PageOps> = self.launcher_for(&backend)?.launch(&launch).await?;
        let id = self.next_session_id().await;
        let init_proxy_idx = if options.proxies.is_empty() {
            0
        } else {
            options.proxies.len() - 1
        };
        let mut managed = ManagedSession {
            id: id.clone(),
            profile: options.profile.clone(),
            page: Arc::new(Mutex::new(page)),
            current_url: "about:blank".to_string(),
            last_active: std::time::Instant::now(),
            opts: launch,
            proxy_pool: options.proxies.clone(),
            proxy_idx: init_proxy_idx,
        };

        let navigated = match &options.url {
            Some(url) => {
                self.policy.check(url)?;
                let mut guard = managed.page.lock().await;
                let nav = guard.navigate(url).await?;
                managed.current_url = nav.url.clone();
                Some(nav)
            }
            None => None,
        };

        let info = SessionInfo {
            id,
            profile: options.profile.clone(),
            url: managed.current_url.clone(),
        };
        self.sessions.lock().await.insert(info.id.clone(), managed);

        tracing::info!(session = %info.id, "session opened");
        Ok((info, navigated))
    }

    pub async fn close(&self, id: &SessionId) -> Result<bool> {
        let removed = self.sessions.lock().await.remove(id).is_some();
        if removed {
            tracing::info!(%id, "session closed");
        }
        Ok(removed)
    }

    /// Close **all** live sessions. Called on daemon shutdown so that CDP
    /// browser handles (and their chrome processes) are dropped cleanly
    /// rather than orphaned. Each `ManagedSession` drop tears down its
    /// `CdpSession`, which drops the `Browser` handle and closes chrome.
    pub async fn close_all(&self) {
        let all: Vec<SessionId> = self.sessions.lock().await.keys().cloned().collect();
        for id in all {
            tracing::info!(%id, "closing session on shutdown");
            if let Err(e) = self.close(&id).await {
                tracing::warn!(%id, error = %e, "error closing session during shutdown");
            }
        }
    }

    pub async fn list(&self) -> Vec<SessionInfo> {
        self.sessions
            .lock()
            .await
            .values()
            .map(|s| SessionInfo {
                id: s.id.clone(),
                profile: s.profile.clone(),
                url: s.current_url.clone(),
            })
            .collect()
    }

    /// Rotate a session to the next proxy endpoint: re-launch the browser with
    /// the next `--proxy-server` from the session's pool (preserving profile +
    /// stealth + headless), swap the page atomically, and drop the old browser
    /// so its chrome process is torn down. Requires a 2+ proxy pool at open.
    pub async fn rotate_proxy(&self, id: &SessionId) -> Result<ActionResult> {
        let (opts, pool, idx) = {
            let map = self.sessions.lock().await;
            let s = map
                .get(id)
                .ok_or_else(|| VakError::NotFound(format!("unknown session {id}")))?;
            (s.opts.clone(), s.proxy_pool.clone(), s.proxy_idx)
        };
        let n = pool.len();
        if n < 2 {
            return Err(VakError::Engine(
                "RotateProxy needs a proxy pool of 2+ endpoints (open with proxies=[a,b,…])".into(),
            ));
        }
        let next = (idx + 1) % n;
        let mut launch = opts.clone();
        launch.proxy_server = Some(pool[next].clone());

        tracing::info!(session = %id, proxy = %pool[next], "rotating proxy (endpoint {next}/{n})");
        let page: Box<dyn PageOps> = self.launcher.launch(&launch).await?;

        // Remember where the session was, then swap the page live under the
        // lock. The old CdpSession drops here and its chrome is torn down.
        let restored_url = {
            let mut map = self.sessions.lock().await;
            let s = map
                .get_mut(id)
                .ok_or_else(|| VakError::NotFound(format!("unknown session {id}")))?;
            let mut guard = s.page.lock().await;
            *guard = page;
            s.proxy_idx = next;
            s.last_active = std::time::Instant::now();
            s.current_url.clone()
        };

        // Re-establish the session's URL on the freshly launched browser. This
        // is what makes rotation useful in practice (bot wall → rotate → carry
        // on with the same page, new source IP). A session that never
        // navigated stays on about:blank. The URL already passed policy at
        // open(); we re-check cheaply to keep the invariant honoured.
        if restored_url != "about:blank" {
            self.policy.check(&restored_url)?;
            let mut map = self.sessions.lock().await;
            let s = map
                .get_mut(id)
                .ok_or_else(|| VakError::NotFound(format!("unknown session {id}")))?;
            let mut guard = s.page.lock().await;
            let nav = guard.navigate(&restored_url).await?;
            s.current_url = nav.url.clone();
        }

        Ok(ActionResult::Flag { ok: true })
    }

    pub async fn act(&self, id: &SessionId, action: Action) -> Result<ActionResult> {
        // RotateProxy re-launches the browser (a 100-500ms chrome spawn) and
        // swaps the page under the lock. Handle it *before* grabbing `page`
        // so we never hold the page mutex across the launch (which would
        // deadlock against our own swap) and so the old CdpSession drops and
        // tears down its chrome cleanly.
        if matches!(action, Action::RotateProxy) {
            return self.rotate_proxy(id).await;
        }

        let entry = {
            let map = self.sessions.lock().await;
            map.get(id)
                .map(|s| (s.page.clone(), s.profile.clone()))
                .ok_or_else(|| VakError::NotFound(format!("unknown session {id}")))?
        };
        let (page_arc, _profile) = entry;
        let mut page = page_arc.lock().await;

        // Gate every navigation through the URL policy. `NewTab` with an
        // inline URL is the same class of move as `Navigate` — it must not
        // bypass the allowlist.
        match &action {
            Action::Navigate { url } | Action::NewTab { url: Some(url) } => {
                self.policy.check(url)?;
            }
            _ => {}
        }

        // Refresh the activity tick *before* the action runs, so a long
        // action (e.g. wait_for_truthy) cannot be reaped mid-flight by the
        // idle reaper. The tick before lock held by page_arc is cheap.
        if let Some(s) = self.sessions.lock().await.get_mut(id) {
            s.last_active = std::time::Instant::now();
        }

        let result = match action {
            // Handled before the page lock is acquired (re-launches the browser).
            Action::RotateProxy => unreachable!("RotateProxy returns before the page lock"),
            Action::Navigate { url } => {
                let nav = page.navigate(&url).await?;
                ActionResult::Navigated {
                    url: nav.url.clone(),
                    title: nav.title,
                }
            }
            Action::Snapshot => ActionResult::Snapshot {
                snapshot: page.snapshot().await?,
            },
            Action::Click { r#ref } => {
                let out = page.click(&ElementRef(r#ref)).await?;
                ActionResult::Clicked {
                    navigated: out.navigated,
                    url: out.url,
                }
            }
            Action::Fill { r#ref, text } => {
                page.fill(&ElementRef(r#ref), &text).await?;
                ActionResult::Done
            }
            Action::SelectOption { r#ref, value } => ActionResult::Flag {
                ok: page.select_option(&ElementRef(r#ref), &value).await?,
            },
            Action::PressKey { key } => {
                page.press_key(&key).await?;
                ActionResult::Done
            }
            Action::Scroll { dx, dy } => {
                page.scroll(dx, dy).await?;
                ActionResult::Done
            }
            Action::EvalText { expression } => ActionResult::Text {
                text: page.eval_text(&expression).await?,
            },
            Action::FindByCss { selector } => ActionResult::Elements {
                refs: page.find_by_css(&selector).await?,
            },
            Action::WaitForTruthy {
                expression,
                timeout_ms,
            } => {
                page.wait_for_truthy(&expression, timeout_ms).await?;
                ActionResult::Done
            }
            Action::WaitForUrl {
                pattern,
                timeout_ms,
            } => {
                page.wait_for_url(&pattern, timeout_ms).await?;
                ActionResult::Done
            }
            Action::Cookies => ActionResult::Cookies {
                cookies: page.cookies().await?,
            },
            Action::SetCookie { cookie } => {
                page.set_cookie(&cookie).await?;
                ActionResult::Done
            }
            Action::ClearCookies => {
                page.clear_cookies().await?;
                ActionResult::Done
            }
            Action::SetDownloadDir { dir } => {
                page.set_download_dir(std::path::Path::new(&dir)).await?;
                ActionResult::Done
            }
            Action::Screenshot { full_page } => {
                let png = page.screenshot(full_page).await?;
                use base64::Engine as _;
                ActionResult::Image {
                    png_base64: base64::engine::general_purpose::STANDARD.encode(png),
                }
            }
            Action::ClickAt { x, y } => {
                page.click_at(x, y).await?;
                ActionResult::Done
            }
            Action::WebMcpTools => ActionResult::Tools {
                tools: page.webmcp_tools().await?,
            },
            Action::WebMcpInvoke {
                name,
                arguments_json,
            } => ActionResult::Text {
                text: page.webmcp_invoke(&name, &arguments_json).await?,
            },

            Action::Tabs => ActionResult::Tabs {
                tabs: page.tabs().await?,
            },
            Action::NewTab { url } => {
                let tab = page.new_tab(url.as_deref()).await?;
                // New tab becomes active; keep the session-level URL shadow
                // in sync so `list()`/Status report the right page.
                if let Some(st) = self.sessions.lock().await.get_mut(id) {
                    st.current_url = tab.url.clone();
                }
                ActionResult::TabOpened { tab }
            }
            Action::SwitchTab { tab } => {
                page.switch_tab(&tab).await?;
                ActionResult::Done
            }
            Action::CloseTab { tab } => ActionResult::Flag {
                ok: page.close_tab(&tab).await?,
            },

            Action::Back => {
                let nav = page.back().await?;
                ActionResult::Navigated {
                    url: nav.url,
                    title: nav.title,
                }
            }
            Action::Forward => {
                let nav = page.forward().await?;
                ActionResult::Navigated {
                    url: nav.url,
                    title: nav.title,
                }
            }
            Action::Extract => {
                let ex: Extracted = page.extract().await?;
                ActionResult::Text {
                    text: format!("{}\n{}\n\n{}", ex.title, ex.url, ex.text),
                }
            }
            Action::Reload => {
                let nav = page.reload().await?;
                ActionResult::Navigated {
                    url: nav.url,
                    title: nav.title,
                }
            }
        };

        if let ActionResult::Navigated { url, .. } = &result
            && let Some(s) = self.sessions.lock().await.get_mut(id)
        {
            s.current_url = url.clone();
        }

        Ok(result)
    }

    /// Single entry point used by every transport (uds, mcp, tests).
    /// Always returns `Ok(...)` for well-formed requests; application errors
    /// are carried on `Ok(ResponsePayload::Error)` so transports can map the
    /// error *kind* to HTTP/MCP status. The `Err(String)` arm is reserved
    /// for handler panics that surface as a 500-style internal error.
    pub async fn handle(&self, request: Request) -> Response {
        let payload = match request {
            Request::Open { options } => match self.open(options).await {
                Ok((info, _)) => ResponsePayload::Opened(info),
                Err(e) => ResponsePayload::Error(e.into()),
            },
            Request::Close { session } => match self.close(&session).await {
                Ok(removed) => ResponsePayload::Closed(removed),
                Err(e) => ResponsePayload::Error(e.into()),
            },
            Request::ListSessions => ResponsePayload::Sessions(self.list().await),
            Request::Act { session, action } => match self.act(&session, action).await {
                Ok(result) => ResponsePayload::Result(result),
                Err(e) => ResponsePayload::Error(e.into()),
            },
            Request::Batch { session, actions } => {
                let mut out = Vec::with_capacity(actions.len());
                let mut failed: Option<VakError> = None;
                for action in actions {
                    match self.act(&session, action).await {
                        Ok(r) => out.push(r),
                        Err(e) => {
                            failed = Some(e);
                            break;
                        }
                    }
                }
                match failed {
                    Some(e) => ResponsePayload::Error(e.into()),
                    None => ResponsePayload::Results(out),
                }
            }
        };
        Ok(payload)
    }
}

fn default_profiles_root() -> Option<PathBuf> {
    dirs::data_dir().map(|d| d.join("vakbrowse").join("profiles"))
}

#[cfg(test)]
mod types_tests {
    use super::*;

    #[test]
    fn actions_roundtrip_through_json() {
        let req = Request::Act {
            session: SessionId::new("s1"),
            action: Action::Click {
                r#ref: "@e3".into(),
            },
        };
        let json = serde_json::to_string(&req).unwrap();
        let back: Request = serde_json::from_str(&json).unwrap();
        assert!(
            matches!(back, Request::Act { ref action, .. } if matches!(action, Action::Click { r#ref } if r#ref == "@e3"))
        );
    }

    #[test]
    fn batch_roundtrip_through_json() {
        // Two actions of different shapes in one batch — exercises the
        // internally-tagged Action enum inside the externally-tagged Request
        // (serde tag "type" -> "batch"; each action keeps its own tag like
        // "click" / "snapshot").
        let req = Request::Batch {
            session: SessionId::new("s7"),
            actions: vec![
                Action::Click {
                    r#ref: "@e3".into(),
                },
                Action::Snapshot,
                Action::WaitForUrl {
                    pattern: "ready".into(),
                    timeout_ms: 1000,
                },
            ],
        };
        let json = serde_json::to_string(&req).unwrap();
        let back: Request = serde_json::from_str(&json).unwrap();
        match back {
            Request::Batch { session, actions } => {
                assert_eq!(session, SessionId::new("s7"));
                assert_eq!(actions.len(), 3);
                assert!(matches!(&actions[0], Action::Click { .. }));
                assert!(matches!(&actions[1], Action::Snapshot));
                assert!(matches!(
                    &actions[2],
                    Action::WaitForUrl { pattern, .. } if pattern == "ready"
                ));
            }
            other => panic!("expected Batch, got {other:?}"),
        }
        // The wire tag is "batch".
        assert!(json.contains("\"type\":\"batch\""));
    }

    #[test]
    fn policy_gate() {
        let p = Policy {
            url_allow_prefixes: vec!["https://intranet.example/".into()],
        };
        assert!(p.check("https://intranet.example/x").is_ok());
        assert!(p.check("https://evil.example/x").is_err());
        assert!(Policy::default().check("https://anything").is_ok());
    }
}
