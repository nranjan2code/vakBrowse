//! vakbrowse-core: shared domain types for the agent-native browser.

use serde::{Deserialize, Serialize};

pub type Result<T, E = VakError> = std::result::Result<T, E>;

#[derive(Debug, thiserror::Error)]
pub enum VakError {
    #[error("engine error: {0}")]
    Engine(String),
    #[error("protocol error: {0}")]
    Protocol(String),
    #[error("perception error: {0}")]
    Perception(String),
    #[error("policy violation: {0}")]
    Policy(String),
    #[error("not found: {0}")]
    NotFound(String),
    #[error("http error: {0}")]
    Http(String),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("unsupported: {0}")]
    Unsupported(String),
    #[error("timeout: {0}")]
    Timeout(String),
    /// Capacity is exhausted (memory budget, CPU slots, or the admission
    /// queue is full / the wait expired). Nothing was done; retry later.
    #[error("busy: {0}")]
    Busy(String),
}

macro_rules! id_type {
    ($name:ident) => {
        #[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
        pub struct $name(pub String);

        impl $name {
            pub fn new(value: impl Into<String>) -> Self {
                Self(value.into())
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(f, "{}", self.0)
            }
        }
    };
}

id_type!(SessionId);
id_type!(ProfileId);
id_type!(ElementRef);
id_type!(TabId);

/// A browser tab (target) inside a session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TabInfo {
    pub id: TabId,
    pub url: String,
}

/// A stable reference to an interactive element within a snapshot turn,
/// e.g. `@e12`. Refs are reconciled on each snapshot and valid until the
/// page navigates or the snapshot is superseded.
impl ElementRef {
    pub fn parse(raw: &str) -> Option<Self> {
        let body = raw.strip_prefix('@')?;
        let digits = body.strip_prefix('e').unwrap_or(body);
        if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        Some(Self(raw.to_string()))
    }
}

/// Minimal snapshot model; perception lands in P1 but the wire shape is
/// fixed here so all crates agree from day one.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Snapshot {
    pub url: String,
    pub title: String,
    /// Interactive elements in reading order.
    pub elements: Vec<SnapshotNode>,
    /// Page headings, positioned among the elements, so repeated controls
    /// ("Read more" ×20) can be told apart by the section they sit under.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub headings: Vec<Heading>,
    /// Interactive elements dropped because the page exceeded the snapshot
    /// size cap. They have no refs; narrow with `find_by_css` or scroll.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub omitted: usize,
}

fn is_zero(n: &usize) -> bool {
    *n == 0
}

/// A heading seen in the accessibility tree (context only — not actionable).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Heading {
    /// Number of snapshot elements that precede this heading.
    pub before: usize,
    pub level: u8,
    pub text: String,
}

/// A completed download in the session's download directory.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadInfo {
    /// Path to the downloaded file.
    pub path: String,
    /// File size in bytes at the time of the query.
    pub bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotNode {
    pub r#ref: ElementRef,
    pub role: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    #[serde(default)]
    pub clickable: bool,
    /// Widget state: `checked` | `unchecked` | `mixed`, `disabled`,
    /// `expanded` | `collapsed`, `selected`, `required`. Empty when none apply.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub state: Vec<String>,
}

/// A cookie as seen by agents (read shape).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Cookie {
    pub name: String,
    pub value: String,
    pub domain: String,
    #[serde(default)]
    pub path: String,
    #[serde(default)]
    pub secure: bool,
    #[serde(default)]
    pub http_only: bool,
    #[serde(default)]
    pub session: bool,
    /// "Strict" | "Lax" | "None" — None means "unspecified".
    #[serde(default)]
    pub same_site: Option<String>,
    /// Expiry as seconds since the Unix epoch; None for session cookies.
    #[serde(default)]
    pub expires: Option<f64>,
}

/// A cookie an agent may set (write shape).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CookieInput {
    pub name: String,
    pub value: String,
    /// Required for cross-session persistence; use host without port.
    pub domain: String,
    #[serde(default = "default_path")]
    pub path: String,
    #[serde(default)]
    pub secure: bool,
    #[serde(default)]
    pub http_only: bool,
    /// "Strict" | "Lax" | "None". Setting it is required for cross-site
    /// cookies; Chromium rejects same-site-undefined cookies in restricted
    /// modes, so agents must be explicit when they need cross-site.
    #[serde(default)]
    pub same_site: Option<String>,
    /// Expiry as seconds since the Unix epoch. Omit for a session cookie
    /// (which will not survive a restart of a persistent profile).
    #[serde(default)]
    pub expires: Option<f64>,
}

fn default_path() -> String {
    "/".to_string()
}

/// Default `extract` window: ~5k tokens, enough for most articles.
pub const DEFAULT_EXTRACT_CHARS: usize = 20_000;
/// Ceiling on one `extract` window, matching the server's free-text cap;
/// read further with `offset` paging instead.
pub const MAX_EXTRACT_CHARS: usize = 60_000;

/// Which slice of a page's extracted text to return. Offsets and lengths
/// count Unicode scalar values (`char`s), not bytes or UTF-16 units.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExtractWindow {
    pub offset: usize,
    /// Clamped to `1..=MAX_EXTRACT_CHARS`.
    pub max_chars: usize,
}

