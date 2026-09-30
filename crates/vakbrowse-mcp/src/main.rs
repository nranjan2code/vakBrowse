//! Thin stdio wrapper around the vakbrowse-mcp library.

use rmcp::ServiceExt;
use vakbrowse_mcp::VakMcp;
use vakbrowse_mcp::stdio_framer::normalize_stdin;
use vakbrowse_server::Policy;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // stdout is the protocol channel; logs go to stderr only.
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "warn".into()),
        )
        .init();

    let policy = Policy::from_env();

    // Wrap the client's stdin in a framing normalizer so vak-mcp accepts BOTH
    // Content-Length-block and newline-delimited-JSON (NDJSON) input framing.
    // rmcp's `AsyncRwTransport` speaks NDJSON; the normalizer rewrites any
    // incoming Content-Length frames into NDJSON lines. Outgoing responses
    // stay NDJSON, which is what the official `mcp` Python + TS SDKs read.
    let (stdin, stdout) = rmcp::transport::stdio();
    let framed_stdin = normalize_stdin(stdin);

    let server = VakMcp::new(policy);
    let manager = server.manager();
    manager.spawn_reaper();
    let service = server.serve((framed_stdin, stdout)).await?;
    // Stop on client disconnect OR on SIGINT/SIGTERM; either way close every
    // session so Chrome is not left running.
    tokio::select! {
        result = service.waiting() => { result?; }
        _ = vakbrowse_server::shutdown_signal() => {}
    }
    manager.close_all().await;
    Ok(())
}
