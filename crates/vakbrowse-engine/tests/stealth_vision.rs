//! P4 integration tests: stealth fingerprinting, vision fallback,
//! WebMCP feature-detection. Offline via local fixtures.

use std::path::PathBuf;

use url::Url;
use vakbrowse_engine::{CdpLauncher, EngineLauncher, LaunchOptions};
use vakbrowse_stealth::StealthProfile;

fn fixture_url(name: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures")
        .join(name);
    Url::from_file_path(path.canonicalize().expect("fixture exists"))
        .expect("absolute")
        .to_string()
}

#[tokio::test]
async fn stealth_patches_navigator_and_sets_identity() {
    let profile = StealthProfile::generate("test-agent");
    let launcher = CdpLauncher::default();
    let mut session = launcher
        .launch(&LaunchOptions {
            stealth: Some(profile.clone()),
            ..LaunchOptions::default()
        })
        .await
        .expect("launch with stealth");

    session.navigate(&fixture_url("hello.html")).await.unwrap();

    let webdriver = session.eval_text("String(navigator.webdriver)").await.unwrap();
    assert_eq!(webdriver, "false", "navigator.webdriver must be patched");

    let platform = session.eval_text("navigator.platform").await.unwrap();
    assert_eq!(platform, profile.platform);

    let lang = session.eval_text("navigator.language").await.unwrap();
    assert_eq!(lang, profile.languages[0]);

    let chrome = session.eval_text("typeof window.chrome").await.unwrap();
    assert_eq!(chrome, "object");
}

#[tokio::test]
async fn without_stealth_webdriver_is_exposed() {
    let launcher = CdpLauncher::default();
    let mut session = launcher.launch(&LaunchOptions::default()).await.unwrap();
    session.navigate(&fixture_url("hello.html")).await.unwrap();
    // Sanity contrast: default launches don't pretend to be human.
    let webdriver = session.eval_text("String(navigator.webdriver)").await.unwrap();
    assert_eq!(webdriver, "true");
}

#[tokio::test]
async fn screenshot_returns_valid_png() {
    let launcher = CdpLauncher::default();
    let mut session = launcher.launch(&LaunchOptions::default()).await.unwrap();
    session.navigate(&fixture_url("hello.html")).await.unwrap();

    for full in [false, true] {
        let png = session.screenshot(full).await.expect("screenshot");
        assert!(png.len() > 100, "png too small: {}", png.len());
        assert_eq!(&png[..4], b"\x89PNG");
    }
}

#[tokio::test]
async fn click_at_coordinates_submits_form() {
    let launcher = CdpLauncher::default();
    let mut session = launcher.launch(&LaunchOptions::default()).await.unwrap();
    session.navigate(&fixture_url("form.html")).await.unwrap();

    // Vision-style flow: locate target purely by coordinates from JS.
    let coords = session
        .eval_text(
            "(() => { const r = document.getElementById('go').getBoundingClientRect();
               return JSON.stringify({x: r.x + r.width/2, y: r.y + r.height/2}); })()",
        )
        .await
        .unwrap();
    let v: serde_json::Value = serde_json::from_str(&coords).unwrap();
    let x = v["x"].as_f64().unwrap();
    let y = v["y"].as_f64().unwrap();

    session.click_at(x, y).await.expect("click_at");
    session
        .wait_for_truthy(
            "document.getElementById('out').textContent.length > 0",
            3_000,
        )
        .await
        .expect("form submitted via coordinate click");
}