impl Default for ExtractWindow {
    fn default() -> Self {
        Self {
            offset: 0,
            max_chars: DEFAULT_EXTRACT_CHARS,
        }
    }
}

/// Readable main-content extraction of a page.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Extracted {
    pub title: String,
    pub url: String,
    /// Main-content text, headings preserved as markdown-style lines.
    pub text: String,
    /// More text follows this window; continue from `next_offset`.
    #[serde(default)]
    pub truncated: bool,
    /// Length of the full extracted text, in chars.
    #[serde(default)]
    pub total_chars: usize,
    /// Char offset `text` starts at.
    #[serde(default)]
    pub offset: usize,
    /// Offset to pass to read the next window; `None` at the end.
    #[serde(default)]
    pub next_offset: Option<usize>,
}

impl Extracted {
    /// Cut `window` out of the full extracted `text`, ending on a line break
    /// (else a space) in the window's back half so a page never ends
    /// mid-word; `next_offset` resumes exactly after the consumed text.
    pub fn windowed(title: String, url: String, text: &str, window: ExtractWindow) -> Self {
        let total = text.chars().count();
        let start = window.offset.min(total);
        let max = window.max_chars.clamp(1, MAX_EXTRACT_CHARS);
        let byte_at = |s: &str, n: usize| s.char_indices().nth(n).map_or(s.len(), |(i, _)| i);
        let rest = &text[byte_at(text, start)..];
        let (chunk, next_offset) = if total - start <= max {
            (rest, None)
        } else {
            let span = &rest[..byte_at(rest, max)];
            let half = span.len() / 2;
            let cut = span
                .rfind('\n')
                .filter(|&i| i >= half)
                .or_else(|| span.rfind(' ').filter(|&i| i >= half))
                .map_or(span.len(), |i| i + 1);
            let chunk = &span[..cut];
            (chunk, Some(start + chunk.chars().count()))
        };
        Self {
            title,
            url,
            text: chunk.trim().to_string(),
            truncated: next_offset.is_some(),
            total_chars: total,
            offset: start,
            next_offset,
        }
    }
}

/// A tool declared by a page via the emerging WebMCP standard
/// (`navigator.modelContext`). Present only on pages that opt in.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebMcpTool {
    pub name: String,
    #[serde(default)]
    pub description: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn win(text: &str, offset: usize, max_chars: usize) -> Extracted {
        Extracted::windowed(
            String::new(),
            String::new(),
            text,
            ExtractWindow { offset, max_chars },
        )
    }

    #[test]
    fn extract_window_fits_whole_text() {
        let ex = win("short page", 0, 100);
        assert_eq!(ex.text, "short page");
        assert!(!ex.truncated);
        assert_eq!((ex.total_chars, ex.offset, ex.next_offset), (10, 0, None));
    }

    #[test]
    fn extract_window_cuts_on_line_then_word_boundary() {
        let text = "line one here\nline two here\nline three";
        let ex = win(text, 0, 20);
        assert_eq!(ex.text, "line one here");
        assert_eq!(ex.next_offset, Some(14));
        // No newline in the back half: fall back to a space.
        let ex = win("alpha beta gamma delta", 0, 13);
        assert_eq!(ex.text, "alpha beta");
        assert_eq!(ex.next_offset, Some(11));
        // No boundary at all: hard cut.
        let ex = win("abcdefghij", 0, 4);
        assert_eq!((ex.text.as_str(), ex.next_offset), ("abcd", Some(4)));
    }

    #[test]
    fn extract_window_pages_through_without_loss() {
        let text = "αβγ δεζ\nηθι κλμ\nνξο πρσ\nτυφ χψω";
        let (mut offset, mut pages) = (0, Vec::new());
        loop {
            let ex = win(text, offset, 9);
            assert!(ex.text.chars().count() <= 9);
            pages.push(ex.text);
            match ex.next_offset {
                Some(n) => offset = n,
                None => break,
            }
        }
        let rebuilt: String = pages.join(" ");
        let norm = |s: &str| s.split_whitespace().collect::<Vec<_>>().join(" ");
        assert_eq!(norm(&rebuilt), norm(text));
    }

    #[test]
    fn extract_window_offset_past_end_and_clamped_max() {
        let ex = win("abc", 99, 10);
        assert_eq!((ex.text.as_str(), ex.offset, ex.next_offset), ("", 3, None));
        let long = "x".repeat(MAX_EXTRACT_CHARS + 5);
        let ex = win(&long, 0, usize::MAX);
        assert_eq!(ex.text.len(), MAX_EXTRACT_CHARS);
        assert_eq!(ex.next_offset, Some(MAX_EXTRACT_CHARS));
        assert_eq!(win("abc", 0, 0).text, "a");
    }

    #[test]
    fn element_ref_parse() {
        assert_eq!(
            ElementRef::parse("@e42").map(|r| r.0),
            Some("@e42".to_string())
        );
        assert!(ElementRef::parse("e1").is_none());
        assert!(ElementRef::parse("@").is_none());
        assert!(ElementRef::parse("@ex").is_none());
    }

    #[test]
    fn ids_roundtrip() {
        let s = SessionId::new("sess-1");
        let de: SessionId = serde_json::from_str(&serde_json::to_string(&s).unwrap()).unwrap();
        assert_eq!(s, de);
    }
}
