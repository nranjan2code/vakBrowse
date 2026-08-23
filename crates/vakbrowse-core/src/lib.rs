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
}

fn default_path() -> String {
    "/".to_string()
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
