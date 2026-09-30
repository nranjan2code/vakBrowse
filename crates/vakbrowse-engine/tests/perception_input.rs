//! Snapshot richness (state, headings) and input fidelity (key chords,
//! select-by-label, page-opened tabs) against a hermetic fixture.

use std::path::PathBuf;

use url::Url;
use vakbrowse_core::Snapshot;
use vakbrowse_engine::{CdpLauncher, EngineLauncher, LaunchOptions, PageOps};

mod common;

fn fixture_url(name: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures")
        .join(name);
    Url::from_file_path(path.canonicalize().expect("fixture exists"))
        .expect("absolute")
        .to_string()
}

async fn open() -> Box<dyn PageOps> {
    let mut s = CdpLauncher::default()
        .launch(&LaunchOptions::default())
        .await
        .unwrap();
    s.navigate(&fixture_url("controls.html")).await.unwrap();
    s
}

fn find<'a>(snap: &'a Snapshot, role: &str, name: &str) -> &'a vakbrowse_core::SnapshotNode {
    snap.elements
        .iter()
        .find(|e| e.role == role && e.name.contains(name))
        .unwrap_or_else(|| panic!("no {role} {name:?} in {snap:#?}"))
}

#[tokio::test]
async fn snapshot_reports_state_and_headings() {
    let _g = common::browser_lock().acquire().await.unwrap();
    let mut s = open().await;
    let snap = s.snapshot().await.unwrap();

    assert!(find(&snap, "checkbox", "Newsletter").state.contains(&"checked".into()));
    assert!(find(&snap, "checkbox", "Terms").state.contains(&"unchecked".into()));
    assert!(find(&snap, "checkbox", "Terms").clickable);
    assert!(find(&snap, "button", "Buy").state.contains(&"disabled".into()));

    // The two identical "Read more" links are told apart by their headings.
    let texts: Vec<&str> = snap.headings.iter().map(|h| h.text.as_str()).collect();
    assert_eq!(texts, ["Shop", "Shoes", "Hats"], "{snap:#?}");
    let reads: Vec<usize> = snap
        .elements
        .iter()
        .enumerate()
        .filter(|(_, e)| e.name == "Read more")
        .map(|(i, _)| i)
        .collect();
    let (shoes, hats) = (&snap.headings[1], &snap.headings[2]);
    assert!(shoes.before <= reads[0] && reads[0] < hats.before.max(reads[0] + 1));
    assert!(hats.before <= reads[1]);
}

#[tokio::test]
async fn select_by_value_or_label() {
    let _g = common::browser_lock().acquire().await.unwrap();
    let mut s = open().await;
    let snap = s.snapshot().await.unwrap();
    let sel = find(&snap, "combobox", "Color").r#ref.clone();

    assert!(s.select_option(&sel, "g").await.unwrap(), "by value");
    assert_eq!(s.eval_text("color.value").await.unwrap(), "g");
    assert!(s.select_option(&sel, " red ").await.unwrap(), "by label, case-insensitive");
    assert_eq!(s.eval_text("color.value").await.unwrap(), "r");
    assert!(!s.select_option(&sel, "Purple").await.unwrap(), "unknown option");
}

#[tokio::test]
async fn ctrl_a_then_typing_replaces_text() {
    let _g = common::browser_lock().acquire().await.unwrap();
    let mut s = open().await;
    let snap = s.snapshot().await.unwrap();
    let q = find(&snap, "textbox", "Query").r#ref.clone();

    s.click(&q).await.unwrap();
    s.press_key("Control+a").await.unwrap();
    s.press_key("x").await.unwrap();
    assert_eq!(s.eval_text("q.value").await.unwrap(), "x", "select-all then type");
    assert!(s.press_key("Hyper+a").await.is_err());
}

#[tokio::test]
async fn page_opened_tabs_are_adopted_and_reported() {
    let _g = common::browser_lock().acquire().await.unwrap();
    let mut s = open().await;
    let snap = s.snapshot().await.unwrap();
    let link = find(&snap, "link", "Open elsewhere").r#ref.clone();

    let out = s.click(&link).await.unwrap();
    let opened = out.opened_tab.expect("click should report the new tab");

    let tabs = s.tabs().await.unwrap();
    assert_eq!(tabs.len(), 2, "{tabs:?}");
    assert!(tabs.iter().any(|t| t.id.0 == opened));
    // Original tab stays active; the new one can be switched to and used.
    assert_ne!(tabs[0].id.0, opened);
    s.switch_tab(&vakbrowse_core::TabId(opened)).await.unwrap();
    let title = s.snapshot().await.unwrap().title;
    assert!(!title.is_empty(), "new tab should have loaded hello.html");
}

#[tokio::test]
async fn cookies_are_browser_wide_and_keep_expiry() {
    let _g = common::browser_lock().acquire().await.unwrap();
    let mut s = open().await; // a file:// page: no cookie scope of its own
    // Chrome caps cookie lifetime at ~400 days, so stay well inside that.
    let exp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs_f64()
        + 30.0 * 86_400.0;
    let cookie = |name: &str, expires| vakbrowse_core::CookieInput {
        name: name.into(),
        value: "v".into(),
        domain: "example.com".into(),
        path: "/".into(),
        secure: false,
        http_only: false,
        same_site: None,
        expires,
    };
    s.set_cookie(&cookie("persist", Some(exp))).await.unwrap();
    s.set_cookie(&cookie("ephemeral", None)).await.unwrap();

    let all = s.cookies().await.unwrap();
    let persist = all.iter().find(|c| c.name == "persist").expect("cross-origin cookie listed");
    assert!((persist.expires.unwrap() - exp).abs() < 5.0, "{:?}", persist.expires);
    assert!(!persist.session);
    let eph = all.iter().find(|c| c.name == "ephemeral").unwrap();
    assert_eq!(eph.expires, None);
    assert!(eph.session);
}
