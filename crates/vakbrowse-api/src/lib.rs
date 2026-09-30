//! REST + WebSocket surface. Thin translation layer: every endpoint maps to
//! the same `Request` model the daemon and MCP server use.
//!
//! Optionally serves the React playground UI (behind the `playground` feature)
//! at `/playground` so `vakd-rest` is a single binary for both API and UI.

use std::net::SocketAddr;
use std::sync::Arc;

use axum::extract::State;
#[cfg(feature = "playground")]
use axum::extract::Path;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::http::StatusCode;
#[cfg(feature = "playground")]
use axum::http::header;
use axum::response::IntoResponse;
#[cfg(feature = "playground")]
use axum::response::Response as AxResponse;
use axum::routing::{any, get, post};
use axum::{Json, Router};
use vakbrowse_core::SessionId;
use vakbrowse_server::{Policy, Request, Response, ResponsePayload, ServiceError, SessionManager};

#[derive(Clone)]
struct ApiState {
    manager: Arc<SessionManager>,
}

/// Who may talk to this server. The API can run arbitrary JS in a real
/// browser, so it must not be reachable by web pages the operator happens to
/// visit (cross-site WebSocket hijack, DNS rebinding) or by the network at
/// large.
///
/// * `Host` must be a loopback name (or listed in `allowed_hosts`) — defeats
///   DNS rebinding. Skipped when a token is set, since a rebinding page cannot
///   know the token.
/// * A browser-supplied `Origin` must match the request's own `Host` or be
///   listed in `allowed_origins` — defeats cross-site WebSocket hijacking.
/// * With `token` set, every route except `/health` needs
///   `Authorization: Bearer <token>`.
#[derive(Clone, Debug, Default)]
pub struct SecurityConfig {
    pub token: Option<String>,
    pub allowed_hosts: Vec<String>,
    pub allowed_origins: Vec<String>,
    /// Permit binding a non-loopback address without a token.
    pub allow_insecure_bind: bool,
}

