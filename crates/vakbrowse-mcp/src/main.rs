//! Thin stdio wrapper around the vakbrowse-mcp library.

use rmcp::ServiceExt;
use vakbrowse_mcp::stdio_framer::normalize_stdin;
use vakbrowse_mcp::VakMcp;
use vakbrowse_server::Policy;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // stdout is the protocol channel; logs go to stderr only.
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "warn".into()),
        )
        .init();

    let policy = Policy {
        url_allow_prefixes: std::env::var("VAKBROWSE_ALLOW_PREFIXES")
            .unwrap_or_default()
            .split(',')
            .filter(|s| !s.trim().is_empty())
            .map(str::to_string)
            .collect(),
    };

    // Wrap the client's stdin in a framing normalizer so vak-mcp accepts BOTH
    // Content-Length-block and newline-delimited-JSON (NDJSON) input framing.
    // rmcp's `AsyncRwTransport` speaks NDJSON; the normalizer rewrites any
    // incoming Content-Length frames into NDJSON lines. Outgoing responses
    // stay NDJSON, which is what the official `mcp` Python + TS SDKs read.
    let (stdin, stdout) = rmcp::transport::stdio();
    let framed_stdin = normalize_stdin(stdin);

    let service = VakMcp::new(policy)
        .serve((framed_stdin, stdout))
        .await?;
    service.waiting().await?;
    Ok(())
}
