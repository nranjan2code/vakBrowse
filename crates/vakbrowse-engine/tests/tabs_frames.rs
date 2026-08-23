//! P5 capability tests: multi-tab sessions and cross-frame perception
//! (elements inside iframes get refs and are clickable).

use std::path::PathBuf;

use url::Url;
use vakbrowse_core::ElementRef;
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
async fn tabs_open_switch_close_and_keep_separate_state() {
    let launcher = CdpLauncher::default();
    let mut session = launcher.launch(&LaunchOptions::default()).await.unwrap();

    session.navigate(&fixture_url("hello.html")).await.unwrap();
    let new_tab = session.new_tab(Some(&fixture_url("form.html"))).await.unwrap();

    // New tab is active with its own document.
    assert!(session.snapshot().await.unwrap().title.contains("Form"));

    let list = session.tabs().await.unwrap();
    assert_eq!(list.len(), 2);
    assert_eq!(list[0].id, new_tab.id, "active tab listed first");

    // Switch back: hello page state intact (its own title/refs).
    let first = list.iter().find(|t| t.id != new_tab.id).unwrap().id.clone();
    session.switch_tab(&first).await.unwrap();
    assert!(session.title().await.unwrap().contains("vakBrowse"));

    // Refs belong to tabs: the button ref from form.html is stale here.
    let err = session.click(&ElementRef::new("@e1")).await.unwrap_err();
    assert!(matches!(err, vakbrowse_core::VakError::NotFound(_)));

    // Close active-while-inactive tab, then try to close the last one.
    assert!(session.close_tab(&new_tab.id).await.unwrap());
    assert!(!session.close_tab(&new_tab.id).await.unwrap());
    let err = session.close_tab(&first).await.unwrap_err();
    assert!(err.to_string().contains("last remaining tab"));
}

#[tokio::test]
async fn elements_inside_iframes_get_refs_and_click_through() {
    let launcher = CdpLauncher::default();
    let mut session = launcher.launch(&LaunchOptions::default()).await.unwrap();

    session.navigate(&fixture_url("iframe.html")).await.unwrap();
    let snap = session.snapshot().await.expect("snapshot");

    // The child frame's button must appear in the merged snapshot.
    let jump = snap
        .elements
        .iter()
        .find(|e| e.role == "button" && e.name.contains("Jump"))
        .cloned()
        .expect("button inside iframe visible in snapshot");

    session.click(&jump.r#ref).await.expect("click inside iframe");

    // file:// iframes are unique-origin (no top navigation possible), so the
    // child signals success by mutating its own DOM — which surfaces in the
    // next merged AX snapshot as the status element's value.
    let snap2 = session.snapshot().await.unwrap();
    let status = snap2
        .elements
        .iter()
        .find(|e| e.role == "textbox" && e.name.contains("Status"))
        .expect("child-frame textbox visible in merged snapshot");
    assert_eq!(
        status.value.as_deref(),
        Some("clicked-ok"),
        "DOM click inside child frame executed its handler"
    );
}