fn split_env(name: &str) -> Vec<String> {
    std::env::var(name)
        .unwrap_or_default()
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

impl SecurityConfig {
    /// `VAKBROWSE_API_TOKEN`, `VAKBROWSE_ALLOWED_HOSTS`,
    /// `VAKBROWSE_ALLOWED_ORIGINS` (comma-separated), and
    /// `VAKBROWSE_INSECURE_NO_AUTH=1` to allow a tokenless non-loopback bind.
    pub fn from_env() -> Self {
        Self {
            token: std::env::var("VAKBROWSE_API_TOKEN")
                .ok()
                .filter(|t| !t.is_empty()),
            allowed_hosts: split_env("VAKBROWSE_ALLOWED_HOSTS"),
            allowed_origins: split_env("VAKBROWSE_ALLOWED_ORIGINS"),
            allow_insecure_bind: matches!(
                std::env::var("VAKBROWSE_INSECURE_NO_AUTH").as_deref(),
                Ok("1") | Ok("true")
            ),
        }
    }

    fn host_allowed(&self, host_header: &str) -> bool {
        let name = hostname(host_header);
        matches!(name.as_str(), "localhost" | "127.0.0.1" | "[::1]")
            || self
                .allowed_hosts
                .iter()
                .any(|h| h.eq_ignore_ascii_case(&name) || h.eq_ignore_ascii_case(host_header))
    }

    fn origin_allowed(&self, origin: &str, host_header: Option<&str>) -> bool {
        if self.allowed_origins.iter().any(|o| o == origin) {
            return true;
        }
        let authority = origin.split_once("://").map(|(_, a)| a).unwrap_or("");
        host_header.is_some_and(|h| h.eq_ignore_ascii_case(authority))
    }
}

/// Host header without its port; bracketed IPv6 keeps its brackets.
fn hostname(host_header: &str) -> String {
    let h = host_header.trim().to_ascii_lowercase();
    if h.starts_with('[') {
        return match h.find(']') {
            Some(i) => h[..=i].to_string(),
            None => h,
        };
    }
    h.split(':').next().unwrap_or("").to_string()
}

fn ct_eq(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

fn screen(
    sec: &SecurityConfig,
    headers: &axum::http::HeaderMap,
    path: &str,
) -> Option<axum::response::Response> {
    use axum::http::header::{AUTHORIZATION, HOST, ORIGIN};
    let get = |name| headers.get(name).and_then(|v| v.to_str().ok());
    let host = get(HOST);

    if sec.token.is_none()
        && let Some(h) = host
        && !sec.host_allowed(h)
    {
        return Some((StatusCode::FORBIDDEN, "host not allowed").into_response());
    }
    if let Some(o) = get(ORIGIN)
        && !sec.origin_allowed(o, host)
    {
        return Some((StatusCode::FORBIDDEN, "origin not allowed").into_response());
    }
    if let Some(token) = &sec.token
        && path != "/health"
    {
        let ok = get(AUTHORIZATION)
            .and_then(|v| v.strip_prefix("Bearer "))
            .is_some_and(|t| ct_eq(t.as_bytes(), token.as_bytes()));
        if !ok {
            return Some(
                (StatusCode::UNAUTHORIZED, "missing or invalid bearer token").into_response(),
            );
        }
    }
    None
}

async fn guard(
    State(sec): State<Arc<SecurityConfig>>,
    req: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    if let Some(denied) = screen(&sec, req.headers(), req.uri().path()) {
        return denied;
    }
    next.run(req).await
}

/// Router with the default (loopback-only, no token) security config.
pub fn build_router(manager: Arc<SessionManager>) -> Router {
    build_router_with(manager, SecurityConfig::default())
}

pub fn build_router_with(manager: Arc<SessionManager>, security: SecurityConfig) -> Router {
    let state = ApiState { manager };

    #[allow(unused_mut)]
    let mut app = Router::new()
        .route("/health", get(health))
        .route("/sessions", post(open_session).get(list_sessions))
        .route("/sessions/{session}", axum::routing::delete(close_session))
        .route("/sessions/{session}/actions", post(dispatch_action))
        .route("/sessions/{session}/batch", post(dispatch_batch))
        .route("/ws", any(ws_bridge))
        // Unified playground RPC — same `Request` model the daemon/CLI/MCP use.
        .route("/playground/rpc", post(playground_rpc));

    #[cfg(feature = "playground")]
    {
        app = app
            .route("/", get(playground_root))
            .route("/playground", get(playground_root))
            .route("/playground/", get(playground_root))
            .route("/playground/{*path}", get(playground_static))
            .route("/assets/{*path}", get(playground_assets))
            .fallback(get(playground_fallback));
    }

    app.layer(axum::middleware::from_fn_with_state(
        Arc::new(security),
        guard,
    ))
    .with_state(state)
}

/// Directory where the prebuilt frontend lives. Override with
/// `VAKBROWSE_PLAYGROUND_DIR`; defaults to `../../playground/static` relative
/// to the manifest dir. Only called when the `playground` feature is active.
#[cfg(feature = "playground")]
fn playground_dir() -> std::path::PathBuf {
    std::env::var("VAKBROWSE_PLAYGROUND_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../playground/static"))
}

/// Serve forever on `addr`. CDP (Chrome) is the only engine backend.
/// Shuts down gracefully on SIGINT/SIGTERM: closes all sessions (dropping
/// browser handles) before exiting.
pub async fn serve(
    addr: SocketAddr,
    policy: Policy,
    security: SecurityConfig,
) -> vakbrowse_core::Result<()> {
    if !addr.ip().is_loopback() && security.token.is_none() && !security.allow_insecure_bind {
        return Err(vakbrowse_core::VakError::Policy(format!(
            "refusing to bind {addr}: this API can run arbitrary JS in a browser. \
             Set VAKBROWSE_API_TOKEN (clients send `Authorization: Bearer <token>`), \
             or VAKBROWSE_INSECURE_NO_AUTH=1 if the network is already trusted."
        )));
    }
    // Unlike the daemon (explicit --idle-timeout-secs), an HTTP service left
    // running would otherwise accumulate forgotten Chrome sessions forever.
    let manager = Arc::new(
        SessionManager::with_policy(policy).with_pool(vakbrowse_server::PoolConfig::from_env(
            Some(1800),
        )),
    );
    manager.spawn_reaper();
    let app = build_router_with(manager.clone(), security);
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .map_err(|e| vakbrowse_core::VakError::Engine(format!("bind {addr}: {e}")))?;
    tracing::info!("vakd-rest listening on http://{addr}");
    axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            vakbrowse_server::shutdown_signal().await;
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
            // Browser/engine-level failures (browser crashed, launch failed)
            // are server-side and may warrant a client retry.
            ServiceError::Engine(_) => StatusCode::BAD_GATEWAY,
            // Protocol violations (CDP disconnect, malformed response) are
            // transient and may also warrant a retry.
            ServiceError::Protocol(_) => StatusCode::BAD_GATEWAY,
            // Caller asked for something the backend can't do (no silent
            // fallback — don't retry, fix the request).
            ServiceError::Unsupported(_) => StatusCode::BAD_REQUEST,
            // IO errors during perception/extraction are server-side.
            ServiceError::Io(_) => StatusCode::INTERNAL_SERVER_ERROR,
            // Perception errors (snapshot parse failure, AX tree collapse)
            // are server-side rendering issues.
            ServiceError::Perception(_) => StatusCode::INTERNAL_SERVER_ERROR,
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

/// Unified playground RPC endpoint. Accepts the same `Request` model as the
/// daemon's UDS wire protocol — `{"type":"open","options":{...}}`,
/// `{"type":"act","session":"s1","action":{...}}`, etc. — and returns the
/// `Response` JSON. This lets the playground UI talk to one endpoint instead
/// of mapping each route individually.
async fn playground_rpc(
    State(state): State<ApiState>,
    Json(request): Json<Request>,
) -> impl IntoResponse {
    let status = if matches!(request, Request::Open { .. }) {
        StatusCode::CREATED
    } else {
        StatusCode::OK
    };
    let response = state.manager.handle(request).await;
    let status = match &response {
        Ok(ResponsePayload::Error(_)) => status_for(&response),
        _ => status,
    };
    (status, Json(response))
}

/// Serve `index.html` for the `/playground` and `/playground/` routes.
#[cfg(feature = "playground")]
async fn playground_root(_state: State<ApiState>) -> AxResponse {
    serve_static_file("index.html")
}

/// Serve a file from the playground build output directory on disk.
/// Falls back to `index.html` for SPA routing if the file is not found.
#[cfg(feature = "playground")]
async fn playground_static(
    _state: State<ApiState>,
    Path(relative): Path<String>,
) -> AxResponse {
    let clean = relative.trim_start_matches("playground/");
    serve_static_file(clean)
}

/// Serve static assets requested from root, e.g. `/assets/...`
#[cfg(feature = "playground")]
async fn playground_assets(
    _state: State<ApiState>,
    Path(relative): Path<String>,
) -> AxResponse {
    let clean = format!("assets/{}", relative);
    serve_static_file(&clean)
}

/// Fallback route: serve static file if it exists, otherwise SPA index.html
#[cfg(feature = "playground")]
async fn playground_fallback(
    _state: State<ApiState>,
    uri: axum::http::Uri,
) -> AxResponse {
    let path = uri.path().trim_start_matches('/');
    serve_static_file(path)
}

#[cfg(feature = "playground")]
fn serve_static_file(relative: &str) -> AxResponse {
    let base = playground_dir();
    let target = base.join(relative);

    match std::fs::read(&target) {
        Ok(bytes) => {
            let mime = mime_guess::from_path(&target).first_or_octet_stream();
            (
                StatusCode::OK,
                [(header::CONTENT_TYPE, mime.to_string())],
                axum::body::Body::from(bytes),
            )
                .into_response()
        }
        // SPA fallback — serve index.html so client-side routing works
        Err(_) => {
            let index_path = base.join("index.html");
            match std::fs::read(&index_path) {
                Ok(bytes) => (
                    StatusCode::OK,
                    [(header::CONTENT_TYPE, "text/html; charset=utf-8")],
                    axum::body::Body::from(bytes),
                )
                    .into_response(),
                Err(_) => (
                    StatusCode::SERVICE_UNAVAILABLE,
                    "Playground UI not built. Build it with:\n  cd playground/frontend && npm install && npm run build",
                )
                    .into_response(),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vakbrowse_server::ServiceError;

    fn err(e: ServiceError) -> Response {
        Ok(ResponsePayload::Error(e))
    }

    #[test]
    fn status_codes_distinguish_retryable_from_not() {
        // Stale ref -> 404 (don't retry, fix the ref).
        assert_eq!(
            status_for(&err(ServiceError::NotFound("stale".into()))),
            StatusCode::NOT_FOUND
        );
        // Policy (URL allowlist) -> 403 (don't retry, fix the URL).
        assert_eq!(
            status_for(&err(ServiceError::Policy("blocked".into()))),
            StatusCode::FORBIDDEN
        );
        // Timeout, browser crash, protocol error -> 502 (retryable).
        assert_eq!(
            status_for(&err(ServiceError::Timeout("t".into()))),
            StatusCode::GATEWAY_TIMEOUT
        );
        assert_eq!(
            status_for(&err(ServiceError::Engine("crashed".into()))),
            StatusCode::BAD_GATEWAY
        );
        assert_eq!(
            status_for(&err(ServiceError::Protocol("disconnected".into()))),
            StatusCode::BAD_GATEWAY
        );
        // Unsupported action -> 400 (don't retry, use a different tool).
        assert_eq!(
            status_for(&err(ServiceError::Unsupported("no-js".into()))),
            StatusCode::BAD_REQUEST
        );
        // Server-side rendering/IO issues -> 500.
        assert_eq!(
            status_for(&err(ServiceError::Io("disk".into()))),
            StatusCode::INTERNAL_SERVER_ERROR
        );
        assert_eq!(
            status_for(&err(ServiceError::Perception("parse".into()))),
            StatusCode::INTERNAL_SERVER_ERROR
        );
    }
}
