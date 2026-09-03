//! Real stdio JSON-RPC transport test for `vak-mcp`.
//!
//! The existing `tools.rs` integration tests drive `VakMcp::tool_call` in-process
//! and therefore never exercise the stdio framing / init handshake that real
//! Claude/Cursor/opencode clients go through. These tests spawn the `vak-mcp`
//! binary as a subprocess and drive it over real stdio.
//!
//! ## Why vak-mcp accepts TWO framing styles
//!
//! rmcp 3.1.4's stdio transport is **newline-delimited JSON-RPC** (it encodes
//! each message as `{"jsonrpc":...}\n` via `JsonRpcMessageCodec::encode` in
//! rmcp's `async_rw.rs`, and decodes by scanning for a `\n` delimiter, parsing
//! each line as JSON and silently skipping non-JSON lines). It does **not**
//! implement Content-Length framing. Both official SDKs actually speak NDJSON
//! (the `mcp` Python SDK writes `json + "\n"` and reads on `\n`; the TypeScript
//! SDK writes `JSON.stringify(m) + '\n'` and splits reads on `'\n'` — neither
//! sends Content-Length), so rmcp's NDJSON transport is interoperable with them
//! as-is. BUT the MCP spec *text* describes `Content-Length: N\r\n\r\n<bytes>`
//! framing, and some spec-literate/legacy clients send that. To be spec-robust,
//! `vak-mcp`'s stdin is wrapped in `vakbrowse_mcp::stdio_framer::StdioFramer`,
//! which normalizes **incoming** Content-Length blocks (and NDJSON) into
//! newline-delimited JSON; **outgoing** responses stay NDJSON, which every SDK
//! reads. The two tests below drive the same handshake+agent-flow once with an
//! NDJSON client (mirrors the `mcp` Python + TS SDKs) and once with a
//! Content-Length client (mirrors spec-literal clients) — both must succeed.
//!
//! Clients offer `protocolVersion` `"2025-11-25"` with an empty `_meta: {}`.
//! SEP-2575 `_meta` keys are only required for protocolVersion >= 2026-07-28;
//! for earlier versions an empty `_meta` satisfies the server (rmcp's
//! `missing_required_keys` returns empty for pre-2026-07-28 protocols).
use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{ChildStdin, Command};
use url::Url;

use std::path::PathBuf;
use std::time::Duration;

/// Protocol version offered to the server. Matches the official `mcp` SDK
/// (2025-11-25), which rmcp 3.1.4 accepts.
const PROTO: &str = "2025-11-25";

#[derive(Debug, Clone, Copy)]
enum Framing {
    Ndjson,
    ContentLength,
}

/// Minimal MCP stdio client supporting BOTH write framings. The response side
/// is always NDJSON: `vak-mcp` emits NDJSON (the StdioFramer only rewrites the
/// *incoming* (stdin) direction; rmcp's NDJSON encoder writes responses).
struct McpStdio {
    stdin: ChildStdin,
    reader: BufReader<tokio::process::ChildStdout>,
    framing: Framing,
}

impl McpStdio {
    fn new(stdin: ChildStdin, stdout: tokio::process::ChildStdout, framing: Framing) -> Self {
        Self {
            stdin,
            reader: BufReader::new(stdout),
            framing,
        }
    }

    async fn send(&mut self, msg: &Value) {
        let body = serde_json::to_vec(&msg).expect("serializable json-rpc");
        let bytes: Vec<u8> = match self.framing {
            Framing::Ndjson => {
                let mut b = body;
                b.push(b'\n');
                b
            }
            Framing::ContentLength => {
                let header = format!("Content-Length: {}\r\n\r\n", body.len());
                let mut b = header.into_bytes();
                b.extend(body);
                b
            }
        };
        eprintln!(
            "[stdio] -> id={:?} method={:?} framing={:?}",
            msg.get("id"),
            msg.get("method"),
            self.framing
        );
        tokio::time::timeout(Duration::from_secs(5), self.stdin.write_all(&bytes))
            .await
            .expect("timeout writing to MCP server")
            .expect("error writing to MCP server");
        tokio::time::timeout(Duration::from_secs(5), self.stdin.flush())
            .await
            .expect("timeout flushing to MCP server")
            .expect("error flushing to MCP server");
    }

    /// Read the next response (a JSON-RPC message carrying an `id`). Id-less
    /// messages are server notifications and are skipped.
    async fn recv_resp(&mut self) -> Value {
        loop {
            let mut line = Vec::new();
            let n = tokio::time::timeout(
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

/// Spawn the real `vak-mcp` binary (stderr redirected to a file to avoid the
/// 64KB pipe-fill deadlock that Chrome's sandbox/GPU warnings would cause).
fn spawn_vak_mcp() -> tokio::process::Child {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let bin = format!("{manifest_dir}/../../target/debug/vak-mcp");
    let stderr_file =
        std::fs::File::create("/tmp/vakmcp_stdiotest_stderr.log").expect("stderr log file");
    Command::new(&bin)
        .kill_on_drop(true)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::from(stderr_file))
        .env("RUST_LOG", "rmcp=debug")
        .spawn()
        .unwrap_or_else(|e| panic!("failed to spawn {bin}: {e}"))
}

/// The full agent-style flow driven identically for both framings:
/// init -> initialized -> tools/list -> open form.html -> snapshot (@e1) ->
/// screenshot (real PNG) -> close.
async fn drive_full_agent_flow(io: &mut McpStdio) {
    // 1) initialize
    io.send(&serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": PROTO,
            "capabilities": {},
            "clientInfo": { "name": "vak-test", "version": "1" },
            "_meta": {},
        },
    }))
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
    assert_eq!(
        init.get("result")
            .and_then(|r| r.get("serverInfo"))
            .and_then(|s| s.get("name").and_then(|n| n.as_str())),
        Some("vakBrowse"),
        "init result: {init}"
    );

    // 2) initialized notification (completes the handshake)
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
}

#[tokio::test(flavor = "multi_thread")]
async fn mcp_stdio_ndjson_client() {
    let mut child = spawn_vak_mcp();
    let mut io = McpStdio::new(
        child.stdin.take().expect("stdin piped"),
        child.stdout.take().expect("stdout piped"),
        Framing::Ndjson,
    );
    drive_full_agent_flow(&mut io).await;
    drop(io);
    let _ = tokio::time::timeout(Duration::from_secs(5), child.wait()).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn mcp_stdio_content_length_client() {
    let mut child = spawn_vak_mcp();
    let mut io = McpStdio::new(
        child.stdin.take().expect("stdin piped"),
        child.stdout.take().expect("stdout piped"),
        Framing::ContentLength,
    );
    drive_full_agent_flow(&mut io).await;
    drop(io);
    let _ = tokio::time::timeout(Duration::from_secs(5), child.wait()).await;
}
