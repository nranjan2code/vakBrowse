//! P1 integration tests: perception (stable @eN refs) + the action layer,
//! fully offline against local fixtures.

use std::path::PathBuf;
use url::Url;
use vakbrowse_core::{ElementRef, VakError};
use vakbrowse_engine::{CdpLauncher, EngineLauncher, LaunchOptions};

fn fixture_url(name: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures")
        .join(name);
    Url::from_file_path(path.canonicalize().expect("fixture exists"))
        .expect("absolute path")
        .to_string()
}

fn find<'a>(
    snap: &'a vakbrowse_core::Snapshot,
    role: &str,
    name: &str,
) -> Option<&'a ElementRef> {
    snap.elements
        .iter()
        .find(|e| e.role == role && e.name.contains(name))
        .map(|e| &e.r#ref)
}

#[tokio::test]
async fn snapshot_exposes_stable_refs() {
    let launcher = CdpLauncher::default();
    let mut session = launcher.launch(&LaunchOptions::default()).await.unwrap();

    session.navigate(&fixture_url("form.html")).await.unwrap();

    let s1 = session.snapshot().await.unwrap();
    assert!(s1.title.contains("Form"));
    let name1 = find(&s1, "textbox", "Name").cloned().unwrap();
    let button = find(&s1, "button", "Send").cloned().unwrap();
    let combo = find(&s1, "combobox", "Pet").cloned().unwrap();

    // Second snapshot: same DOM -> same refs.
    let s2 = session.snapshot().await.unwrap();
    let name2 = find(&s2, "textbox", "Name").cloned().unwrap();
    assert_eq!(name1, name2);

    // Stale ref after navigation must be rejected, not misfired.
    session.navigate(&fixture_url("hello.html")).await.unwrap();
    let err = session.click(&button).await.unwrap_err();
    assert!(matches!(err, VakError::NotFound(_)));
    drop(combo);
}

#[tokio::test]
async fn fill_select_click_wait_roundtrip() {
    let launcher = CdpLauncher::default();
    let mut session = launcher.launch(&LaunchOptions::default()).await.unwrap();
    session.navigate(&fixture_url("form.html")).await.unwrap();

    let snap = session.snapshot().await.unwrap();
    let name = find(&snap, "textbox", "Name").unwrap().clone();
    let email = find(&snap, "textbox", "Email").unwrap().clone();
    let pet = find(&snap, "combobox", "Pet").unwrap().clone();
    let msg = find(&snap, "textbox", "Message").unwrap().clone();
    let send = find(&snap, "button", "Send").unwrap().clone();

    session.fill(&name, "Ada").await.unwrap();
    session.fill(&email, "ada@example.com").await.unwrap();
    assert!(session.select_option(&pet, "dog").await.unwrap());
    session.fill(&msg, "hello").await.unwrap();
    session.click(&send).await.unwrap();

    session
        .wait_for_truthy(
            "document.getElementById('out').textContent.includes('dog')",
            3_000,
        )
        .await
        .unwrap();

    let out = session
        .eval_text("document.getElementById('out').textContent")
        .await
        .unwrap();
    assert!(out.contains("name=Ada"), "got: {out}");
    assert!(out.contains("email=ada@example.com"), "got: {out}");
    assert!(out.contains("pet=dog"), "got: {out}");

    // Timeout surfaces as VakError::Timeout.
    let err = session.wait_for_truthy("false", 250).await.unwrap_err();
    assert!(matches!(err, VakError::Timeout(_)));
}

#[tokio::test]
async fn cookies_and_downloads_configurable() {
    let tmp = tempfile::tempdir().unwrap();
    let launcher = CdpLauncher::default();
    let mut session = launcher.launch(&LaunchOptions::default()).await.unwrap();

    session.set_download_dir(tmp.path()).await.unwrap();
    session.set_cookie(&vakbrowse_core::CookieInput {
        name: "k".into(),
        value: "v".into(),
        domain: "example.com".into(),
        path: "/".into(),
        secure: false,
        http_only: false,
    }).await.unwrap();

    // file:// origin doesn't expose example.com cookies; read them via CDP-level list.
    let all = session.cookies().await.unwrap();
    // GetCookies without urls returns page cookies; on file:// it may be empty.
    let _ = all; // smoke: call must not error

    session.clear_cookies().await.unwrap();
}

/// Live-network check. Run explicitly:
/// `cargo test -p vakbrowse-engine --test actions -- --ignored`
#[tokio::test]
#[ignore = "requires network"]
async fn live_navigation_and_snapshot() {
    let launcher = CdpLauncher::default();
    let mut session = launcher.launch(&LaunchOptions::default()).await.unwrap();
    let nav = session.navigate("https://example.com").await.unwrap();
    assert!(nav.title.to_lowercase().contains("example"));
    let snap = session.snapshot().await.unwrap();
    assert!(
        snap.elements.iter().any(|e| e.role == "link"),
        "expected at least one link in a11y snapshot"
    );
}
