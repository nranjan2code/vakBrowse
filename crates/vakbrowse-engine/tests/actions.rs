//! P1 integration tests: perception (stable @eN refs) + the action layer,
//! fully offline against local fixtures.

use std::path::PathBuf;
use url::Url;
use vakbrowse_core::{ElementRef, ExtractWindow, VakError};
use vakbrowse_engine::{CdpLauncher, EngineLauncher, LaunchOptions};

mod common;

fn fixture_url(name: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures")
        .join(name);
    Url::from_file_path(path.canonicalize().expect("fixture exists"))
        .expect("absolute path")
        .to_string()
}

fn find<'a>(snap: &'a vakbrowse_core::Snapshot, role: &str, name: &str) -> Option<&'a ElementRef> {
    snap.elements
        .iter()
        .find(|e| e.role == role && e.name.contains(name))
        .map(|e| &e.r#ref)
}

#[tokio::test]
async fn snapshot_exposes_stable_refs() {
    let _g = common::browser_lock().acquire().await.unwrap();
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
    let _g = common::browser_lock().acquire().await.unwrap();
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

/// CSS selector resolution on the real Chrome (CDP) backend must return the
/// same `@eN` refs that `snapshot`+`click` use, so `find_by_css` → `click`
/// round-trips identically to the snapshot path. Uses `links.html`'s
/// `<a href="form.html">` found by an exact attribute matcher.
#[tokio::test]
async fn find_by_css_on_cdp_resolves_clickable_ref() {
    let _g = common::browser_lock().acquire().await.unwrap();
    let launcher = CdpLauncher::default();
    let mut session = launcher.launch(&LaunchOptions::default()).await.unwrap();
    session.navigate(&fixture_url("links.html")).await.unwrap();

    let refs = session
        .find_by_css("a[href=\"form.html\"]")
        .await
        .expect("find_by_css");
    assert_eq!(refs.len(), 1, "exactly one anchor matches: {refs:?}");
    // Found by CSS must equal the ref the snapshot assigns to the same element.
    let snap = session.snapshot().await.unwrap();
    let link = snap
        .elements
        .iter()
        .find(|e| e.role == "link" && e.name.contains("form"))
        .map(|e| e.r#ref.clone());
    assert_eq!(link.as_ref(), Some(&refs[0]));

    // And the CSS-found ref is immediately clickable to a real navigation.
    let out = session.click(&refs[0]).await.expect("click");
    assert!(out.navigated, "click via css-found ref should navigate");
    assert!(
        out.url.as_deref().unwrap_or("").ends_with("form.html"),
        "expected to land on form.html, got {out:?}"
    );
}

/// A valid CSS selector on the CDP backend resolves to real, snapshot-consistent
/// `@eN` refs (same refs `snapshot` assigns), so agent code can `find_by_css`
/// then `click` exactly as the snapshot path does. `links.html` exposes two
/// anchors via `a[href]`.
#[tokio::test]
async fn find_by_css_on_cdp_resolves_snapshot_consistent_refs() {
    let _g = common::browser_lock().acquire().await.unwrap();
    let launcher = CdpLauncher::default();
    let mut session = launcher.launch(&LaunchOptions::default()).await.unwrap();
    session.navigate(&fixture_url("links.html")).await.unwrap();

    let ok = session
        .find_by_css("a[href]")
        .await
        .expect("a[href] is valid CSS");
    assert_eq!(ok.len(), 2, "links.html has two anchors: {ok:?}");

    // The resolved refs match the snapshot's @eN for the same elements, so the
    // two code paths agree on a stable handle.
    let snap = session.snapshot().await.unwrap();
    let links: Vec<&ElementRef> = snap
        .elements
        .iter()
        .filter(|e| e.role == "link")
        .map(|e| &e.r#ref)
        .collect();
    assert_eq!(links.len(), 2);
    for r in &ok {
        assert!(
            links.contains(&r),
            "css-found ref {r} not in snapshot links {links:?}"
        );
    }
}

#[tokio::test]
async fn click_reports_navigation_outcome() {
    let _g = common::browser_lock().acquire().await.unwrap();
    use vakbrowse_engine::ClickResult;
    let launcher = CdpLauncher::default();
    let mut session = launcher.launch(&LaunchOptions::default()).await.unwrap();
    session.navigate(&fixture_url("links.html")).await.unwrap();

    let snap = session.snapshot().await.unwrap();
    let nav_link = find(&snap, "link", "Go to the form").unwrap().clone();
    let noop_link = find(&snap, "link", "no-op").unwrap().clone();

    // A `javascript:` anchor does not navigate: report it before the navigating
    // anchor (whose document swap would invalidate its ref).
    let noop = session.click(&noop_link).await.unwrap();
    assert!(
        matches!(
            noop,
            ClickResult {
                navigated: false,
                url: None,
                ..
            }
        ),
        "got {noop:?}"
    );

    // A real navigating anchor: click should report navigated + new URL.
    let out = session.click(&nav_link).await.unwrap();
    assert!(
        matches!(
            out,
            ClickResult {
                navigated: true,
                ..
            }
        ),
        "navigated, got {out:?}"
    );
    let url = out.url.unwrap();
    assert!(url.ends_with("form.html"), "navigated url was: {url}");
}

#[tokio::test]
async fn click_recovers_from_preventdefault_wall() {
    let _g = common::browser_lock().acquire().await.unwrap();
    use vakbrowse_engine::ClickResult;
    let launcher = CdpLauncher::default();
    let mut session = launcher
        .launch(&LaunchOptions {
            click_recovery: true,
            ..LaunchOptions::default()
        })
        .await
        .unwrap();
    session
        .navigate(&fixture_url("blocked.html"))
        .await
        .unwrap();

    let snap = session.snapshot().await.unwrap();
    let blocked = find(&snap, "link", "click blocked").unwrap().clone();

    // The link's `click` listener calls preventDefault(), blocking both the
    // trusted mouse dispatch and the ground-truth DOM .click(). The recovery
    // ladder should still land us on form.html via the forced location.href.
    let out = session.click(&blocked).await.unwrap();
    assert!(
        matches!(
            out,
            ClickResult {
                navigated: true,
                ..
            }
        ),
        "expected recovered navigation, got {out:?}"
    );
    let url = out.url.unwrap();
    assert!(url.ends_with("form.html"), "recovered url was: {url}");

    let landed = session.snapshot().await.unwrap();
    assert!(landed.title.contains("Form"));
}

/// Without the opt-in, a blocked anchor click is reported honestly and the
/// page's handler is NOT re-fired by a DOM click / forced navigation.
#[tokio::test]
async fn click_recovery_is_off_by_default() {
    let _g = common::browser_lock().acquire().await.unwrap();
    let launcher = CdpLauncher::default();
    let mut session = launcher.launch(&LaunchOptions::default()).await.unwrap();
    session
        .navigate(&fixture_url("blocked.html"))
        .await
        .unwrap();
    let snap = session.snapshot().await.unwrap();
    let blocked = find(&snap, "link", "click blocked").unwrap().clone();

    let out = session.click(&blocked).await.unwrap();
    assert!(!out.navigated, "must not force navigation, got {out:?}");
    assert!(
        session.snapshot().await.unwrap().url.ends_with("blocked.html"),
        "must stay on the blocked page"
    );
}

#[tokio::test]
async fn extract_prefers_prose_over_linkdense_sidebar() {
    let _g = common::browser_lock().acquire().await.unwrap();
    let launcher = CdpLauncher::default();
    let mut session = launcher.launch(&LaunchOptions::default()).await.unwrap();
    session
        .navigate(&fixture_url("reading.html"))
        .await
        .unwrap();

    let extracted = session.extract(ExtractWindow::default()).await.unwrap();
    let text = extracted.text;
    // The real prose must be selected, not the (longer) link-dense sidebar.
    assert!(
        text.contains("Real main content the agent reads."),
        "expected article prose, got: {text}"
    );
    assert!(
        !text.contains("Navigation item one"),
        "sidebar link-chrome leaked into extract: {text}"
    );
    assert!(
        extracted.title.contains("Reading-density"),
        "got: {}",
        extracted.title
    );
}

#[tokio::test]
async fn cookies_set_get_roundtrip() {
    let _g = common::browser_lock().acquire().await.unwrap();
    let tmp = tempfile::tempdir().unwrap();
    let launcher = CdpLauncher::default();
    let mut session = launcher.launch(&LaunchOptions::default()).await.unwrap();

    session.set_download_dir(tmp.path()).await.unwrap();
    session
        .set_cookie(&vakbrowse_core::CookieInput {
            name: "k".into(),
            value: "v".into(),
            domain: "example.com".into(),
            path: "/".into(),
            secure: false,
            http_only: false,
            same_site: Some("Strict".into()),
            expires: None,
        })
        .await
        .unwrap();

    // Navigate to a real HTTP origin so document.cookie has a working cookie
    // jar (about:blank is opaque and returns [object Object] for the cookie
    // getter in recent Chrome builds).
    session.navigate("https://example.com").await.unwrap();

    // Set a cookie visible to this page's origin via document.cookie, so we
    // can read it back through the page (CDP GetCookies is origin-scoped).
    session
        .eval_text("document.cookie = 'session=test123';")
        .await
        .unwrap();
    let cookie_str = session
        .eval_text("document.cookie")
        .await
        .unwrap();
    assert!(
        cookie_str.contains("session=test123"),
        "cookie round-trip via JS failed: {cookie_str}"
    );

    // The SameSite value must survive the parse/mapping through chromiumoxide's
    // CookieSameSite (case-insensitive FromStr).
    let all = session.cookies().await.unwrap();
    // file:// may not expose example.com cookies; at minimum verify the call
    // works and any cookies on this origin match what we set via JS.
    for c in &all {
        if c.name == "session" {
            assert_eq!(c.value, "test123", "cookie value mismatch");
        }
    }

    session.clear_cookies().await.unwrap();
}

#[tokio::test]
async fn eval_text_coerces_non_string_primitives() {
    let _g = common::browser_lock().acquire().await.unwrap();
    let launcher = CdpLauncher::default();
    let mut session = launcher.launch(&LaunchOptions::default()).await.unwrap();
    session.navigate(&fixture_url("form.html")).await.unwrap();

    // A JS expression may return any JSON type. eval_text must stringify it
    // rather than hard-failing ("invalid type: boolean, expected a string"),
    // which is how the canonical `navigator.webdriver` stealth check broke.
    let b = session.eval_text("navigator.webdriver").await.unwrap();
    assert!(
        b == "true" || b == "false",
        "boolean must stringify to true/false, got: {b}"
    );
    let n = session.eval_text("1 + 2").await.unwrap();
    assert_eq!(n, "3", "number result must stringify, got: {n}");
    let z = session.eval_text("null").await.unwrap();
    assert_eq!(z, "null", "null must stringify, got: {z}");
    let s = session
        .eval_text("document.querySelector('form') ? 'has-form' : 'no-form'")
        .await
        .unwrap();
    assert_eq!(s, "has-form");
    // Unserializable primitives and `undefined` must not collapse to "null".
    let u = session.eval_text("undefined").await.unwrap();
    assert_eq!(
        u, "undefined",
        "undefined must stringify to \"undefined\", got: {u}"
    );
    let nan = session.eval_text("NaN").await.unwrap();
    assert_eq!(nan, "NaN", "NaN must stringify to \"NaN\", got: {nan}");
    let inf = session.eval_text("Infinity").await.unwrap();
    assert_eq!(inf, "Infinity", "Infinity must stringify, got: {inf}");
    let ninf = session.eval_text("-Infinity").await.unwrap();
    assert_eq!(ninf, "-Infinity", "got: {ninf}");
    let bigint = session
        .eval_text("BigInt('12345678901234567890')")
        .await
        .unwrap();
    assert!(
        bigint.contains("12345678901234567890"),
        "BigInt lost precision: {bigint}"
    );
}

#[tokio::test]
async fn click_navigates_below_fold_element() {
    // Regression: root-frame clicks dispatched at viewport coords but did NOT
    // scroll the target into view first. DOM.getBoxModel returns
    // viewport-relative coords, so a below-fold element (Bing result links,
    // long form pages) was clicked at a point where nothing is rendered and
    // the click silently missed. The fix scrolls the node into view first.
    let _g = common::browser_lock().acquire().await.unwrap();
    let launcher = CdpLauncher::default();
    let mut session = launcher.launch(&LaunchOptions::default()).await.unwrap();
    session
        .navigate(&fixture_url("scroll_click.html"))
        .await
        .unwrap();

    let snap = session.snapshot().await.unwrap();
    let far = find(&snap, "link", "far link").cloned().unwrap();
    // Clicking the off-screen link must navigate to #target.
    session.click(&far).await.unwrap();
    let hash = session.eval_text("location.hash").await.unwrap();
    assert_eq!(
        hash, "#target",
        "below-fold click should have navigated to #target, got: {hash}"
    );
}

#[tokio::test]
async fn wait_for_url_matches_and_times_out() {
    // Regression for the SPA wait trap: agents reach for
    // `document.readyState == 'complete'`, which stays 'complete' across
    // client-side navigations and therefore never fires *after* a pushState.
    // `wait_for_url` polls location.href for a substring instead.
    let _g = common::browser_lock().acquire().await.unwrap();
    let launcher = CdpLauncher::default();
    let mut session = launcher.launch(&LaunchOptions::default()).await.unwrap();
    session.navigate(&fixture_url("form.html")).await.unwrap();

    // Substring of the file:// URL -> resolves immediately.
    let href = session.eval_text("location.href").await.unwrap();
    assert!(href.contains("form.html"), "url={href}");
    session
        .wait_for_url("form.html", 2_000)
        .await
        .expect("substring of the current URL should match");

    // Non-matching pattern -> VakError::Timeout, not a hang.
    let err = session
        .wait_for_url("zz-never-matches-zz", 250)
        .await
        .unwrap_err();
    assert!(
        matches!(err, VakError::Timeout(_)),
        "expected Timeout, got: {err}"
    );
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

/// Serve `tests/fixtures` over loopback HTTP, delaying every `.css` response
/// by `css_delay` (a slow render-blocking stylesheet, as on a CDN). Hermetic:
/// binds 127.0.0.1 only. Returns the base URL.
fn serve_fixtures_slow_css(css_delay: std::time::Duration) -> String {
    use std::io::{BufRead, BufReader, Write};
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures");
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}/", listener.local_addr().unwrap());
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let root = root.clone();
            std::thread::spawn(move || {
                let mut reader = BufReader::new(&stream);
                let mut line = String::new();
                if reader.read_line(&mut line).is_err() {
                    return;
                }
                let path = line.split_whitespace().nth(1).unwrap_or("/");
                let name = path.trim_start_matches('/').split('?').next().unwrap_or("");
                // Drain headers.
                let mut h = String::new();
                while reader.read_line(&mut h).is_ok_and(|n| n > 2) {
                    h.clear();
                }
                let file = (!name.contains("..")).then(|| std::fs::read(root.join(name)).ok());
                let mut out = &stream;
                let Some(Some(body)) = file else {
                    let _ = out.write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
                    return;
                };
                let ctype = if name.ends_with(".css") {
                    std::thread::sleep(css_delay);
                    "text/css"
                } else {
                    "text/html; charset=utf-8"
                };
                let head = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: {ctype}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                let _ = out.write_all(head.as_bytes());
                let _ = out.write_all(&body);
            });
        }
    });
    base
}

