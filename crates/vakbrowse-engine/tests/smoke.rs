//! P0 smoke test: engine resolves, launches, navigates a local fixture,
//! and reads back DOM content — fully offline.

use std::path::PathBuf;
use url::Url;
use vakbrowse_engine::{CdpLauncher, EngineLauncher, LaunchOptions};

fn fixture_url(name: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures")
        .join(name);
    Url::from_file_path(path.canonicalize().expect("fixture exists"))
        .expect("absolute path")
        .to_string()
}

#[tokio::test]
async fn navigates_local_fixture_and_reads_dom() {
    let launcher = CdpLauncher::default();
    let mut session = launcher
        .launch(&LaunchOptions {
            headless: true,
            ..LaunchOptions::default()
        })
        .await
        .expect("launch browser");

    let nav = session
        .navigate(&fixture_url("hello.html"))
        .await
        .expect("navigate");
    assert_eq!(nav.title, "Hello vakBrowse");

    let h1 = session
        .eval_text("document.querySelector('h1').textContent")
        .await
        .expect("eval h1");
    assert_eq!(h1.trim(), "It works.");

    let snapshot = session.snapshot().await.expect("snapshot");
    assert!(snapshot.url.starts_with("file://"));
    // Perception now extracts real controls: the fixture has 1 link + 1 button.
    assert_eq!(snapshot.elements.len(), 2, "expected link + button in snapshot");
    assert!(snapshot.elements.iter().any(|e| e.role == "link"));
    assert!(snapshot.elements.iter().any(|e| e.role == "button"));

    drop(session);
}

/// Live-network check of the managed download path. Run explicitly:
/// `cargo test -p vakbrowse-engine --test smoke -- --ignored`
#[tokio::test]
#[ignore = "requires network; downloads chrome-headless-shell"]
async fn managed_engine_downloads_and_launches() {
    let launcher = CdpLauncher::default();
    let artifact = vakbrowse_engine::cft::ensure_headless_shell(&launcher.cft)
        .await
        .expect("download/pin shell");
    println!("artifact: {} @ {}", artifact.version, artifact.executable.display());

    let mut session = launcher
        .launch(&LaunchOptions::default())
        .await
        .expect("launch managed shell");
    let nav = session.navigate("https://example.com").await.expect("live nav");
    assert!(nav.title.to_lowercase().contains("example"));
}
