use std::net::SocketAddr;

use vakbrowse_api::serve;
use vakbrowse_server::{Backend, Policy};

fn parse_backend(s: &str) -> Backend {
    match s.to_ascii_lowercase().as_str() {
        "cdp" => Backend::Cdp,
        "dom" => Backend::Dom,
        other => {
            eprintln!("error: VAKBROWSE_BACKEND must be `cdp` or `dom`, got `{other}`");
            std::process::exit(2);
        }
    }
}

#[tokio::main]
async fn main() -> vakbrowse_core::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();

    let port: u16 = std::env::var("VAKBROWSE_HTTP_PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(7788);
    let bind_host = std::env::var("VAKBROWSE_HTTP_HOST").unwrap_or_else(|_| "127.0.0.1".into());
    let addr: SocketAddr = format!("{bind_host}:{port}")
        .parse()
        .unwrap_or_else(|_| SocketAddr::from(([127, 0, 0, 1], port)));
    let policy = Policy {
        url_allow_prefixes: std::env::var("VAKBROWSE_ALLOW_PREFIXES")
            .unwrap_or_default()
            .split(',')
            .filter(|s| !s.trim().is_empty())
            .map(str::to_string)
            .collect(),
    };
    let default_backend = std::env::var("VAKBROWSE_BACKEND")
        .map(|s| parse_backend(&s))
        .unwrap_or(Backend::Cdp);

    serve(
        addr,
        policy,
        default_backend,
    )
    .await
}
