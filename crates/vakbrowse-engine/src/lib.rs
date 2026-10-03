//! Engine seam: every capability in vakBrowse is written against these traits
//! so the CDP backend can be swapped (Lightpanda, Servo) without rewrites.

pub mod cdp;
pub mod cft;

pub use cdp::CdpLauncher;

use std::path::{Path, PathBuf};
use vakbrowse_core::{
    Cookie, CookieInput, DownloadInfo, ElementRef, ExtractWindow, Extracted, Result, Snapshot,
    TabId, TabInfo, VakError, WebMcpTool,
};

/// Everything needed to start a browser process.
#[derive(Debug, Clone)]
pub struct LaunchOptions {
    /// Explicit browser executable; when `None` the backend resolves or
    /// downloads its default (chrome-headless-shell).
    pub executable: Option<PathBuf>,
    pub headless: bool,
    /// Chromium profile directory; reuse it across launches for persistent
    /// authenticated sessions (one profile per agent identity).
    pub user_data_dir: Option<PathBuf>,
    pub window_size: (u32, u32),
    pub extra_args: Vec<String>,
    /// When set, applies the profile's fingerprint patches and humanized
    /// input behavior.
    pub stealth: Option<vakbrowse_stealth::StealthProfile>,
    /// Chromium proxy, e.g. `http://user:pass@host:port` or `socks5://host:port`.
    pub proxy_server: Option<String>,
    /// When set, inject small randomized delays before input actions so the
    /// agent's timing isn't robotic (defeats cadence-based behavioral tells;
    /// stealth stays honest — it does NOT fake TLS/HTTP2 fingerprints).
    pub human_timing: bool,
    /// When a real `<a href>` click produces no navigation, escalate to a DOM
    /// `.click()` and then a forced `location.href = href`. Off by default:
    /// the escalation re-fires the element's handlers, so a link whose JS
    /// handler already did something (add-to-cart, vote, delete) would run it
    /// up to three times. Enable only for known click-blocking sites.
    pub click_recovery: bool,
}

impl Default for LaunchOptions {
    fn default() -> Self {
        Self {
            executable: None,
            headless: true,
            user_data_dir: None,
            window_size: (1280, 800),
            extra_args: Vec::new(),
            stealth: None,
            proxy_server: None,
            human_timing: false,
            click_recovery: false,
        }
    }
}

/// A running page the agent operates on. Backends translate their native
/// page handle into the operations below.
#[async_trait::async_trait]
pub trait PageOps: Send {
    async fn navigate(&mut self, url: &str) -> Result<Navigated>;
    async fn title(&self) -> Result<String>;
    /// Clean main-content extraction (readability-style) for LLM consumption,
    /// returning `window` of the full text (page with `next_offset`).
    async fn extract(&mut self, window: ExtractWindow) -> Result<Extracted>;
    /// History: go back / forward one entry, reload current document.
    async fn back(&mut self) -> Result<Navigated>;
    async fn forward(&mut self) -> Result<Navigated>;
    async fn reload(&mut self) -> Result<Navigated>;

    /// Compact a11y snapshot with stable `@eN` refs. Refs stay valid across
    /// snapshots until the next navigation.
    async fn snapshot(&mut self) -> Result<Snapshot>;
    /// CSS selector → stable `@eN` refs (clickable immediately). Backends that
    /// cannot resolve selectors return `Unsupported`. CDP routes through
    /// `querySelectorAll` + the snapshot's ref numbering.
    async fn find_by_css(&mut self, selector: &str) -> Result<Vec<ElementRef>> {
        let _ = selector;
        Err(VakError::Unsupported(
            "this backend does not support CSS element resolution".into(),
        ))
    }
    /// Evaluate a JS expression and return its string result.
    async fn eval_text(&self, expression: &str) -> Result<String>;

    async fn click(&mut self, r: &ElementRef) -> Result<ClickResult>;
    /// Set an input's value and fire input/change events (framework-safe).
    async fn fill(&mut self, r: &ElementRef, text: &str) -> Result<()>;
    /// Select an `<option>` by value on a combobox/listbox. Returns whether
    /// the selection took effect.
    async fn select_option(&mut self, r: &ElementRef, value: &str) -> Result<bool>;
    /// Press a named key ("Enter", "Tab", "Escape", "ArrowDown", ...) or a
    /// literal character.
    async fn press_key(&mut self, key: &str) -> Result<()>;
    async fn scroll(&mut self, dx: f64, dy: f64) -> Result<()>;
    /// Poll `expression` until it evaluates truthy or times out.
    async fn wait_for_truthy(&self, expression: &str, timeout_ms: u64) -> Result<()>;

    /// Poll the page URL until `location.href` contains `pattern` (case-sensitive
    /// substring), then return. SPA-safe: `document.readyState` stays
    /// `'complete'` across client-side route changes, so agents must wait on
    /// the URL rather than on `readyState` (the trap this prevents was observed
    /// on the Wikipedia search flow). Times out with `VakError::Timeout`.
    async fn wait_for_url(&self, pattern: &str, timeout_ms: u64) -> Result<()>;

