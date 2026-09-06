//! Hermetic HTTP + WebSocket tests: real axum server on an ephemeral port,
//! real browser against local fixtures.

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::OnceLock;

use futures::{SinkExt, StreamExt};
use serde_json::{Value, json};
use tokio::sync::Semaphore;
use url::Url;
use vakbrowse_server::SessionManager;

/// Chrome launch is heavy and contends for OS resources in constrained
/// containers; serialize browser-launching integration tests within this
/// binary so parallel `cargo test` stays green on Linux.
fn browser_lock() -> &'static Semaphore {
    static LOCK: OnceLock<Semaphore> = OnceLock::new();
    LOCK.get_or_init(|| Semaphore::new(1))
}

fn fixture_url(name: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures")
        .join(name);
    Url::from_file_path(path.canonicalize().expect("fixture exists"))
        .expect("absolute")
        .to_string()
}

async fn spawn_server() -> (SocketAddr, tempfile::TempDir) {
    // Isolated profiles root so tests never touch real user data.
    let dir = tempfile::tempdir().unwrap();
    let manager = SessionManager::default().with_profiles_root(dir.path().to_path_buf());
    let app = vakbrowse_api::build_router(Arc::new(manager));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.expect("serve");
    });
    (addr, dir)
}

#[tokio::test]
async fn health_and_open_snapshot_close_over_http() {
    let _g = browser_lock().acquire().await.unwrap();
    let (addr, _dir) = spawn_server().await;
    let base = format!("http://{addr}");
    let client = reqwest::Client::new();

    assert_eq!(
        client
            .get(format!("{base}/health"))
            .send()
            .await
            .unwrap()
            .text()
            .await
            .unwrap(),
        "ok"
    );

    // Open with initial navigation.
    let resp = client
        .post(format!("{base}/sessions"))
        .json(&json!({ "url": fixture_url("hello.html") }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201);
    let body: Value = resp.json().await.unwrap();
    let session = body["Ok"]["Opened"]["id"]
        .as_str()
        .expect("session id shape")
        .to_string();

    // Dispatch a snapshot action.
    let resp = client
        .post(format!("{base}/sessions/{session}/actions"))
        .json(&json!({ "type": "snapshot" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();
    let elements = body["Ok"]["Result"]["snapshot"]["elements"]
        .as_array()
        .expect("elements array");
    assert!(elements.iter().any(|e| e["role"] == "link"));

    // Policy-free manager allows navigation anywhere.
    // DNS/engine failure (`.invalid` TLD never resolves) surfaces as an
    // Error payload mapped to 502 — a transient gateway error the agent
    // may retry, not a 400 bad-request.
    let resp = client
        .post(format!("{base}/sessions/{session}/actions"))
        .json(&json!({ "type": "navigate", "url": "https://blocked.invalid/" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 502);
    let body: Value = resp.json().await.unwrap();
    assert!(
        body["Ok"]["Error"].is_object(),
        "expected structured application error, got {body}"
    );

    // Close.
    let resp = client
        .delete(format!("{base}/sessions/{session}"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
}

#[tokio::test]
async fn ws_bridge_roundtrip() {
    let _g = browser_lock().acquire().await.unwrap();
    let (addr, _dir) = spawn_server().await;
    let (mut ws, _) = tokio_tungstenite_connect(format!("ws://{addr}/ws")).await;

    let open_req = json!({
        "type": "open",
        "options": { "url": fixture_url("form.html") }
    });
    send_ws_text(&mut ws, open_req.to_string()).await;
    let reply = recv_ws_text(&mut ws).await;
    let session = reply["Ok"]["Opened"]["id"]
        .as_str()
        .unwrap_or_else(|| panic!("session id shape; open reply: {reply}"))
        .to_string();

    send_ws_text(
        &mut ws,
        json!({ "type": "act", "session": session.clone(), "action": { "type": "snapshot" } })
            .to_string(),
    )
    .await;
    let reply = recv_ws_text(&mut ws).await;
    let text = serde_json::to_string(&reply).unwrap();
    assert!(text.contains("combobox"), "snapshot via ws: {text}");

    send_ws_text(
        &mut ws,
        json!({ "type": "close", "session": session }).to_string(),
    )
    .await;
    let reply = recv_ws_text(&mut ws).await;
    assert_eq!(
        reply["Ok"]["Closed"],
        true,
        "close reply was {}",
        serde_json::to_string(&reply).unwrap()
    );
}

/// `POST /sessions/{session}/batch`: one round-trip, N actions, fail-fast.
#[tokio::test]
async fn batch_actions_over_http() {
    let _g = browser_lock().acquire().await.unwrap();
    let (addr, _dir) = spawn_server().await;
    let base = format!("http://{addr}");
    let client = reqwest::Client::new();

    let resp = client
        .post(format!("{base}/sessions"))
        .json(&json!({ "url": fixture_url("form.html") }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201);
    let session = resp.json::<Value>().await.unwrap()["Ok"]["Opened"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let resp = client
        .post(format!("{base}/sessions/{session}/batch"))
        .json(&json!([
            { "type": "snapshot" },
            { "type": "wait_for_url", "pattern": "form.html", "timeout_ms": 2000 },
        ]))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200, "batch should succeed");
    let body = resp.json::<Value>().await.unwrap();
    let results = &body["Ok"]["Results"];
    assert_eq!(
        results.as_array().expect("results array").len(),
        2,
        "{body}"
    );
    let merged = serde_json::to_string(&body).unwrap();
    // [0] snapshot contains form inputs, [1] wait_url resolves to Done.
    assert!(merged.contains("textbox"), "[0] should snapshot: {merged}");
    assert!(
        merged.contains("\"done\""),
        "[1] should be wait_url done: {merged}"
    );

    client
        .delete(format!("{base}/sessions/{session}"))
        .send()
        .await
        .unwrap();
}

/// `Request::Batch` flows through the WebSocket bridge (generic dispatch).
#[tokio::test]
async fn ws_batch_roundtrip() {
    let _g = browser_lock().acquire().await.unwrap();
    let (addr, _dir) = spawn_server().await;
    let (mut ws, _) = tokio_tungstenite_connect(format!("ws://{addr}/ws")).await;

    send_ws_text(
        &mut ws,
        json!({
            "type": "open", "options": { "url": fixture_url("form.html") }
        })
        .to_string(),
    )
    .await;
    let session = recv_ws_text(&mut ws).await["Ok"]["Opened"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    send_ws_text(
        &mut ws,
        json!({
            "type": "batch",
            "session": session,
            "actions": [
                { "type": "snapshot" },
                { "type": "wait_for_url", "pattern": "form.html", "timeout_ms": 2000 }
            ]
        })
        .to_string(),
    )
    .await;
    let reply = recv_ws_text(&mut ws).await;
    let text = serde_json::to_string(&reply).unwrap();
    let results = reply["Ok"]["Results"].as_array().expect("results array");
    assert_eq!(results.len(), 2, "batch via ws: {text}");
    assert!(text.contains("textbox"), "[0] snapshot over ws: {text}");
    assert!(
        text.contains("\"done\""),
        "[1] wait_url done over ws: {text}"
    );

    send_ws_text(
        &mut ws,
        json!({ "type": "close", "session": session }).to_string(),
    )
    .await;
    let _ = recv_ws_text(&mut ws).await;
}

/// The `click` navigation signal survives the WS bridge: an anchor click that
/// navigates returns `Result.Clicked { navigated:true, url }` over the wire.
#[tokio::test]
async fn click_signal_over_ws() {
    let _g = browser_lock().acquire().await.unwrap();
    let (addr, _dir) = spawn_server().await;
    let (mut ws, _) = tokio_tungstenite_connect(format!("ws://{addr}/ws")).await;

    send_ws_text(
        &mut ws,
        json!({ "type": "open", "options": { "url": fixture_url("links.html") } }).to_string(),
    )
    .await;
    let session = recv_ws_text(&mut ws).await["Ok"]["Opened"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    send_ws_text(
        &mut ws,
        json!({ "type": "act", "session": session, "action": { "type": "snapshot" } }).to_string(),
    )
    .await;
    let snap = recv_ws_text(&mut ws).await;
    let link_ref = snap["Ok"]["Result"]["snapshot"]["elements"]
        .as_array()
        .expect("elements")
        .iter()
        .find(|e| e["role"] == "link" && e["name"] == "Go to the form")
        .and_then(|e| e["ref"].as_str())
        .expect("navigating link ref")
        .to_string();

    send_ws_text(
        &mut ws,
        json!({ "type": "act", "session": session, "action": { "type": "click", "ref": link_ref } }).to_string(),
    )
    .await;
    let reply = recv_ws_text(&mut ws).await;
    let text = serde_json::to_string(&reply).unwrap();
    assert_eq!(
        reply["Ok"]["Result"]["navigated"], true,
        "click should navigate over ws: {text}"
    );
    assert!(
        reply["Ok"]["Result"]["url"]
            .as_str()
            .unwrap()
            .ends_with("form.html"),
        "landed url: {text}"
    );

    send_ws_text(
        &mut ws,
        json!({ "type": "close", "session": session }).to_string(),
    )
    .await;
    let _ = recv_ws_text(&mut ws).await;
}

/// The `click` navigation signal survives the REST/HTTP boundary too.
#[tokio::test]
async fn click_navigates_over_http() {
    let _g = browser_lock().acquire().await.unwrap();
    let (addr, _dir) = spawn_server().await;
    let base = format!("http://{addr}");
    let client = reqwest::Client::new();

    let session = client
        .post(format!("{base}/sessions"))
        .json(&json!({ "url": fixture_url("links.html") }))
        .send()
        .await
        .unwrap()
        .json::<Value>()
        .await
        .unwrap()["Ok"]["Opened"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let snap_body = client
        .post(format!("{base}/sessions/{session}/actions"))
        .json(&json!({ "type": "snapshot" }))
        .send()
        .await
        .unwrap()
        .json::<Value>()
        .await
        .unwrap();
    let elements = snap_body["Ok"]["Result"]["snapshot"]["elements"]
        .as_array()
        .expect("elements")
        .iter()
        .find(|e| e["role"] == "link" && e["name"] == "Go to the form")
        .and_then(|e| e["ref"].as_str())
        .expect("navigating link ref")
        .to_string();

    let body = client
        .post(format!("{base}/sessions/{session}/actions"))
        .json(&json!({ "type": "click", "ref": elements }))
        .send()
        .await
        .unwrap()
        .json::<Value>()
        .await
        .unwrap();
    assert_eq!(
        body["Ok"]["Result"]["navigated"], true,
        "click should navigate over http: {body}"
    );
    assert!(
        body["Ok"]["Result"]["url"]
            .as_str()
            .unwrap()
            .ends_with("form.html"),
        "landed url: {body}"
    );

    client
        .delete(format!("{base}/sessions/{session}"))
        .send()
        .await
        .unwrap();
}

/// The new `Clicked` shape round-trips the WebSocket bridge for a non-navigating
/// (javascript:) anchor too — `navigated:false`, over the wire.
#[tokio::test]
async fn click_no_navigation_over_ws() {
    let _g = browser_lock().acquire().await.unwrap();
    let (addr, _dir) = spawn_server().await;
    let (mut ws, _) = tokio_tungstenite_connect(format!("ws://{addr}/ws")).await;

    send_ws_text(
        &mut ws,
        json!({ "type": "open", "options": { "url": fixture_url("links.html") } }).to_string(),
    )
    .await;
    let session = recv_ws_text(&mut ws).await["Ok"]["Opened"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    send_ws_text(
        &mut ws,
        json!({ "type": "act", "session": session, "action": { "type": "snapshot" } }).to_string(),
    )
    .await;
    let snap = recv_ws_text(&mut ws).await;
    let link_ref = snap["Ok"]["Result"]["snapshot"]["elements"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["role"] == "link" && e["name"] == "no-op link")
        .and_then(|e| e["ref"].as_str())
        .expect("no-op link ref")
        .to_string();

    send_ws_text(
        &mut ws,
        json!({ "type": "act", "session": session, "action": { "type": "click", "ref": link_ref } }).to_string(),
    )
    .await;
    let reply = recv_ws_text(&mut ws).await;
    let text = serde_json::to_string(&reply).unwrap();
    assert_eq!(reply["Ok"]["Result"]["navigated"], false, "{text}");
    assert_eq!(
        reply["Ok"]["Result"]["url"],
        serde_json::Value::Null,
        "{text}"
    );

    send_ws_text(
        &mut ws,
        json!({ "type": "close", "session": session }).to_string(),
    )
    .await;
    let _ = recv_ws_text(&mut ws).await;
}
/// The unified `/playground/rpc` endpoint mirrors the daemon's UDS protocol:
/// same `Request` JSON in, same `Response` JSON out. This test verifies the
/// open → snapshot → close round-trip goes through the single RPC endpoint.
#[tokio::test]
async fn playground_rpc_roundtrips_request_model() {
    let _g = browser_lock().acquire().await.unwrap();
    let (addr, _dir) = spawn_server().await;
    let base = format!("http://{addr}");
    let client = reqwest::Client::new();

    // Open via the unified Request model.
    let resp = client
        .post(format!("{base}/playground/rpc"))
        .json(&json!({ "type": "open", "options": { "url": fixture_url("hello.html") } }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201);
    let session = resp.json::<Value>().await.unwrap()["Ok"]["Opened"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    // Snapshot via the unified Request model.
    let resp = client
        .post(format!("{base}/playground/rpc"))
        .json(&json!({ "type": "act", "session": session, "action": { "type": "snapshot" } }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body = resp.json::<Value>().await.unwrap();
    let elements = body["Ok"]["Result"]["snapshot"]["elements"]
        .as_array()
        .expect("elements array");
    assert!(elements.iter().any(|e| e["role"] == "link"), "should find a link: {body}");

    // Close via the unified Request model.
    let resp = client
        .post(format!("{base}/playground/rpc"))
        .json(&json!({ "type": "close", "session": session }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    assert_eq!(
        resp.json::<Value>().await.unwrap()["Ok"]["Closed"],
        true,
    );

    // ListSessions via the unified Request model.
    let resp = client
        .post(format!("{base}/playground/rpc"))
        .json(&json!({ "type": "list_sessions" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body = resp.json::<Value>().await.unwrap();
    assert_eq!(
        body["Ok"]["Sessions"].as_array().unwrap().len(),
        0,
        "all sessions closed: {body}"
    );
}

mod ws_util {
    use super::*;

    pub type WsStream = tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >;

    pub async fn connect(url: String) -> (WsStream, ()) {
        let (stream, _) = tokio_tungstenite::connect_async(url).await.unwrap();
        (stream, ())
    }

    pub async fn send(s: &mut WsStream, text: String) {
        s.send(tokio_tungstenite::tungstenite::Message::Text(text.into()))
            .await
            .unwrap();
    }

    pub async fn recv(s: &mut WsStream) -> Value {
        loop {
            match s.next().await {
                Some(Ok(msg)) => {
                    if let tokio_tungstenite::tungstenite::Message::Text(t) = msg {
                        return serde_json::from_str(t.as_ref()).unwrap();
                    }
                }
                other => panic!("ws closed: {other:?}"),
            }
        }
    }
}
use ws_util::{connect as tokio_tungstenite_connect, recv as recv_ws_text, send as send_ws_text};
