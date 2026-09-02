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
    let manager =
        SessionManager::default().with_profiles_root(dir.path().to_path_buf());
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
        client.get(format!("{base}/health")).send().await.unwrap().text().await.unwrap(),
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
    // DNS/engine failure surfaces as an Error payload mapped to 400.
    let resp = client
        .post(format!("{base}/sessions/{session}/actions"))
        .json(&json!({ "type": "navigate", "url": "https://blocked.invalid/" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 400);
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

    send_ws_text(&mut ws, json!({ "type": "close", "session": session }).to_string()).await;
    let reply = recv_ws_text(&mut ws).await;
    assert_eq!(
        reply["Ok"]["Closed"],
        true,
        "close reply was {}",
        serde_json::to_string(&reply).unwrap()
    );
}

// Minimal WS helpers without pulling tungstenite types into signatures.
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