    async fn cookies(&self) -> Result<Vec<Cookie>>;
    async fn set_cookie(&mut self, cookie: &CookieInput) -> Result<()>;
    async fn clear_cookies(&self) -> Result<()>;
    async fn set_download_dir(&mut self, dir: &Path) -> Result<()>;

    /// PNG bytes of the viewport (or full page when `full_page`).
    async fn screenshot(&self, full_page: bool) -> Result<Vec<u8>>;
    /// Click at raw viewport coordinates — the vision fallback when the
    /// a11y tree has no useful refs (canvas, maps).
    async fn click_at(&mut self, x: f64, y: f64) -> Result<()>;
    /// Tools declared by the page via WebMCP (`navigator.modelContext`),
    /// empty when the page/browser doesn't support it.
    async fn webmcp_tools(&self) -> Result<Vec<WebMcpTool>>;
    /// Invoke a page-declared WebMCP tool; errors Unsupported when absent.
    async fn webmcp_invoke(&self, name: &str, arguments_json: &str) -> Result<String>;

    /// Tabs of this session, active tab first.
    /// Lists tabs, first adopting any the page opened itself (`target=_blank`,
    /// `window.open`) and dropping ones it closed.
    async fn tabs(&mut self) -> Result<Vec<TabInfo>>;
    /// Open a new tab, optionally navigating immediately; it becomes active.
    async fn new_tab(&mut self, url: Option<&str>) -> Result<TabInfo>;
    /// Make an existing tab active. Snapshot refs belong to tabs.
    async fn switch_tab(&mut self, tab: &TabId) -> Result<()>;
    /// Close a tab (the last remaining one cannot be closed). Returns
    /// whether it existed.
    async fn close_tab(&mut self, tab: &TabId) -> Result<bool>;

    /// Set files on a file input element (`<input type=file>`). Uses
    /// `DOM.setFileInputFiles` so browser security restrictions on
    /// programmatic `.files` assignment are bypassed. Returns whether the
    /// element was found. Backends without CDP return `Unsupported`.
    async fn set_file_chooser(&mut self, r: &ElementRef, paths: &[String]) -> Result<bool> {
        let _ = (r, paths);
        Err(VakError::Unsupported(
            "this backend does not support file chooser input".into(),
        ))
    }
    /// Current page HTML source (`document.documentElement.outerHTML`).
    async fn source(&self) -> Result<String>;
    /// List completed downloads in the session's download directory.
    async fn downloads(&mut self) -> Result<Vec<DownloadInfo>> {
        Err(VakError::Unsupported(
            "this backend does not support download listing".into(),
        ))
    }
}

#[derive(Debug, Clone)]
pub struct Navigated {
    pub url: String,
    pub title: String,
}

/// Outcome of a `click`. A click that does not navigate (a non-navigating
/// element, or a bot-wall / JS-handler anchor that the click failed to fire)
/// reports `navigated:false` so the agent can branch instead of guessing.
#[derive(Debug, Clone)]
pub struct ClickResult {
    /// Whether the click triggered a (same-process) navigation.
    pub navigated: bool,
    /// The post-click URL when `navigated` is true (the navigated-to page);
    /// `None` otherwise.
    pub url: Option<String>,
    /// Id of a tab the click caused the page to open (`target=_blank`,
    /// `window.open`). It is registered but NOT made active: `switch_tab` to
    /// follow it.
    pub opened_tab: Option<String>,
}
impl ClickResult {
    pub fn navigated(url: String) -> Self {
        Self {
            navigated: true,
            url: Some(url),
            opened_tab: None,
        }
    }
    pub fn stayed() -> Self {
        Self {
            navigated: false,
            url: None,
            opened_tab: None,
        }
    }
}

/// Factory for pages. Implemented by each backend and held by the session
/// manager so the CDP backend is a plug-in, not a baked-in dependency —
/// a different backend would provide a different `EngineLauncher` implementation.
#[async_trait::async_trait]
pub trait EngineLauncher: Send + Sync {
    fn name(&self) -> &'static str;
    /// Ensure an engine binary exists locally, returning its path
    /// (downloading/pinning if supported and needed).
    async fn ensure_executable(&self) -> Result<PathBuf>;
    /// Start a browser and open a first page.
    async fn launch(&self, options: &LaunchOptions) -> Result<Box<dyn PageOps>>;
}

/// Helper used by backends to validate URLs before handing them to a real
/// browser process.
pub fn validate_url(raw: &str) -> Result<url::Url> {
    let parsed = url::Url::parse(raw).map_err(|e| VakError::Engine(format!("bad url: {e}")))?;
    match parsed.scheme() {
        "http" | "https" | "file" | "about" | "data" => Ok(parsed),
        other => Err(VakError::Policy(format!("scheme not allowed: {other}"))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn url_policy() {
        assert!(validate_url("https://example.com").is_ok());
        assert!(validate_url("file:///tmp/x.html").is_ok());
        assert!(validate_url("ftp://example.com").is_err());
    }
}
