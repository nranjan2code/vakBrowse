//! Hermetic MCP tool tests: drive `VakMcp::tool_call` directly (no
//! transport), against a real offline browser.

use rmcp::model::{CallToolResult, ContentBlock, JsonObject};
use serde_json::{Value, json};
use std::path::PathBuf;
use url::Url;
use vakbrowse_mcp::VakMcp;

fn fixture_url(name: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures")
        .join(name);
    Url::from_file_path(path.canonicalize().expect("fixture exists"))
        .expect("absolute")
        .to_string()
}

fn args(pairs: &[(&str, Value)]) -> Option<JsonObject> {
    let mut map = JsonObject::new();
    for (k, v) in pairs {
        map.insert(k.to_string(), v.clone());
    }
    Some(map)
}

fn text_of(result: CallToolResult) -> String {
    result
        .content
        .iter()
        .filter_map(|c| match c {
            ContentBlock::Text(t) => Some(t.text.clone()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[tokio::test]
async fn full_agent_flow_through_mcp_tools() {
    let server = VakMcp::default();

    // open + navigate
    let out = server
        .tool_call(
            "browser_open",
            args(&[("url", json!(fixture_url("form.html")))]).as_ref(),
        )
        .await
        .expect("tool_call ok");
    let text = text_of(out);
    let session = text
        .split(' ')
        .nth(1)
        .expect("session id in response")
        .to_string();
    assert!(text.contains("open"), "{text}");

    // snapshot: agent's eyes
    let out = server
        .tool_call("browser_snapshot", args(&[("session", json!(&session))]).as_ref())
        .await
        .unwrap_or_else(|e| panic!("snapshot failed {e}"));
    let snap_text = text_of(out);
    assert!(snap_text.contains("@e1"), "{snap_text}");
    assert!(snap_text.contains("Send application"), "{snap_text}");

    // find refs by parsing the snapshot lines
    let ref_for = |label: &str| -> String {
        snap_text
            .lines()
            .find(|l| l.contains(label))
            .and_then(|l| l.split('\t').next())
            .unwrap()
            .to_string()
    };
    let name_ref = ref_for("Name");
    let send_ref = ref_for("Send");

    for call in [
        (
            "browser_fill",
            args(&[("session", json!(&session)), ("ref", json!(name_ref)), ("text", json!("Linus"))]),
        ),
        (
            "browser_click",
            args(&[("session", json!(&session)), ("ref", json!(send_ref))]),
        ),
        (
            "browser_wait",
            args(&[
                ("session", json!(&session)),
                (
                    "expression",
                    json!("document.getElementById('out').textContent.includes('name=Linus')"),
                ),
                ("timeout_ms", json!(3000)),
            ]),
        ),
    ] {
        let out = server.tool_call(call.0, call.1.as_ref()).await.expect(call.0);
        let t = text_of(out);
        assert!(!t.starts_with("error"), "tool {} said: {}", call.0, t);
    }

    // close
    let out = server
        .tool_call("browser_close", args(&[("session", json!(&session))]).as_ref())
        .await
        .unwrap();
    assert!(text_of(out).contains("closed"));
}

#[tokio::test]
async fn unknown_tool_is_invalid_params() {
    let server = VakMcp::default();
    let err = server
        .tool_call("browser_fly", None)
        .await
        .err();
    assert!(err.is_some(), "unknown tool must be rejected");
}

#[tokio::test]
async fn sessions_listing_roundtrip() {
    let server = VakMcp::default();
    let out = server.tool_call("browser_sessions", None).await.unwrap();
    let text = text_of(out);
    assert!(text.contains("(no sessions)") || text.starts_with('s'));
}
