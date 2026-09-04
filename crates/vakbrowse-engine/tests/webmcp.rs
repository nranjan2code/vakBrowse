//! Offline coverage for the WebMCP detection/invoke surface (`navigator.modelContext`).
//! Was previously untested; the refactor to pass args as structured CDP
//! `CallArgument`s (instead of string interpolation) is exercised here.

use std::path::PathBuf;

use url::Url;
use vakbrowse_engine::{CdpLauncher, EngineLauncher, LaunchOptions};

fn fixture_url(name: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures")
        .join(name);
    Url::from_file_path(path.canonicalize().expect("fixture exists"))
        .expect("absolute")
        .to_string()
}

#[tokio::test]
async fn webmcp_tools_listed_and_invokable() {
    let launcher = CdpLauncher::default();
    let mut session = launcher.launch(&LaunchOptions::default()).await.unwrap();
    session.navigate(&fixture_url("webmcp.html")).await.unwrap();

    let tools = session.webmcp_tools().await.expect("tools");
    assert_eq!(tools.len(), 1);
    assert_eq!(tools[0].name, "greet");

    // Arguments arrive host-side as JSON, passed in-page as a value (no
    // string interpolation into JS source).
    let out = session
        .webmcp_invoke("greet", r#"{ "who": "world" }"#)
        .await
        .expect("invoke greet");
    assert!(out.contains("hello world"), "{out}");

    // Unknown tool surfaces as Unsupported, not a panic.
    let err = session.webmcp_invoke("nope", "{}").await.unwrap_err();
    assert!(matches!(err, vakbrowse_core::VakError::Unsupported(_)));
}
