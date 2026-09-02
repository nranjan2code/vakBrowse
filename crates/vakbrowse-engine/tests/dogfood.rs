//! Regression tests from the real-web dogfooding session:
//! 1. snapshot must self-heal its URL when clicks navigate the page
//! 2. fill must focus, so a following press_key(Enter) submits forms/SPAs
//! 3. back/forward/reload across all surfaces

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
async fn click_navigation_self_heals_snapshot_url_and_refs() {
    let _g = common::browser_lock().acquire().await.unwrap();
    let launcher = CdpLauncher::default();
    let mut session = launcher.launch(&LaunchOptions::default()).await.unwrap();

    // hello.html links to "#next" only; use form->iframe-free nav instead:
    // iframe_child navigates nothing cross-origin, so drive hash navigation.
    session.navigate(&fixture_url("form.html")).await.unwrap();

    let before = session.snapshot().await.unwrap();
    assert!(before.url.ends_with("form.html"));

    // Click-driven URL change (like a real link) — hash works on file://.
    session.eval_text("location.hash = 'pushed'; location.hash").await.unwrap();
    let after = session.snapshot().await.unwrap();
    assert!(
        after.url.ends_with("#pushed"),
        "snapshot url must reflect live document, got {}",
        after.url
    );
    // Ref turn restarted on detected navigation.
    if !after.elements.is_empty() {
        assert_eq!(after.elements[0].r#ref.0, "@e1", "refs restart after nav");
    }
}

#[tokio::test]
async fn fill_focuses_so_enter_submits() {
    let _g = common::browser_lock().acquire().await.unwrap();
    let launcher = CdpLauncher::default();
    let mut session = launcher.launch(&LaunchOptions::default()).await.unwrap();
    session.navigate(&fixture_url("form.html")).await.unwrap();

    let snap = session.snapshot().await.unwrap();
    let name = snap
        .elements
        .iter()
        .find(|e| e.role == "textbox" && e.name.contains("Name"))
        .map(|e| e.r#ref.clone())
        .unwrap();

    session.fill(&name, "Focus").await.unwrap();
    // Human pattern: hit Enter instead of clicking Submit.
    session.press_key("Enter").await.unwrap();
    session
        .wait_for_truthy(
            "document.getElementById('out').textContent.includes('name=Focus')",
            3_000,
        )
        .await
        .expect("Enter submitted the form because fill focused the field");
}

#[tokio::test]
async fn history_back_forward_reload() {
    let _g = common::browser_lock().acquire().await.unwrap();
    let launcher = CdpLauncher::default();
    let mut session = launcher.launch(&LaunchOptions::default()).await.unwrap();

    session.navigate(&fixture_url("hello.html")).await.unwrap();
    session.navigate(&fixture_url("form.html")).await.unwrap();

    let back = session.back().await.unwrap();
    assert!(back.url.ends_with("hello.html"), "{:?}", back.url);

    let fwd = session.forward().await.unwrap();
    assert!(fwd.url.ends_with("form.html"), "{:?}", fwd.url);

    let reloaded = session.reload().await.unwrap();
    assert!(reloaded.title.contains("Form"), "{:?}", reloaded);
}
