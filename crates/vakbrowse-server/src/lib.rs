//! Session manager + command model shared by the daemon, CLI and MCP
//! server. Every surface speaks the same `Request`/`Response` language, so
//! behavior is identical whether an agent embeds the manager in-process or
//! talks to a running daemon over a socket.

/// Hard cap on `Request::Batch` actions. Prevents a malicious or buggy
/// client from OOM-ing the daemon with an arbitrarily large batch array.
/// 500 is generous for agent loops (fill→press→wait→extract is ~4) while
/// bounding memory for the `Vec::with_capacity` allocation.
pub const MAX_BATCH_SIZE: usize = 500;

/// Cap on free-form text a single action may return (`eval`, `source`,
/// WebMCP results). ~15k tokens: enough to read a page, not enough for one
/// runaway `document.documentElement.outerHTML` to flood an agent's context.
pub const MAX_TEXT_RESULT_CHARS: usize = 60_000;

fn cap_text(mut text: String) -> String {
    let total = text.chars().count();
    if total <= MAX_TEXT_RESULT_CHARS {
        return text;
    }
    let cut = text
        .char_indices()
        .nth(MAX_TEXT_RESULT_CHARS)
        .map(|(i, _)| i)
        .unwrap_or(text.len());
    text.truncate(cut);
    text.push_str(&format!(
        "\n[truncated: {} more characters; use extract, or narrow the expression]",
        total - MAX_TEXT_RESULT_CHARS
    ));
    text
}

/// `extract` as agent-facing text. The footer is the only way a text-only
/// surface (MCP, CLI) learns the content was cut and where to resume.
fn render_extract(ex: &Extracted) -> String {
    let mut text = format!("{}\n{}\n\n{}", ex.title, ex.url, ex.text);
    match ex.next_offset {
        Some(next) => text.push_str(&format!(
            "\n\n[truncated: characters {}..{next} of {}; extract with offset={next} for more]",
            ex.offset, ex.total_chars
        )),
        None if ex.offset > 0 => text.push_str(&format!(
            "\n\n[end of content: characters {}..{} of {}]",
            ex.offset, ex.total_chars, ex.total_chars
        )),
        None => {}
    }
    text
}

pub mod governor;
pub mod netguard;
pub mod render;
#[cfg(unix)]
pub mod uds;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;
use vakbrowse_core::{
    Cookie, CookieInput, DEFAULT_EXTRACT_CHARS, ElementRef, ExtractWindow, Extracted, ProfileId,
    Result, SessionId, Snapshot, TabId, TabInfo, VakError, WebMcpTool,
};
use vakbrowse_engine::{
    CdpLauncher, EngineLauncher, LaunchOptions, Navigated, PageOps, SessionState,
};

use governor::{Governor, Lease, ResourceStatus, Take};
use netguard::{GuardRules, NetGuard};

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
    /// Escalate a no-op anchor click to a DOM click and a forced
    /// `location.href` (defeats click-blocking sites, but re-fires the
    /// element's handlers — see `LaunchOptions::click_recovery`).
    #[serde(default)]
    pub click_recovery: bool,
    /// Lean rendering (no images, web fonts or autoplay). None = the
    /// governor's default, which is on for hosts with 4 GB or less.
    #[serde(default)]
    pub lean: Option<bool>,
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
            stealth_seed: None,
            proxy: None,
            proxies: Vec::new(),
            human_timing: false,
            click_recovery: false,
            lean: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionInfo {
    pub id: SessionId,
    pub profile: Option<ProfileId>,
    pub url: String,
    /// The browser was shut down to free memory; the next action restores
    /// its tabs, URLs and cookies transparently.
    #[serde(default)]
    pub hibernated: bool,
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
    /// Set files on a `<input type=file>` element by @eN ref (bypasses
    /// browser security that blocks programmatic file selection).
    SetFileChooser {
        r#ref: String,
        paths: Vec<String>,
    },
    /// Current page HTML source (`document.documentElement.outerHTML`).
    /// Useful for debugging when the a11y snapshot loses details (e.g.
    /// collapsed <details>, canvas, or content behind CSP).
    Source,
    /// List completed downloads in the session's download directory.
    /// Returns JSON: `[{path, bytes}, …]`.
    Downloads,
    /// List tools the page declares via WebMCP (`navigator.modelContext`), if any.
    WebMcpTools,
    /// Invoke a page-declared WebMCP tool.
    WebMcpInvoke {
        name: String,
        arguments_json: String,
    },

    Back,
    Forward,
    Reload,
    /// Readable main-content extraction (title + markdown-ish text), one
    /// window at a time: `offset` (chars, default 0) and `max_chars`
    /// (default 20k, max 60k). A cut window ends with a footer naming the
    /// `offset` to continue from. `{"type":"extract"}` stays valid.
    Extract {
        #[serde(default)]
        offset: usize,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        max_chars: Option<usize>,
    },

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
        /// Tab the click made the page open; not active until `switch_tab`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        opened_tab: Option<TabId>,
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
    /// Capacity, budget, queue and slot usage: what the host can take now.
    Status,
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
    /// Retryable: capacity was exhausted and the request was not run.
    Busy(String),
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
            ServiceError::Busy(m) => ("busy", m),
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
            VakError::Busy(s) => ServiceError::Busy(s),
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
    Status(ResourceStatus),
    /// Application-level error (the request was well-formed but the action
    /// failed). Transport-level failures (panic in handler, encode errors)
    /// stay on the `Err(String)` arm of `Response`.
    Error(ServiceError),
}

