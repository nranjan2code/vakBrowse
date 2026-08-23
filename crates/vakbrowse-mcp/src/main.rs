//! Thin stdio wrapper around the vakbrowse-mcp library.

use rmcp::ServiceExt;
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

    let service = VakMcp::new(policy)
        .serve(rmcp::transport::stdio())
        .await?;
    service.waiting().await?;
    Ok(())
}
