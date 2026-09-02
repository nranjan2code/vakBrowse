//! Extraction + proxy option tests.

use std::path::PathBuf;

use url::Url;
use vakbrowse_engine::{CdpLauncher, EngineLauncher, LaunchOptions};

mod common;

fn fixture_url(name: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures")
        .join(name);
    Url::from_file_path(path.canonicalize().expect("fixture exists"))
        .expect("absolute")
        .to_string()
}

#[tokio::test]
async fn extract_returns_clean_main_content() {
    let _g = common::browser_lock().acquire().await.unwrap();
    let launcher = CdpLauncher::default();
    let mut session = launcher.launch(&LaunchOptions::default()).await.unwrap();
    session.navigate(&fixture_url("article.html")).await.unwrap();

    let ex = session.extract().await.unwrap();
    assert_eq!(ex.title.trim(), "vakBrowse Article Fixture");
    // Headings preserved, prose present.
    assert!(ex.text.contains("# The Article Title"), "{}", ex.text);
    assert!(ex.text.contains("## Section One"));
    assert!(ex.text.contains("First paragraph"));
    assert!(ex.text.contains("List item beta"));
    // Chrome stripped: nav/footer/script noise must not dominate.
    assert!(!ex.text.contains("copyright footer boilerplate"));
    assert!(!ex.text.contains("console.log"));
    assert!(!ex.truncated);
}

#[tokio::test]
async fn proxy_flag_launches_and_reaches_a_site() {
    let _g = common::browser_lock().acquire().await.unwrap();
    // We can't assume an external proxy; verify the launch path accepts the
    // option and browsing still works (proxy arg malformed only breaks
    // networking, not launch).
    let launcher = CdpLauncher::default();
    let mut session = launcher
        .launch(&LaunchOptions {
            proxy_server: Some("http://127.0.0.1:1".into()), // dead proxy on purpose
            ..LaunchOptions::default()
        })
        .await
        .unwrap();
    // Local file navigation bypasses the proxy entirely.
    let nav = session.navigate(&fixture_url("hello.html")).await.unwrap();
    assert_eq!(nav.title, "Hello vakBrowse");
}