pub type Response = std::result::Result<ResponsePayload, String>;

/// Static policy applied to every navigation. Empty prefix list = allow all.
/// This is the architectural defense against page-driven prompt injection:
/// scope what an agent's browser may reach, independent of prompts.
///
/// Entries are matched structurally (scheme + host + port + path segment),
/// never as raw string prefixes, so `https://a.com` does not admit
/// `https://a.com.evil.io` or `https://a.com@evil.io`. Entries that are not
/// hierarchical URLs (e.g. `data:text/html`) fall back to a string prefix.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Policy {
    #[serde(default)]
    pub url_allow_prefixes: Vec<String>,
    /// Whether `file:` URLs may be opened (and local paths handed to file
    /// inputs). The library default is permissive so embedders and hermetic
    /// fixtures work; the daemon/REST/MCP/FFI surfaces build their policy with
    /// [`Policy::from_env`], which turns this OFF unless explicitly enabled.
    #[serde(default = "default_true")]
    pub allow_file: bool,
    /// Route every browser connection through the private-network guard
    /// (blocks loopback/private/link-local/metadata addresses, incl. via
    /// redirects, iframes and subresources). Off in the library default for
    /// embedders and hermetic tests; ON for every surface via `from_env`.
    #[serde(default)]
    pub block_private: bool,
    /// Private hosts the guard still admits (`host`, `host:port`, URL).
    #[serde(default)]
    pub private_allow: Vec<String>,
}

impl Default for Policy {
    fn default() -> Self {
        Self {
            url_allow_prefixes: Vec::new(),
            allow_file: true,
            block_private: false,
            private_allow: Vec::new(),
        }
    }
}

fn env_list(name: &str) -> Vec<String> {
    std::env::var(name)
        .unwrap_or_default()
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect()
}

fn env_flag(name: &str) -> bool {
    matches!(
        std::env::var(name).as_deref(),
        Ok("1") | Ok("true") | Ok("TRUE") | Ok("yes")
    )
}

impl Policy {
    /// Policy for network-facing surfaces: `VAKBROWSE_ALLOW_PREFIXES`
    /// (comma-separated) and `VAKBROWSE_ALLOW_FILE=1` to opt back in to `file:`.
    pub fn from_env() -> Self {
        Self {
            url_allow_prefixes: env_list("VAKBROWSE_ALLOW_PREFIXES"),
            allow_file: env_flag("VAKBROWSE_ALLOW_FILE"),
            block_private: !env_flag("VAKBROWSE_ALLOW_PRIVATE"),
            private_allow: env_list("VAKBROWSE_ALLOW_PRIVATE_HOSTS"),
        }
    }

    /// Guard exemptions: explicit private hosts plus every allowlisted
    /// prefix (an operator who allowlists `http://localhost:3000` means it).
    fn guard_rules(&self) -> GuardRules {
        let mut entries = self.private_allow.clone();
        entries.extend(self.url_allow_prefixes.iter().cloned());
        GuardRules::new(&entries)
    }

    fn restricted(&self) -> bool {
        !self.url_allow_prefixes.is_empty()
    }

    fn allows(&self, raw: &str) -> bool {
        let parsed = url::Url::parse(raw).ok();
        if let Some(u) = &parsed {
            if u.scheme() == "file" && !self.allow_file {
                return false;
            }
            // about:blank is the neutral parking page and carries no content.
            if u.scheme() == "about" && u.path() == "blank" {
                return true;
            }
        }
        if !self.restricted() {
            return true;
        }
        let Some(u) = parsed else {
            return false;
        };
        if !u.username().is_empty() || u.password().is_some() {
            return false;
        }
        self.url_allow_prefixes
            .iter()
            .any(|p| prefix_matches(p, &u, raw))
    }

    fn check(&self, url: &str) -> Result<()> {
        if self.allows(url) {
            Ok(())
        } else if url.starts_with("file:") && !self.allow_file {
            Err(VakError::Policy(
                "file: URLs are disabled (set VAKBROWSE_ALLOW_FILE=1 to enable)".into(),
            ))
        } else {
            Err(VakError::Policy(format!("url blocked by allowlist: {url}")))
        }
    }
}

fn prefix_matches(prefix: &str, url: &url::Url, raw: &str) -> bool {
    match url::Url::parse(prefix) {
        Ok(p) if !p.cannot_be_a_base() => {
            if p.scheme() != url.scheme()
                || p.host_str() != url.host_str()
                || p.port_or_known_default() != url.port_or_known_default()
            {
                return false;
            }
            let base = p.path().trim_end_matches('/');
            base.is_empty() || url.path() == base || url.path().starts_with(&format!("{base}/"))
        }
        _ => raw.starts_with(prefix),
    }
}

/// Resolves when the process is asked to stop: SIGINT (Ctrl-C) or, on unix,
/// SIGTERM (what `docker stop`, systemd and process supervisors send).
pub async fn shutdown_signal() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{SignalKind, signal};
        if let Ok(mut term) = signal(SignalKind::terminate()) {
            tokio::select! {
                _ = tokio::signal::ctrl_c() => {}
                _ = term.recv() => {}
            }
            return;
        }
    }
    let _ = tokio::signal::ctrl_c().await;
}

