//! REST + WebSocket surface. Thin translation layer: every endpoint maps to
//! the same `Request` model the daemon and MCP server use.

use std::net::SocketAddr;
use std::sync::Arc;

use axum::extract::State;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{any, get, post};
use axum::{Json, Router};
use vakbrowse_core::SessionId;
use vakbrowse_server::{
    Backend, Policy, Request, Response, ResponsePayload, ServiceError, SessionManager,
};

#[derive(Clone)]
struct ApiState {
    manager: Arc<SessionManager>,
}

pub fn build_router(manager: Arc<SessionManager>) -> Router {
    let state = ApiState { manager };
    Router::new()
        .route("/health", get(health))
        .route("/sessions", post(open_session).get(list_sessions))
        .route("/sessions/{session}", axum::routing::delete(close_session))
        .route("/sessions/{session}/actions", post(dispatch_action))
        .route("/sessions/{session}/batch", post(dispatch_batch))
        .route("/ws", any(ws_bridge))
        .with_state(state)
}

/// Serve forever on `addr`. `default_backend` is the server-wide engine chosen
/// when a client omits `SessionOptions.backend` (`VAKBROWSE_BACKEND`).
/// Shuts down gracefully on SIGINT/SIGTERM: closes all sessions (dropping
/// browser handles) before exiting.
pub async fn serve(
    addr: SocketAddr,
    policy: Policy,
    default_backend: Backend,
) -> vakbrowse_core::Result<()> {
    let manager = Arc::new(SessionManager::with_policy_and_backend(
        policy,
        default_backend,
    ));
    let app = build_router(manager.clone());
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .map_err(|e| vakbrowse_core::VakError::Engine(format!("bind {addr}: {e}")))?;
    tracing::info!("vakd-rest listening on http://{addr}");
    axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            let _ = tokio::signal::ctrl_c().await;
            tracing::info!(
                "shutdown signal received; closing {} session(s)",
                manager.stats().await.0
            );
            manager.close_all().await;
            tracing::info!("vakd-rest stopped");
        })
        .await
        .map_err(|e| vakbrowse_core::VakError::Engine(format!("serve: {e}")))?;
    Ok(())
}

async fn health() -> &'static str {
    "ok"
}

fn status_for(response: &Response) -> StatusCode {
    match response {
        Ok(ResponsePayload::Opened(_)) => StatusCode::CREATED,
        Ok(ResponsePayload::Error(e)) => match e {
            ServiceError::NotFound(_) => StatusCode::NOT_FOUND,
            ServiceError::Policy(_) => StatusCode::FORBIDDEN,
            ServiceError::Timeout(_) => StatusCode::GATEWAY_TIMEOUT,
            ServiceError::Http(_) => StatusCode::BAD_GATEWAY,
            // Engine/Protocol/Perception/Unsupported/Io are caller/action
            // errors surfaced as 400 (no status-code granularity to give).
            _ => StatusCode::BAD_REQUEST,
        },
        Ok(_) => StatusCode::OK,
        // Only reachable if the handler panics and is caught as a transport
        // error by the UDS layer.
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

fn response_to_http(response: Response) -> impl IntoResponse {
    let status = status_for(&response);
    (status, Json(response))
}

async fn open_session(
    State(state): State<ApiState>,
    Json(options): Json<vakbrowse_server::SessionOptions>,
) -> impl IntoResponse {
    let response = state.manager.handle(Request::Open { options }).await;
    response_to_http(response)
}

async fn list_sessions(State(state): State<ApiState>) -> impl IntoResponse {
    let response = state.manager.handle(Request::ListSessions).await;
    response_to_http(response)
}

async fn close_session(
    State(state): State<ApiState>,
    axum::extract::Path(session): axum::extract::Path<String>,
) -> impl IntoResponse {
    let response = state
        .manager
        .handle(Request::Close {
            session: SessionId(session),
        })
        .await;
    response_to_http(response)
}

async fn dispatch_action(
    State(state): State<ApiState>,
    axum::extract::Path(session): axum::extract::Path<String>,
    Json(action): Json<vakbrowse_server::Action>,
) -> impl IntoResponse {
    let response = state
        .manager
        .handle(Request::Act {
            session: SessionId(session),
            action,
        })
        .await;
    response_to_http(response)
}

/// Run a sequence of actions in one request (fail-fast on the first error).
/// Bodies as `POST /sessions/{session}/batch` with a JSON array of action
/// objects, e.g. `[{"type":"navigate","url":"…"},{"type":"extract"}]`.
async fn dispatch_batch(
    State(state): State<ApiState>,
    axum::extract::Path(session): axum::extract::Path<String>,
    Json(actions): Json<Vec<vakbrowse_server::Action>>,
) -> impl IntoResponse {
    let response = state
        .manager
        .handle(Request::Batch {
            session: SessionId(session),
            actions,
        })
        .await;
    response_to_http(response)
}

/// WebSocket bridge: client sends `Request` JSON frames, receives one
/// `Response` JSON frame per request. Long-lived agents avoid per-call HTTP
/// overhead; ordering guarantees snapshot-then-act sequences.
async fn ws_bridge(State(state): State<ApiState>, upgrade: WebSocketUpgrade) -> impl IntoResponse {
    upgrade.on_upgrade(move |socket| handle_ws(socket, state))
}

async fn handle_ws(mut socket: WebSocket, state: ApiState) {
    while let Some(Ok(msg)) = socket.recv().await {
        let Message::Text(text) = msg else {
            continue;
        };
        let response = match serde_json::from_str::<Request>(&text) {
            Ok(request) => state.manager.handle(request).await,
            Err(e) => Err(format!("bad request: {e}")),
        };
        let out = serde_json::to_string(&response)
            .unwrap_or_else(|e| format!("{{\"Err\":\"encode failure: {e}\"}}"));
        if socket.send(Message::Text(out.into())).await.is_err() {
            break;
        }
    }
}
