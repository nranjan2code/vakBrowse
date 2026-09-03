//! Real stdio JSON-RPC transport test for `vak-mcp`.
//!
//! The existing `tools.rs` integration tests drive `VakMcp::tool_call` in-process
//! and therefore never exercise the stdio framing / init handshake that real
//! Claude/Cursor/opencode clients go through. This test spawns the `vak-mcp`
//! binary as a subprocess and drives it over real stdio with a minimal,
//! dependency-free NDJSON framing client.
//!
//! ## Why NDJSON, not Content-Length
//!
//! rmcp 3.1.4's stdio transport is **newline-delimited JSON-RPC** on both
//! directions: it encodes each message as `{"jsonrpc":...}\n` (see
//! `JsonRpcMessageCodec::encode` in rmcp's `async_rw.rs`, which appends `b'\n'`)
//! and decodes by scanning for a `\n` delimiter, parsing each line as JSON and
//! silently skipping any line that isn't valid JSON. It does **not** implement
//! Content-Length framing. A Content-Length-framed client sends the header
//! (`Content-Length: 266`) as its own line, which rmcp skips, then sends the JSON
//! body with no trailing newline — so the server never sees a complete line
//! until stdin hits EOF, and a streaming client that keeps stdin open hangs
//! forever waiting for a response it will only emit at EOF.
//!
//! The official `mcp` Python SDK uses this same NDJSON wire format: it writes
//! `json + "\n"` (no Content-Length) and reads by splitting on `\n` (see
//! `mcp/client/stdio.py`). So this client mirrors it exactly: `protocolVersion`
//! `"2025-11-25"` with an empty `_meta: {}` (SEP-2575 `_meta` keys are only
//! required for protocolVersion >= 2026-07-28; for earlier versions an empty
//! `_meta` is sufficient). This guards the real wire contract.
use std::path::PathBuf;
use std::time::Duration;

use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{ChildStdin, Command};
use tokio::time::timeout;
use url::Url;

/// Protocol version offered to the server. Matches the official `mcp` SDK
/// (2025-11-25), which rmcp 3.1.4 accepts; for any version < 2026-07-28 an
/// empty `_meta` satisfies the server.
const PROTO: &str = "2025-11-25";

/// params map carrying an empty `_meta` block, mirroring the `mcp` SDK.
fn params_with_meta(args: serde_json::Map<String, Value>) -> serde_json::Map<String, Value> {
    let mut p = args;
    p.insert("_meta".to_string(), Value::Object(serde_json::Map::new()));
    p
}

fn request(id: u64, method: &str, args: serde_json::Map<String, Value>) -> Value {
    serde_json::json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": method,
        "params": serde_json::Value::Object(params_with_meta(args)),
    })
}

fn notification(method: &str) -> Value {
    serde_json::json!({ "jsonrpc": "2.0", "method": method })
}

/// A `tools/call` request nests the tool `name` and `arguments` object inside
/// `params` (rmcp extracts `arguments` as the handler's `args` map).
fn tool_call(id: u64, name: &str, arguments: serde_json::Map<String, Value>) -> Value {
    let mut args = serde_json::Map::new();
    args.insert("name".to_string(), Value::String(name.to_string()));
    args.insert("arguments".to_string(), Value::Object(arguments));
    request(id, "tools/call", args)
}

fn fixture_url(name: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures")
        .join(name);
    Url::from_file_path(path.canonicalize().expect("fixture exists"))
        .expect("absolute")
        .to_string()
}