/// A session's browser: `None` while hibernated (state is in `dormant`).
type PageSlot = Option<Box<dyn PageOps>>;

struct ManagedSession {
    id: SessionId,
    profile: Option<ProfileId>,
    page: Arc<Mutex<PageSlot>>,
    /// Memory this session holds in the governor (base + extra tabs).
    lease: Option<Lease>,
    /// Tabs beyond the first, as charged in `lease`.
    extra_tabs: usize,
    /// Set while hibernated: what the next action restores.
    dormant: Option<SessionState>,
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

impl PoolConfig {
    /// `VAKBROWSE_MAX_SESSIONS` and `VAKBROWSE_IDLE_TIMEOUT_SECS` (`0` turns
    /// idle reaping off). `default_idle` applies when the latter is unset.
    pub fn from_env(default_idle: Option<u64>) -> Self {
        let max_sessions = std::env::var("VAKBROWSE_MAX_SESSIONS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or_else(default_max_sessions);
        let idle_timeout_secs = match std::env::var("VAKBROWSE_IDLE_TIMEOUT_SECS")
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
        {
            Some(0) => None,
            Some(n) => Some(n),
            None => default_idle,
        };
        Self {
            max_sessions,
            idle_timeout_secs,
        }
    }
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
    /// The CDP engine (chrome-headless-shell). CDP is the only backend; the
    /// `EngineLauncher`/`PageOps` trait seam remains for unit-testability but
    /// is not exposed as a runtime swappable backend.
    launcher: CdpLauncher,
    policy: Policy,
    pool: PoolConfig,
    profiles_root: Option<PathBuf>,
    next_id: Mutex<u64>,
    /// Opens that passed the cap check but have not yet inserted their
    /// session (Chrome takes seconds to launch). Counted against the cap so
    /// concurrent opens can't all slip under it.
    opening: AtomicUsize,
    /// Compute-aware admission: memory leases, FIFO queue, CPU slots.
    gov: Arc<Governor>,
    /// Private-network guard proxy, started on first launch when the policy
    /// asks for it.
    guard: tokio::sync::OnceCell<Arc<NetGuard>>,
}

/// Holds one slot of the session cap while a browser is launching.
/// Released explicitly on success (under the sessions lock, so the slot moves
/// atomically from "opening" to "open") or implicitly on any early return.
struct Reservation<'a> {
    counter: &'a AtomicUsize,
    live: bool,
}

impl Reservation<'_> {
    fn release(&mut self) {
        if self.live {
            self.live = false;
            self.counter.fetch_sub(1, Ordering::SeqCst);
        }
    }
}

impl Drop for Reservation<'_> {
    fn drop(&mut self) {
        self.release();
    }
}

impl Default for SessionManager {
    fn default() -> Self {
        Self::new(Policy::default())
    }
}

impl SessionManager {
    /// Construct with a CDP launcher (chrome-headless-shell). Use
    /// `SessionManager::default()` or `SessionManager::with_policy(policy)`
    /// for the zero-argument convenience constructors.
    pub fn new(policy: Policy) -> Self {
        Self {
            sessions: Mutex::new(HashMap::new()),
            launcher: CdpLauncher::default(),
            policy,
            pool: PoolConfig::default(),
            profiles_root: default_profiles_root(),
            next_id: Mutex::new(0),
            opening: AtomicUsize::new(0),
            gov: Governor::from_env(),
            guard: tokio::sync::OnceCell::new(),
        }
    }

    /// Replace the resource governor (tests, embedders with fixed budgets).
    pub fn with_governor(mut self, gov: Arc<Governor>) -> Self {
        self.gov = gov;
        self
    }

    pub fn governor(&self) -> &Arc<Governor> {
        &self.gov
    }

    pub async fn status(&self) -> ResourceStatus {
        let (n, dormant) = {
            let map = self.sessions.lock().await;
            (
                map.len(),
                map.values().filter(|s| s.dormant.is_some()).count(),
            )
        };
        self.gov.status(n, dormant, self.policy.block_private)
    }

    async fn net_guard(&self) -> Result<Option<Arc<NetGuard>>> {
        if !self.policy.block_private {
            return Ok(None);
        }
        let rules = self.policy.guard_rules();
        let g = self
            .guard
            .get_or_try_init(|| async move { NetGuard::start(rules).await.map(Arc::new) })
            .await?;
        Ok(Some(g.clone()))
    }

    /// Refuse a top-level URL whose host is private before the browser
    /// tries it (the proxy enforces the same rule for everything else).
    async fn guard_check(&self, url: &str) -> Result<()> {
        if let Some(g) = self.net_guard().await? {
            g.check_url(url)
                .await
                .map_err(|why| VakError::Policy(format!("private network blocked: {why}")))?;
        }
        Ok(())
    }

    /// A navigation the guard refused surfaces from Chrome as a generic
    /// network error; report it as the policy decision it was.
    fn explain(&self, e: VakError) -> VakError {
        if matches!(
            e,
            VakError::Protocol(_) | VakError::Engine(_) | VakError::Http(_)
        ) && let Some(g) = self.guard.get()
            && let Some(why) = g.recent_block(std::time::Duration::from_secs(5))
        {
            return VakError::Policy(format!("private network blocked: {why}"));
        }
        e
    }