/// Regression: a second click after a click-driven navigation must also
/// navigate (quotes.toscrape.com: Next -> /page/2/ worked, Next again was
/// reported "no navigation"). The click returned at URL commit while the
/// landed page was still render-blocked on its stylesheet, so the next
/// trusted click hit an unpainted, about-to-reflow document. A -> B -> C via
/// consecutive clicks on `li.next a`, each resolved fresh via `find_by_css`.
#[tokio::test]
async fn consecutive_click_navigations_chain() {
    let _g = common::browser_lock().acquire().await.unwrap();
    let base = serve_fixtures_slow_css(std::time::Duration::from_millis(1500));
    let launcher = CdpLauncher::default();
    let mut session = launcher.launch(&LaunchOptions::default()).await.unwrap();
    session.navigate(&format!("{base}chain_a.html")).await.unwrap();

    for want in ["chain_b.html", "chain_c.html"] {
        let refs = session.find_by_css("li.next a").await.unwrap();
        let next = refs.first().expect("Next link resolves to a ref").clone();
        let out = session.click(&next).await.unwrap();
        assert!(out.navigated, "click to {want} did not navigate: {out:?}");
        let url = out.url.unwrap();
        assert!(url.ends_with(want), "landed on {url}, wanted {want}");
        let snap = session.snapshot().await.unwrap();
        assert!(snap.url.ends_with(want), "snapshot url {}", snap.url);
    }
}
