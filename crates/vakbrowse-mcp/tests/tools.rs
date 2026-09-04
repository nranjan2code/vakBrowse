//! Hermetic MCP tool tests: drive `VakMcp::tool_call` directly (no
//! transport), against a real offline browser.

use rmcp::model::{CallToolResult, ContentBlock, JsonObject};
use serde_json::{Value, json};
use std::path::PathBuf;
use std::sync::OnceLock;
use url::Url;
use vakbrowse_mcp::VakMcp;

/// Chrome launch is heavy and contends for OS resources in constrained
/// containers; serialize browser-launching integration tests within this
/// binary so parallel `cargo test` stays green on Linux.
fn browser_lock() -> &'static tokio::sync::Semaphore {
    static LOCK: OnceLock<tokio::sync::Semaphore> = OnceLock::new();
    LOCK.get_or_init(|| tokio::sync::Semaphore::new(1))
}

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

/// Collect base64-encoded image data from an `Image` content block, proving
/// screenshot tooling hands back real pixels (not a placeholder string).
fn image_of(result: &CallToolResult) -> Option<String> {
    result
        .content
        .iter()
        .filter_map(|c| match c {
            ContentBlock::Image(i) => Some(i.data.clone()),
            _ => None,
        })
        .next()
}

#[tokio::test]
async fn full_agent_flow_through_mcp_tools() {
    let _g = browser_lock().acquire().await.unwrap();
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
        .tool_call(
            "browser_snapshot",
            args(&[("session", json!(&session))]).as_ref(),
        )
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
            args(&[
                ("session", json!(&session)),
                ("ref", json!(name_ref)),
                ("text", json!("Linus")),
            ]),
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
        let out = server
            .tool_call(call.0, call.1.as_ref())
            .await
            .expect(call.0);
        let t = text_of(out);
        assert!(!t.starts_with("error"), "tool {} said: {}", call.0, t);
    }

    // close
    let out = server
        .tool_call(
            "browser_close",
            args(&[("session", json!(&session))]).as_ref(),
        )
        .await
        .unwrap();
    assert!(text_of(out).contains("closed"));
}

/// One round-trip for many actions: `browser_batch` dispatches a sequence
/// server-side and returns one `[i]`-prefixed result per action (fail-fast
/// on the first error). Proves the batching path end-to-end through MCP.
#[tokio::test]
async fn batch_runs_actions_in_one_roundtrip() {
    let _g = browser_lock().acquire().await.unwrap();
    let server = VakMcp::default();

    let out = server
        .tool_call(
            "browser_open",
            args(&[("url", json!(fixture_url("form.html")))]).as_ref(),
        )
        .await
        .expect("open ok");
    let session = text_of(out)
        .split(' ')
        .nth(1)
        .expect("session id")
        .to_string();

    // Two actions, one call: snapshot then SPA-safe wait_url.
    let actions = json!([
        { "type": "snapshot" },
        { "type": "wait_for_url", "pattern": "form.html", "timeout_ms": 2000 },
    ]);
    let out = server
        .tool_call(
            "browser_batch",
            args(&[("session", json!(&session)), ("actions", actions)]).as_ref(),
        )
        .await
        .expect("batch ok");
    let text = text_of(out);
    // Each result is prefixed with its 0-based index.
    assert!(text.contains("[0]"), "first result indexed: {text}");
    assert!(
        text.contains("@e1"),
        "snapshot rendered in batch [0]: {text}"
    );
    assert!(text.contains("[1] done"), "wait_url result indexed: {text}");

    // Fail-fast: a bad click ref surfaces as an error, not a partial list.
    let bad = json!([
        { "type": "click", "ref": "@stale-Nope" },
        { "type": "snapshot" },
    ]);
    let out = server
        .tool_call(
            "browser_batch",
            args(&[("session", json!(&session)), ("actions", bad)]).as_ref(),
        )
        .await
        .expect("batch error is an Ok(Error) payload");
    let err_text = text_of(out);
    // Fail-fast: the batch surfaced the FIRST action's error (Click on a
    // stale ref) and never ran the second (Snapshot). MCP surfaces app errors
    // as an error result; the body is the ServiceError Display.
    assert!(
        err_text.contains("not found") || err_text.contains("stale"),
        "fail-fast should surface the error: {err_text}"
    );

    server
        .tool_call(
            "browser_close",
            args(&[("session", json!(&session))]).as_ref(),
        )
        .await
        .unwrap();
}

#[tokio::test]
async fn unknown_tool_is_invalid_params() {
    let server = VakMcp::default();
    let err = server.tool_call("browser_fly", None).await.err();
    assert!(err.is_some(), "unknown tool must be rejected");
}

#[tokio::test]
async fn sessions_listing_roundtrip() {
    let server = VakMcp::default();
    let out = server.tool_call("browser_sessions", None).await.unwrap();
    let text = text_of(out);
    assert!(text.contains("(no sessions)") || text.starts_with('s'));
}

#[tokio::test]
async fn browser_screenshot_returns_real_png_blob() {
    let _g = browser_lock().acquire().await.unwrap();
    let server = VakMcp::default();
    let out = server
        .tool_call(
            "browser_open",
            args(&[("url", json!(fixture_url("form.html")))]).as_ref(),
        )
        .await
        .expect("open");
    let session = text_of(out).split(' ').nth(1).unwrap().to_string();

    let out = server
        .tool_call(
            "browser_screenshot",
            args(&[("session", json!(&session))]).as_ref(),
        )
        .await
        .expect("screenshot");
    let b64 = image_of(&out).expect("screenshot must return an Image content block");
    assert!(!b64.is_empty(), "blob must not be empty");
    // The base64 encoding of the 8-byte PNG signature is the canonical
    // header `iVBORw0KGgo` (the trailing `=` is end-of-stream padding and
    // never appears mid-string, so it's omitted). Confirms real raster bytes,
    // not a placeholder string like "(screenshot)".
    assert!(
        b64.starts_with("iVBORw0KGgo"),
        "screenshot did not start with PNG signature, got: {b64}"
    );
}
