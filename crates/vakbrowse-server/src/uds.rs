//! Newline-delimited JSON over a Unix domain socket. The daemon listens;
//! CLI (or any client) connects, sends one `Request` line per call and
//! reads one `Response` line. Windows named pipes are a later port.

use futures::FutureExt;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use vakbrowse_core::Result;

use crate::{Request, Response};

/// Serve requests until the listener errors; one task per connection,
/// requests handled sequentially per connection.
pub async fn serve(
    socket_path: &std::path::Path,
    manager: std::sync::Arc<crate::SessionManager>,
) -> Result<()> {
    let _ = std::fs::remove_file(socket_path);
    if let Some(parent) = socket_path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    let listener = UnixListener::bind(socket_path).map_err(|e| {
        vakbrowse_core::VakError::Engine(format!("bind {}: {e}", socket_path.display()))
    })?;
    tracing::info!(socket = %socket_path.display(), "vakd listening");

    loop {
        let (stream, _addr) = match listener.accept().await {
            Ok(x) => x,
            Err(e) => {
                tracing::warn!("accept failed: {e}");
                continue;
            }
        };
        let manager = manager.clone();
        tokio::spawn(async move {
            if let Err(e) = handle_conn(stream, manager).await {
                tracing::debug!("connection ended: {e}");
            }
        });
    }
}

async fn handle_conn(
    stream: UnixStream,
    manager: std::sync::Arc<crate::SessionManager>,
) -> Result<()> {
    let (reader, mut writer) = stream.into_split();
    let mut lines = BufReader::new(reader).lines();

    while let Some(line) = lines.next_line().await? {
        if line.trim().is_empty() {
            continue;
        }
        let request: Request = match serde_json::from_str(&line) {
            Ok(r) => r,
            Err(e) => {
                tracing::error!("bad request: {e}");
                return Err(vakbrowse_core::VakError::Engine(format!(
                    "bad request: {e}"
                )));
            }
        };
        // A panicking handler must not take down the connection silently;
        // surface it as a normal error response.
        let response = std::panic::AssertUnwindSafe(manager.handle(request))
            .catch_unwind()
            .await
            .unwrap_or_else(|p| Err(format!("internal error: {p:?}")));
        let out = serde_json::to_string(&response)
            .map_err(|e| vakbrowse_core::VakError::Engine(format!("encode: {e}")))?;
        writer.write_all(out.as_bytes()).await?;
        writer.write_all(b"\n").await?;
    }
    Ok(())
}

/// One-shot client call: connect, send, read single response.
pub async fn call(socket_path: &std::path::Path, request: Request) -> Result<Response> {
    let stream = UnixStream::connect(socket_path).await.map_err(|e| {
        vakbrowse_core::VakError::Engine(format!(
            "cannot reach daemon at {} ({e}); start it with `vakd serve`",
            socket_path.display()
        ))
    })?;
    let (reader, mut writer) = stream.into_split();
    let mut line = String::new();
    let mut buf = BufReader::new(reader);
    let payload = serde_json::to_string(&request)
        .map_err(|e| vakbrowse_core::VakError::Engine(format!("encode: {e}")))?;
    writer.write_all(payload.as_bytes()).await?;
    writer.write_all(b"\n").await?;
    buf.read_line(&mut line).await?;
    serde_json::from_str(line.trim())
        .map_err(|e| vakbrowse_core::VakError::Engine(format!("bad response: {e}")))
}