fn tool_names(resp: &Value) -> Vec<String> {
    resp.get("result")
        .and_then(|r| r.get("tools").and_then(|t| t.as_array()))
        .map(|arr| {
            arr.iter()
                .filter_map(|t| t.get("name").and_then(|n| n.as_str()).map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

fn text_of(resp: &Value) -> String {
    let content: Vec<Value> = resp
        .get("result")
        .and_then(|r| r.get("content"))
        .and_then(|c| c.as_array())
        .cloned()
        .unwrap_or_default();
    content
        .iter()
        .filter_map(|c| c.get("text").and_then(|t| t.as_str()).map(str::to_string))
        .collect::<Vec<_>>()
        .join("\n")
}

fn image_b64(resp: &Value) -> String {
    resp.get("result")
        .and_then(|r| r.get("content"))
        .and_then(|c| c.as_array())
        .and_then(|arr| {
            arr.iter().find(|c| {
                c.get("type").and_then(|t| t.as_str()) == Some("image")
                    && c.get("mimeType").and_then(|m| m.as_str()) == Some("image/png")
            })
        })
        .and_then(|c| c.get("data").and_then(|d| d.as_str()).map(str::to_string))
        .unwrap_or_default()
}

/// Minimal stateful NDJSON MCP stdio client. Writes one JSON object per message
/// (newline-terminated, no Content-Length header) and reads the next
/// *response* — a message carrying an `id` — skipping any unsolicited
/// notifications the server emits between responses.
struct NdjsonStdio {
    stdin: ChildStdin,
    reader: BufReader<tokio::process::ChildStdout>,
}

impl NdjsonStdio {
    async fn send(&mut self, msg: &Value) {
        let mut bytes = serde_json::to_vec(&msg).expect("serializable json-rpc");
        bytes.push(b'\n');
        eprintln!(
            "[stdio] -> id={:?} method={:?}",
            msg.get("id"),
            msg.get("method")
        );
        timeout(Duration::from_secs(5), self.stdin.write_all(&bytes))
            .await
            .expect("timeout writing to MCP server")
            .expect("error writing to MCP server");
        timeout(Duration::from_secs(5), self.stdin.flush())
            .await
            .expect("timeout flushing to MCP server")
            .expect("error flushing to MCP server");
    }

    /// Read the next response (a JSON-RPC message carrying an `id`). Id-less
    /// messages are server notifications and are skipped.
    async fn recv_resp(&mut self) -> Value {
        loop {
            let mut line = Vec::new();
            let n = timeout(
                Duration::from_secs(15),
                self.reader.read_until(b'\n', &mut line),
            )
            .await
            .expect("timeout reading MCP line")
            .expect("error reading MCP line");
            if n == 0 {
                panic!(
                    "server closed stdout before a response; buffered: {:?}",
                    String::from_utf8_lossy(&line)
                );
            }
            let line = line.strip_suffix(b"\n").unwrap_or(&line);
            let value: Value = serde_json::from_slice(line).unwrap_or_else(|e| {
                panic!(
                    "unparsable MCP message: {e}; bytes={:?}",
                    String::from_utf8_lossy(line)
                )
            });
            if value.get("id").is_some() {
                return value;
            }
            eprintln!("[stdio] (skip notification) {:?}", value.get("method"));
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn mcp_stdio_handshake_full_agent_flow() {
    // Spawn the real `vak-mcp` binary; no env allowlist => empty prefix list
    // => allow all (scheme still gated by validate_url, which permits file://).
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let bin = format!("{manifest_dir}/../../target/debug/vak-mcp");
    // Chrome emits sandbox/GPU warnings on stderr; a piped stderr that we never
    // drain would fill the 64KB OS pipe and deadlock the child. Redirect to a
    // file instead so vak-mcp never blocks on stderr writes.
    let stderr_file =
        std::fs::File::create("/tmp/vakmcp_stdiotest_stderr.log").expect("stderr log file");
    let mut child = Command::new(&bin)
        .kill_on_drop(true)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::from(stderr_file))
        .env("RUST_LOG", "rmcp=debug")
        .spawn()
        .unwrap_or_else(|e| panic!("failed to spawn {bin}: {e}"));
    let stdin = child.stdin.take().expect("stdin piped");
    let stdout = child.stdout.take().expect("stdout piped");
    let mut io = NdjsonStdio {
        stdin,
        reader: BufReader::new(stdout),
    };

    // 1) initialize
    io.send(
        &serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": PROTO,
                "capabilities": {},
                "clientInfo": { "name": "vak-test", "version": "1" },
                "_meta": {},
            },
        }),
    )
    .await;
    let init = io.recv_resp().await;
    assert_eq!(
        init.get("id").and_then(|i| i.as_u64()),
        Some(1),
        "initialize response id mismatch: {init}"
    );
    assert_eq!(
        init.get("error"),
        None,
        "initialize returned an error: {init}"
    );
    let server_info = init
        .get("result")
        .and_then(|r| r.get("serverInfo"))
        .and_then(|s| s.get("name").and_then(|n| n.as_str()));
    assert_eq!(server_info, Some("vakBrowse"), "init result: {init}");

    // 2) initialized notification (NDJSON; rmcp does not require this to be
    //    acknowledged, but real clients send it to complete the handshake).
    io.send(&notification("notifications/initialized")).await;

    // 3) tools/list
    io.send(&request(2, "tools/list", serde_json::Map::new())).await;
    let tools = io.recv_resp().await;
    assert_eq!(tools.get("error"), None, "tools/list error: {tools}");
    let names = tool_names(&tools);
    assert!(
        names.len() >= 24,
        "expected >= 24 MCP tools, got {}: {names:?}",
        names.len()
    );
    for must in ["browser_open", "browser_snapshot", "browser_screenshot"] {
        assert!(names.iter().any(|n| n == must), "missing tool {must}");
    }

    // 4) browser_open (hermetic file:// fixture)
    let mut args = serde_json::Map::new();
    args.insert("url".to_string(), Value::String(fixture_url("form.html")));
    io.send(&tool_call(3, "browser_open", args)).await;
    let open = io.recv_resp().await;
    let open_txt = text_of(&open);
    assert!(open_txt.contains("open"), "browser_open: {open_txt}");
    let sid = open_txt
        .split_whitespace()
        .nth(1)
        .expect("session id in 'session <id> open' line")
        .to_string();

    // 5) browser_snapshot -> a11y @e1 refs
    let mut args = serde_json::Map::new();
    args.insert("session".to_string(), Value::String(sid.clone()));
    io.send(&tool_call(4, "browser_snapshot", args)).await;
    let snap = io.recv_resp().await;
    let snap_txt = text_of(&snap);
    assert!(snap_txt.contains("@e1"), "snapshot should expose @e1: {snap_txt}");
    assert!(
        snap_txt.contains("Send") || snap_txt.contains("send") || snap_txt.contains("form"),
        "snapshot should reflect form fixture: {snap_txt}"
    );

    // 6) browser_screenshot -> real PNG blob (not a placeholder)
    let mut args = serde_json::Map::new();
    args.insert("session".to_string(), Value::String(sid.clone()));
    io.send(&tool_call(5, "browser_screenshot", args)).await;
    let shot = io.recv_resp().await;
    let png = image_b64(&shot);
    assert!(
        png.starts_with("iVBORw0KGgo"),
        "screenshot must be a real PNG, got: {png:.40}..."
    );

    // 7) browser_close
    let mut args = serde_json::Map::new();
    args.insert("session".to_string(), Value::String(sid));
    io.send(&tool_call(6, "browser_close", args)).await;
    let close = io.recv_resp().await;
    assert!(
        text_of(&close).to_lowercase().contains("clos"),
        "close: {}",
        text_of(&close)
    );

    drop(io);
    let _ = timeout(Duration::from_secs(5), child.wait()).await;
}
