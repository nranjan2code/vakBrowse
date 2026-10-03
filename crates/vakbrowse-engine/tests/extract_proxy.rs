//! Extraction + proxy option tests.

use std::path::PathBuf;

use url::Url;
use vakbrowse_core::ExtractWindow;
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
    session
        .navigate(&fixture_url("article.html"))
        .await
        .unwrap();

    let ex = session.extract(ExtractWindow::default()).await.unwrap();
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
async fn extract_reads_div_span_card_layouts() {
    let _g = common::browser_lock().acquire().await.unwrap();
    let launcher = CdpLauncher::default();
    let mut session = launcher.launch(&LaunchOptions::default()).await.unwrap();
    session.navigate(&fixture_url("cards.html")).await.unwrap();

    let ex = session.extract(ExtractWindow::default()).await.unwrap();
    // Card text living only in div/span (no <p>) is extracted, and the tiny
    // promo <section> does not win the main-content pick.
    assert!(
        ex.text
            .contains("“The world as we have created it is a process of our thinking.”"),
        "{}",
        ex.text
    );
    assert!(
        ex.text
            .contains("“There are only two ways to live your life.”")
    );
    // Inline runs stay on one line; block children split lines.
    assert!(ex.text.contains("by J.K. Rowling (about)"), "{}", ex.text);
    assert!(ex.text.contains("Tags: change thinking"), "{}", ex.text);
    assert!(ex.text.contains("# Card Wall"));
    // Hidden nodes, nav chrome and script source never leak.
    assert!(!ex.text.contains("hidden tracking blurb"));
    assert!(!ex.text.contains("Next page navigation link"));
    assert!(!ex.text.contains("createElement"));
}

#[tokio::test]
async fn extract_keeps_table_rows_together() {
    let _g = common::browser_lock().acquire().await.unwrap();
    let launcher = CdpLauncher::default();
    let mut session = launcher.launch(&LaunchOptions::default()).await.unwrap();
    session
        .navigate(
            "data:text/html,<title>t</title><article><p>Intro prose for the table.</p>\
<table><tr><th>Developer</th><td>The Rust Team</td></tr>\
<tr><th>License</th><td>MIT</td></tr></table></article>",
        )
        .await
        .unwrap();

    let ex = session.extract(ExtractWindow::default()).await.unwrap();
    assert!(ex.text.contains("Developer | The Rust Team"), "{}", ex.text);
    assert!(ex.text.contains("License | MIT"), "{}", ex.text);
}

#[tokio::test]
async fn extract_pages_through_long_text_with_offsets() {
    let _g = common::browser_lock().acquire().await.unwrap();
    let launcher = CdpLauncher::default();
    let mut session = launcher.launch(&LaunchOptions::default()).await.unwrap();
    session
        .navigate(&fixture_url("article.html"))
        .await
        .unwrap();

    let full = session.extract(ExtractWindow::default()).await.unwrap();
    assert!(!full.truncated);
    assert_eq!(full.next_offset, None);
    assert!(full.total_chars > 60, "{}", full.text);

    // Small windows: every page ends on a boundary and paging is lossless.
    let mut window = ExtractWindow {
        offset: 0,
        max_chars: 40,
    };
    let mut pages = Vec::new();
    loop {
        let ex = session.extract(window).await.unwrap();
        assert_eq!(ex.total_chars, full.total_chars);
        assert_eq!(ex.offset, window.offset);
        assert!(ex.text.chars().count() <= 40);
        pages.push(ex.text);
        match ex.next_offset {
            Some(n) => {
                assert!(ex.truncated);
                window.offset = n;
            }
            None => break,
        }
    }
    assert!(pages.len() > 1);
    let norm = |s: &str| s.split_whitespace().collect::<Vec<_>>().join(" ");
    assert_eq!(norm(&pages.join(" ")), norm(&full.text));
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