    /// Wait (FIFO, bounded) for a CPU-bound slot.
    async fn slot<'a>(
        &self,
        sem: &'a tokio::sync::Semaphore,
        what: &str,
    ) -> Result<tokio::sync::SemaphorePermit<'a>> {
        match tokio::time::timeout(self.gov.queue_timeout(), sem.acquire()).await {
            Ok(Ok(p)) => Ok(p),
            Ok(Err(_)) => Err(VakError::Engine("governor shut down".into())),
            Err(_) => Err(VakError::Busy(format!(
                "waited {}s for a free {what} slot (all in use); retry later",
                self.gov.cfg.queue_timeout_secs
            ))),
        }
    }

    /// Wait in the admission queue until `mb` fits the memory budget and
    /// the live free memory, hibernating idle sessions to make room.
    async fn admit(&self, mb: u64, exclude: Option<&SessionId>) -> Result<Lease> {
        let ticket = self.gov.enqueue().map_err(VakError::Busy)?;
        let deadline = tokio::time::Instant::now() + self.gov.queue_timeout();
        loop {
            let notified = self.gov.notified();
            match self.gov.try_take(&ticket, mb) {
                Take::Granted(lease) => return Ok(lease),
                Take::NoRoom => {
                    if self.hibernate_one(exclude).await {
                        continue;
                    }
                }
                Take::NotYourTurn => {}
            }
            let now = tokio::time::Instant::now();
            if now >= deadline {
                return Err(VakError::Busy(format!(
                    "waited {}s for {mb} MB of browser memory (in use {}/{} MB, live free {}, {} queued); retry later or close sessions",
                    self.gov.cfg.queue_timeout_secs,
                    self.gov.used_mb(),
                    self.gov.cfg.budget_mb,
                    self.gov
                        .live_available()
                        .map_or("unknown".into(), |m| format!("{m} MB")),
                    self.gov.queued()
                )));
            }
            let tick = (deadline - now).min(std::time::Duration::from_millis(250));
            let _ = tokio::time::timeout(tick, notified).await;
        }
    }

    /// Launch with the guard applied (unless the session has its own
    /// proxy, where traffic leaves through that proxy's network) and inside
    /// a launch slot.
    async fn launch_browser(&self, opts: &LaunchOptions) -> Result<Box<dyn PageOps>> {
        let mut opts = opts.clone();
        if opts.proxy_server.is_none()
            && let Some(g) = self.net_guard().await?
        {
            opts.proxy_server = Some(g.proxy_url());
            opts.extra_args.extend(NetGuard::chrome_args());
        }
        let _launching = self.slot(&self.gov.launches, "browser launch").await?;
        self.launcher.launch(&opts).await
    }

    /// Shut a session's browser down to free memory, keeping what is needed
    /// to rebuild it (tabs, URLs, cookies). Skips a session mid-action.
    pub async fn hibernate(&self, id: &SessionId) -> bool {
        let Some(page_arc) = self.sessions.lock().await.get(id).map(|s| s.page.clone()) else {
            return false;
        };
        let Ok(mut slot) = page_arc.try_lock() else {
            return false;
        };
        let Some(mut page) = slot.take() else {
            return false;
        };
        match page.export_state().await {
            Ok(state) => {
                drop(page);
                if let Some(s) = self.sessions.lock().await.get_mut(id) {
                    s.dormant = Some(state);
                    s.lease = None;
                    s.extra_tabs = 0;
                }
                tracing::info!(%id, "session hibernated");
                true
            }
            Err(e) => {
                tracing::warn!(%id, error = %e, "hibernate failed; session kept live");
                *slot = Some(page);
                false
            }
        }
    }

    /// Free memory for a waiting request: hibernate the least recently used
    /// idle session.
    async fn hibernate_one(&self, exclude: Option<&SessionId>) -> bool {
        let min_idle = std::time::Duration::from_secs(self.gov.cfg.evict_min_idle_secs);
        let mut candidates: Vec<(std::time::Instant, SessionId)> = {
            let map = self.sessions.lock().await;
            map.values()
                .filter(|s| {
                    s.dormant.is_none()
                        && Some(&s.id) != exclude
                        && s.last_active.elapsed() >= min_idle
                })
                .map(|s| (s.last_active, s.id.clone()))
                .collect()
        };
        candidates.sort();
        for (_, id) in candidates {
            if self.hibernate(&id).await {
                return true;
            }
        }
        false
    }

    /// Bring a hibernated session back (called with its page lock held).
    async fn restore(&self, id: &SessionId, slot: &mut PageSlot) -> Result<()> {
        let (state, opts) = {
            let map = self.sessions.lock().await;
            let s = map
                .get(id)
                .ok_or_else(|| VakError::NotFound(format!("unknown session {id}")))?;
            (s.dormant.clone(), s.opts.clone())
        };
        let state =
            state.ok_or_else(|| VakError::Engine(format!("session {id} has no browser")))?;
        let extra = state.tabs.len().saturating_sub(1);
        let cost = self.gov.cfg.session_cost_mb + self.gov.cfg.tab_cost_mb * extra as u64;
        let lease = self.admit(cost, Some(id)).await?;
        let mut page = self.launch_browser(&opts).await?;
        page.import_state(&state).await?;
        *slot = Some(page);
        if let Some(s) = self.sessions.lock().await.get_mut(id) {
            s.dormant = None;
            s.lease = Some(lease);
            s.extra_tabs = extra;
        }
        tracing::info!(%id, tabs = state.tabs.len(), "session restored");
        Ok(())
    }

    /// Keep a session's lease in step with its tab count. `admitted` is a
    /// lease already queued for (an explicit new tab); tabs the page opened
    /// itself are charged without waiting — they already exist.
    async fn charge_tabs(&self, id: &SessionId, extra: usize, admitted: Option<Lease>) {
        let cost = self.gov.cfg.tab_cost_mb;
        let mut map = self.sessions.lock().await;
        let Some(s) = map.get_mut(id) else {
            return;
        };
        let lease = s.lease.get_or_insert_with(|| self.gov.empty_lease());
        let mut admitted = admitted;
        while s.extra_tabs < extra {
            match admitted.take() {
                Some(l) => lease.absorb(l),
                None => lease.absorb(self.gov.force(cost)),
            }
            s.extra_tabs += 1;
        }
        while s.extra_tabs > extra {
            lease.shrink(cost);
            s.extra_tabs -= 1;
        }
    }

    /// Convenience: `new` wired to a CDP launcher with a custom policy.
    pub fn with_policy(policy: Policy) -> Self {
        Self::new(policy)
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
                let close_after = this
                    .pool
                    .idle_timeout_secs
                    .map(std::time::Duration::from_secs);
                let sleep_after = this
                    .gov
                    .cfg
                    .hibernate_after_secs
                    .map(std::time::Duration::from_secs);
                let (expired, sleepy): (Vec<SessionId>, Vec<SessionId>) = {
                    let map = this.sessions.lock().await;
                    let mut expired = Vec::new();
                    let mut sleepy = Vec::new();
                    for s in map.values() {
                        let idle = s.last_active.elapsed();
                        // A locked page means an action is still running
                        // (last_active is only stamped when it starts).
                        if s.page.try_lock().is_err() {
                            continue;
                        }
                        if close_after.is_some_and(|t| idle > t) {
                            expired.push(s.id.clone());
                        } else if s.dormant.is_none() && sleep_after.is_some_and(|t| idle > t) {
                            sleepy.push(s.id.clone());
                        }
                    }
                    (expired, sleepy)
                };
                for id in expired {
                    tracing::info!(%id, "reaping idle session");
                    let _ = this.close(&id).await;
                }
                for id in sleepy {
                    this.hibernate(&id).await;
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
        let own_proxy = launch_proxy(&options).is_some();
        if let Some(url) = &options.url {
            self.policy.check(url)?;
            if !own_proxy {
                self.guard_check(url).await?;
            }
        }
        let lease = self.admit(self.gov.cfg.session_cost_mb, None).await?;
        let mut reservation = {
            let map = self.sessions.lock().await;
            if map.len() + self.opening.load(Ordering::SeqCst) >= self.pool.max_sessions {
                return Err(VakError::Busy(format!(
                    "session cap reached ({}); close or wait for a session",
                    self.pool.max_sessions
                )));
            }
            self.opening.fetch_add(1, Ordering::SeqCst);
            Reservation {
                counter: &self.opening,
                live: true,
            }
        };
        let mut launch = LaunchOptions {
            headless: options.headless,
            stealth: options
                .stealth_seed
                .as_deref()
                .map(vakbrowse_stealth::StealthProfile::generate),
            proxy_server: launch_proxy(&options),
            human_timing: options.human_timing,
            click_recovery: options.click_recovery,
            lean: options.lean.unwrap_or(self.gov.cfg.lean_default),
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

        // CDP is the only engine backend. The launcher is fixed at
        // `SessionManager::new` time, so `SessionOptions` no longer carries a
        // `backend` field.
        let page: Box<dyn PageOps> = self.launch_browser(&launch).await?;
        let id = self.next_session_id().await;
        let init_proxy_idx = INITIAL_PROXY_IDX;
        let mut managed = ManagedSession {
            id: id.clone(),
            profile: options.profile.clone(),
            page: Arc::new(Mutex::new(Some(page))),
            lease: Some(lease),
            extra_tabs: 0,
            dormant: None,
            current_url: "about:blank".to_string(),
            last_active: std::time::Instant::now(),
            opts: launch,
            proxy_pool: options.proxies.clone(),
            proxy_idx: init_proxy_idx,
        };

        let navigated = match &options.url {
            Some(url) => {
                self.policy.check(url)?;
                let _working = self.slot(&self.gov.active, "page work").await?;
                let mut guard = managed.page.lock().await;
                let page = guard
                    .as_mut()
                    .ok_or_else(|| VakError::Engine("browser missing after launch".into()))?;
                let nav = page.navigate(url).await.map_err(|e| self.explain(e))?;
                managed.current_url = nav.url.clone();
                Some(nav)
            }
            None => None,
        };

        let info = SessionInfo {
            id,
            profile: options.profile.clone(),
            url: managed.current_url.clone(),
            hibernated: false,
        };
        {
            let mut map = self.sessions.lock().await;
            map.insert(info.id.clone(), managed);
            reservation.release();
        }

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
                hibernated: s.dormant.is_some(),
            })
            .collect()
    }

    /// Rotate a session to the next proxy endpoint: re-launch the browser with
    /// the next `--proxy-server` from the session's pool (preserving profile +
    /// stealth + headless), swap the page atomically, and drop the old browser
    /// so its chrome process is torn down. Requires a 2+ proxy pool at open.
    pub async fn rotate_proxy(&self, id: &SessionId) -> Result<ActionResult> {
        // Lock discipline: the `sessions` map lock is only ever held briefly
        // and never across another await. `act()` holds a page mutex while it
        // briefly takes `sessions`; if this function held `sessions` while
        // awaiting the page mutex the two would deadlock (and stall every
        // session, since `sessions` is global).
        let (opts, pool, idx, page_arc) = {
            let map = self.sessions.lock().await;
            let s = map
                .get(id)
                .ok_or_else(|| VakError::NotFound(format!("unknown session {id}")))?;
            (
                s.opts.clone(),
                s.proxy_pool.clone(),
                s.proxy_idx,
                s.page.clone(),
            )
        };
        let n = pool.len();
        if n < 2 {
            return Err(VakError::Engine(
                "RotateProxy needs a proxy pool of 2+ endpoints (open with proxies=[a,b,…])".into(),
            ));
        }
        let next = rotation_target(idx, n);
        let mut launch = opts.clone();
        launch.proxy_server = Some(pool[next].clone());

        tracing::info!(session = %id, proxy = %pool[next], "rotating proxy (endpoint {next}/{n})");
        // Wait for any in-flight action on this session first: a hibernated
        // session just records the new endpoint for its next restore.
        let mut guard = page_arc.lock().await;
        if guard.is_none() {
            if let Some(s) = self.sessions.lock().await.get_mut(id) {
                s.proxy_idx = next;
                s.opts.proxy_server = Some(pool[next].clone());
            }
            return Ok(ActionResult::Flag { ok: true });
        }
        let new_page: Box<dyn PageOps> = self.launch_browser(&launch).await?;
        // Swap the page; the old CdpSession drops and its chrome is torn down.
        *guard = Some(new_page);
        let restored_url = {
            let mut map = self.sessions.lock().await;
            match map.get_mut(id) {
                Some(s) => {
                    s.proxy_idx = next;
                    s.opts.proxy_server = Some(pool[next].clone());
                    s.last_active = std::time::Instant::now();
                    s.current_url.clone()
                }
                None => return Err(VakError::NotFound(format!("unknown session {id}"))),
            }
        };

        // Re-establish the session's URL on the freshly launched browser. This
        // is what makes rotation useful in practice (bot wall → rotate → carry
        // on with the same page, new source IP). A session that never
        // navigated stays on about:blank. The URL already passed policy at
        // open(); we re-check cheaply to keep the invariant honoured.
        if restored_url != "about:blank" {
            self.policy.check(&restored_url)?;
            let page = guard
                .as_mut()
                .ok_or_else(|| VakError::Engine("browser missing after rotation".into()))?;
            let nav = page.navigate(&restored_url).await?;
            if let Some(s) = self.sessions.lock().await.get_mut(id) {
                s.current_url = nav.url;
            }
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

        let (page_arc, own_proxy) = {
            let map = self.sessions.lock().await;
            map.get(id)
                .map(|s| (s.page.clone(), s.opts.proxy_server.is_some()))
                .ok_or_else(|| VakError::NotFound(format!("unknown session {id}")))?
        };
        let mut slot = page_arc.lock().await;
        if slot.is_none() {
            self.restore(id, &mut slot).await?;
        }

        // Gate every navigation through the URL policy. `NewTab` with an
        // inline URL is the same class of move as `Navigate` — it must not
        // bypass the allowlist.
        match &action {
            Action::Navigate { url } | Action::NewTab { url: Some(url) } => {
                self.policy.check(url)?;
            }
            Action::SetFileChooser { .. } if !self.policy.allow_file => {
                return Err(VakError::Policy(
                    "local file access is disabled (set VAKBROWSE_ALLOW_FILE=1 to enable)".into(),
                ));
            }
            _ => {}
        }
        if let Action::Navigate { url } | Action::NewTab { url: Some(url) } = &action
            && !own_proxy
        {
            self.guard_check(url).await?;
        }

        // An explicit new tab is new memory: queue for it like an open.
        let tab_lease = match &action {
            Action::NewTab { .. } => Some(self.admit(self.gov.cfg.tab_cost_mb, Some(id)).await?),
            _ => None,
        };
        // Page work is CPU-bound; waits only sleep, so they hold no slot.
        let _working = match &action {
            Action::WaitForTruthy { .. } | Action::WaitForUrl { .. } => None,
            _ => Some(self.slot(&self.gov.active, "page work").await?),
        };
        let page = slot
            .as_mut()
            .ok_or_else(|| VakError::Engine(format!("session {id} has no browser")))?;

        // Refresh the activity tick *before* the action runs, so a long
        // action (e.g. wait_for_truthy) cannot be reaped mid-flight by the
        // idle reaper. The tick before lock held by page_arc is cheap.
        if let Some(s) = self.sessions.lock().await.get_mut(id) {
            s.last_active = std::time::Instant::now();
        }

        // Actions that cannot move the page skip the post-action URL probe.
        let observes_only = matches!(
            action,
            Action::Cookies
                | Action::Tabs
                | Action::Screenshot { .. }
                | Action::Source
                | Action::Downloads
                | Action::WebMcpTools
                | Action::Extract { .. }
                | Action::FindByCss { .. }
                | Action::SetCookie { .. }
                | Action::ClearCookies
                | Action::SetDownloadDir { .. }
        );

        let closing_tab = matches!(action, Action::CloseTab { .. });
        let blocks_before = self.guard.get().map(|g| g.block_count());
        let result: Result<ActionResult> = async {
            Ok(match action {
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
                        opened_tab: out.opened_tab.map(TabId),
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
                    text: cap_text(page.eval_text(&expression).await?),
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
                    text: cap_text(page.webmcp_invoke(&name, &arguments_json).await?),
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
                Action::Extract { offset, max_chars } => {
                    let window = ExtractWindow {
                        offset,
                        max_chars: max_chars.unwrap_or(DEFAULT_EXTRACT_CHARS),
                    };
                    let ex: Extracted = page.extract(window).await?;
                    ActionResult::Text {
                        text: render_extract(&ex),
                    }
                }
                Action::Reload => {
                    let nav = page.reload().await?;
                    ActionResult::Navigated {
                        url: nav.url,
                        title: nav.title,
                    }
                }
                Action::SetFileChooser { r#ref, paths } => {
                    let ok = page
                        .set_file_chooser(&ElementRef::new(&r#ref), &paths)
                        .await?;
                    ActionResult::Flag { ok }
                }
                Action::Source => {
                    let text = cap_text(page.source().await?);
                    ActionResult::Text { text }
                }
                Action::Downloads => {
                    let list = page.downloads().await?;
                    ActionResult::Text {
                        text: serde_json::to_string(&list).unwrap_or_else(|_| "[]".to_string()),
                    }
                }
            })
        }
        .await;
        let result = result.map_err(|e| self.explain(e))?;

        // Tab memory accounting.
        let extra_now = self
            .sessions
            .lock()
            .await
            .get(id)
            .map_or(0, |s| s.extra_tabs);
        match &result {
            ActionResult::TabOpened { .. } => {
                self.charge_tabs(id, extra_now + 1, tab_lease).await;
            }
            ActionResult::Clicked {
                opened_tab: Some(_),
                ..
            } => self.charge_tabs(id, extra_now + 1, None).await,
            ActionResult::Flag { ok: true } if closing_tab => {
                self.charge_tabs(id, extra_now.saturating_sub(1), None)
                    .await;
            }
            ActionResult::Tabs { tabs } => {
                self.charge_tabs(id, tabs.len().saturating_sub(1), None)
                    .await;
            }
            _ => {}
        }

        // Page-driven navigation (link clicks, redirects, history, JS) never
        // passes through `navigate`, so ask the page where it actually is.
        // This keeps `current_url` (used by `list()` and proxy rotation) true,
        // and enforces the allowlist: a violation parks the tab on
        // about:blank and surfaces a Policy error. Subresource and iframe
        // loads are not covered by the allowlist check.
        if !observes_only
            && let Ok(live) = page.eval_text("location.href").await
            && !live.is_empty()
        {
            // The guard refused something during this action; if it was the
            // main document (a redirect into a private range), the page now
            // shows the guard's block page or a browser error page.
            if let (Some(g), Some(before)) = (self.guard.get(), blocks_before)
                && g.block_count() > before
            {
                let probe = format!(
                    "location.protocol === 'chrome-error:' || (document.contentType === 'text/plain' && (document.body ? document.body.innerText : '').startsWith({:?}))",
                    netguard::BLOCK_PAGE_PREFIX
                );
                if page.eval_text(&probe).await.is_ok_and(|v| v == "true") {
                    let why = g
                        .recent_block(std::time::Duration::from_secs(30))
                        .unwrap_or_else(|| "private/internal address".into());
                    let _ = page.navigate("about:blank").await;
                    if let Some(s) = self.sessions.lock().await.get_mut(id) {
                        s.current_url = "about:blank".into();
                    }
                    return Err(VakError::Policy(format!("private network blocked: {why}")));
                }
            }
            if self.policy.restricted()
                && let Err(e) = self.policy.check(&live)
            {
                let _ = page.navigate("about:blank").await;
                if let Some(s) = self.sessions.lock().await.get_mut(id) {
                    s.current_url = "about:blank".into();
                }
                return Err(e);
            }
            if let Some(s) = self.sessions.lock().await.get_mut(id) {
                s.current_url = live;
            }
        }

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
            Request::Status => ResponsePayload::Status(self.status().await),
            Request::Act { session, action } => match self.act(&session, action).await {
                Ok(result) => ResponsePayload::Result(result),
                Err(e) => ResponsePayload::Error(e.into()),
            },
            Request::Batch { session, actions } => {
                if actions.len() > MAX_BATCH_SIZE {
                    return Ok(ResponsePayload::Error(ServiceError::Policy(format!(
                        "batch exceeds max {MAX_BATCH_SIZE} actions (got {})",
                        actions.len()
                    ))));
                }
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

/// Endpoint the browser launches on: the pool's first entry, else `proxy`.
fn launch_proxy(options: &SessionOptions) -> Option<String> {
    options
        .proxies
        .first()
        .cloned()
        .or_else(|| options.proxy.clone())
}

/// `launch_proxy` is `proxies[0]`, so that is the active index at open time.
/// (Starting at `len-1` made the first rotation re-select the same proxy.)
const INITIAL_PROXY_IDX: usize = 0;

fn rotation_target(idx: usize, pool_len: usize) -> usize {
    (idx + 1) % pool_len
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

    #[tokio::test]
    async fn batch_over_max_actions_is_policy_error() {
        // The MAX_BATCH_SIZE guard fires before any session lookup or engine
        // launch, so this is fully hermetic (no Chrome needed).
        let manager = SessionManager::default();
        let oversized = vec![Action::Snapshot; MAX_BATCH_SIZE + 1];
        let resp = manager
            .handle(Request::Batch {
                session: SessionId::new("dummy"),
                actions: oversized,
            })
            .await
            .unwrap();
        match resp {
            ResponsePayload::Error(ServiceError::Policy(_)) => {}
            other => panic!("expected Policy error for oversized batch, got {other:?}"),
        }
    }

    #[test]
    fn policy_gate() {
        let p = Policy {
            url_allow_prefixes: vec!["https://intranet.example/".into()],
            ..Policy::default()
        };
        assert!(p.check("https://intranet.example/x").is_ok());
        assert!(p.check("https://evil.example/x").is_err());
        assert!(Policy::default().check("https://anything").is_ok());
    }

    #[test]
    fn extract_action_wire_shape_is_backward_compatible() {
        let a: Action = serde_json::from_str(r#"{"type":"extract"}"#).unwrap();
        assert!(matches!(
            a,
            Action::Extract {
                offset: 0,
                max_chars: None
            }
        ));
        let a: Action =
            serde_json::from_str(r#"{"type":"extract","offset":500,"max_chars":1000}"#).unwrap();
        assert!(matches!(
            a,
            Action::Extract {
                offset: 500,
                max_chars: Some(1000)
            }
        ));
    }

    #[test]
    fn render_extract_footer_names_resume_offset() {
        let w = |offset, max_chars| ExtractWindow { offset, max_chars };
        let body = "one two\nthree four\nfive six";
        let ex = Extracted::windowed("T".into(), "u".into(), body, w(0, 12));
        let out = render_extract(&ex);
        assert!(out.starts_with("T\nu\n\none two"), "{out}");
        assert!(
            out.ends_with("[truncated: characters 0..8 of 27; extract with offset=8 for more]"),
            "{out}"
        );
        let ex = Extracted::windowed("T".into(), "u".into(), body, w(8, 100));
        assert!(render_extract(&ex).ends_with("[end of content: characters 8..27 of 27]"));
        let ex = Extracted::windowed("T".into(), "u".into(), body, w(0, 100));
        assert_eq!(render_extract(&ex), format!("T\nu\n\n{body}"));
    }

    #[test]
    fn long_text_results_are_capped() {
        let short = "x".repeat(MAX_TEXT_RESULT_CHARS);
        assert_eq!(cap_text(short.clone()), short);
        let long = "é".repeat(MAX_TEXT_RESULT_CHARS + 7);
        let out = cap_text(long);
        assert!(out.starts_with(&"é".repeat(MAX_TEXT_RESULT_CHARS)));
        assert!(out.ends_with("narrow the expression]"));
        assert!(out.contains("7 more characters"));
    }

    #[test]
    fn first_rotation_moves_to_a_different_proxy() {
        let opts = SessionOptions {
            proxies: vec!["http://a:1".into(), "http://b:2".into()],
            ..SessionOptions::default()
        };
        let pool = &opts.proxies;
        let launched = launch_proxy(&opts).unwrap();
        assert_eq!(launched, pool[INITIAL_PROXY_IDX]);
        let next = rotation_target(INITIAL_PROXY_IDX, pool.len());
        assert_ne!(pool[next], launched);
        assert_eq!(rotation_target(next, pool.len()), INITIAL_PROXY_IDX);
    }

    #[test]
    fn policy_is_structural_not_string_prefix() {
        let p = Policy {
            url_allow_prefixes: vec!["https://a.com".into(), "https://b.com/app".into()],
            ..Policy::default()
        };
        assert!(p.check("https://a.com/x?y=1").is_ok());
        assert!(p.check("https://a.com:443/x").is_ok());
        assert!(p.check("https://a.com.evil.io/").is_err());
        assert!(p.check("https://a.com@evil.io/").is_err());
        assert!(p.check("https://user:pw@a.com/").is_err());
        assert!(p.check("http://a.com/").is_err());
        assert!(p.check("https://a.com:8443/").is_err());
        assert!(p.check("https://b.com/app").is_ok());
        assert!(p.check("https://b.com/app/x").is_ok());
        assert!(p.check("https://b.com/application").is_err());
        assert!(p.check("about:blank").is_ok());
        assert!(p.check("not a url").is_err());
    }

    #[test]
    fn opaque_prefixes_and_file_gate() {
        let p = Policy {
            url_allow_prefixes: vec!["data:text/html".into()],
            ..Policy::default()
        };
        assert!(p.check("data:text/html,<p>x</p>").is_ok());
        assert!(p.check("data:text/plain,x").is_err());

        let no_file = Policy {
            allow_file: false,
            ..Policy::default()
        };
        assert!(no_file.check("file:///etc/hosts").is_err());
        assert!(no_file.check("https://example.com").is_ok());
    }
}
