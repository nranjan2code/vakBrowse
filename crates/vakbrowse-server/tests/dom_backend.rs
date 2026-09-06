//! Proves the engine seam: a non-Chromium launcher (`DomLauncher`) drives the
//! exact same `SessionManager` request model as the CDP backend. Covers
//! open -> snapshot (refs + accessible names), fill -> re-snapshot (mutation
//! persists in the DOM), `Request::Batch` (one result per action), and
//! `RotateProxy` (re-launch on the next endpoint, **URL restored** so the
//! page carries on). No chrome is spawned.

use std::path::PathBuf;
use std::sync::Arc;

use url::Url;
use vakbrowse_core::SessionId;
use vakbrowse_dom::DomLauncher;
#[cfg(feature = "dom-backend")]
use vakbrowse_server::Backend;
use vakbrowse_server::{
    Action, ActionResult, Policy, Request, ResponsePayload, ServiceError, SessionManager,
    SessionOptions,
};
use vakbrowse_core::VakError;

fn fixture_url(name: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures")
        .join(name);
    Url::from_file_path(path.canonicalize().expect("fixture exists"))
        .expect("absolute")
        .to_string()
}

/// Inject the experimental DOM launcher as the engine backend.
fn dom_manager() -> SessionManager {
    SessionManager::new(Policy::default(), Arc::new(DomLauncher))
}

#[tokio::test]
async fn dom_backend_open_snapshot_fill_batch_rotate_restore() {
    let manager = dom_manager();

    // open with a proxy *pool* so RotateProxy has 2+ endpoints to spin.
    let opened = manager
        .handle(Request::Open {
            options: SessionOptions {
                url: Some(fixture_url("form.html")),
                proxies: vec!["http://127.0.0.1:9".into(), "http://127.0.0.1:10".into()],
                ..SessionOptions::default()
            },
        })
        .await
        .expect("open");
    let session = match opened {
        ResponsePayload::Opened(info) => info.id,
        other => panic!("unexpected response {other:?}"),
    };

    // Snapshot: the Name textbox ref (@e1, wrapped in <label>) + Send button.
    let snap = manager
        .act(&session, Action::Snapshot)
        .await
        .expect("snapshot");
    let elements = match snap {
        ActionResult::Snapshot { snapshot } => snapshot.elements,
        other => panic!("expected Snapshot, got {other:?}"),
    };
    let name_ref = elements
        .iter()
        .find(|e| e.role == "textbox" && e.name.contains("Name"))
        .map(|e| e.r#ref.0.clone())
        .expect("Name textbox in snapshot");
    assert!(
        elements
            .iter()
            .any(|e| e.role == "button" && e.name.contains("Send")),
        "Send button missing: {elements:?}"
    );

    // Fill the Name field, then re-snapshot: the value must persist in the
    // DOM snapshot (proves fill mutates the document, not just the wire).
    manager
        .act(
            &session,
            Action::Fill {
                r#ref: name_ref.clone(),
                text: "Linus".into(),
            },
        )
        .await
        .expect("fill");
    let after = manager
        .act(&session, Action::Snapshot)
        .await
        .expect("snapshot 2");
    let after = match after {
        ActionResult::Snapshot { snapshot } => snapshot.elements,
        other => panic!("expected Snapshot, got {other:?}"),
    };
    let filled = after
        .iter()
        .find(|e| e.r#ref.0 == name_ref)
        .expect("Name textbox still present");
    assert_eq!(filled.value.as_deref(), Some("Linus"), "fill must persist");

    // Request::Batch: one result per action, in order, fail-fast on error.
    let batched = manager
        .handle(Request::Batch {
            session: session.clone(),
            actions: vec![Action::Snapshot, Action::Extract],
        })
        .await
        .expect("batch");
    let results = match batched {
        ResponsePayload::Results(v) => v,
        other => panic!("expected Results, got {other:?}"),
    };
    assert_eq!(results.len(), 2, "one result per batched action");
    assert!(matches!(results[0], ActionResult::Snapshot { .. }));
    assert!(matches!(results[1], ActionResult::Text { .. }));
    // RotateProxy: re-launch on the next endpoint, URL restored.
    let rotated = manager
        .act(&session, Action::RotateProxy)
        .await
        .expect("rotate");
    assert!(
        matches!(rotated, ActionResult::Flag { ok: true }),
        "rotate ok"
    );
    let restored = manager
        .act(&session, Action::Snapshot)
        .await
        .expect("snapshot 3");
    let restored = match restored {
        ActionResult::Snapshot { snapshot } => snapshot.elements,
        other => panic!("expected Snapshot, got {other:?}"),
    };
    assert!(
        restored
            .iter()
            .any(|e| e.role == "textbox" && e.name.contains("Name")),
        "form must survive proxy rotation (URL restore): {restored:?}"
    );

    // Clean up.
    let closed = manager.handle(Request::Close { session }).await.unwrap();
    assert!(matches!(closed, ResponsePayload::Closed(true)));
}

/// Proves the `--backend dom` routing: opening with `Backend::Dom` on a default
/// `SessionManager` (whose primary launcher is CDP) selects the experimental DOM
/// launcher and serves a chrome-free snapshot — exercising `launcher_for`.
#[cfg(feature = "dom-backend")]
#[tokio::test]
async fn dom_backend_opened_via_session_option_routes_to_dom() {
    let manager = SessionManager::default();
    let opened = manager
        .handle(Request::Open {
            options: SessionOptions {
                url: Some(fixture_url("links.html")),
                backend: Some(Backend::Dom),
                ..SessionOptions::default()
            },
        })
        .await
        .expect("open");
    let session = match opened {
        ResponsePayload::Opened(info) => info.id,
        other => panic!("unexpected response {other:?}"),
    };
    // A snapshot with stable element names proves the DOM backend (not chrome)
    // served it.
    let snap = manager
        .act(&session, Action::Snapshot)
        .await
        .expect("snapshot");
    match snap {
        ActionResult::Snapshot { snapshot } => {
            assert!(
                snapshot.elements.iter().any(|e| e.name.contains("form")),
                "dom backend snapshot missing expected link: {snapshot:?}"
            );
        }
        other => panic!("expected Snapshot, got {other:?}"),
    }
    // And JS works on the routed session.
    let eval = manager
        .act(
            &session,
            Action::EvalText {
                expression: "1 + 2".into(),
            },
        )
        .await
        .expect("eval");
    match eval {
        ActionResult::Text { text } => assert_eq!(text, "3", "dom JS backend eval: {text}"),
        other => panic!("expected Text, got {other:?}"),
    }
}

/// Proves the server-wide default (`VAKBROWSE_BACKEND`): a manager configured
/// with `default_backend = Dom`, opened with `backend: None` (client omitted it),
/// routes to the DOM backend — no chrome.
#[cfg(feature = "dom-backend")]
#[tokio::test]
async fn dom_backend_server_default_backend_routes_to_dom() {
    let manager = SessionManager::with_policy_and_backend(Policy::default(), Backend::Dom);
    let opened = manager
        .handle(Request::Open {
            options: SessionOptions {
                url: Some(fixture_url("links.html")),
                ..SessionOptions::default() // backend explicitly omitted
            },
        })
        .await
        .expect("open");
    let session = match opened {
        ResponsePayload::Opened(info) => info.id,
        other => panic!("unexpected response {other:?}"),
    };
    let snap = manager
        .act(&session, Action::Snapshot)
        .await
        .expect("snapshot");
    match snap {
        ActionResult::Snapshot { snapshot } => {
            assert!(
                snapshot.elements.iter().any(|e| e.name.contains("form")),
                "default-backend=dom should serve a chrome-free snapshot: {snapshot:?}"
            );
        }
        other => panic!("expected Snapshot, got {other:?}"),
    }
}

#[tokio::test]
async fn dom_backend_unknown_session_is_clean_error() {
    let manager = dom_manager();
    let resp = manager
        .handle(Request::Act {
            session: SessionId::new("nope"),
            action: Action::Snapshot,
        })
        .await
        .unwrap();
    assert!(
        matches!(resp, ResponsePayload::Error(_)),
        "expected error for unknown session, got {resp:?}"
    );
}

/// Policy gate is enforced on the non-CDP path too: a `file://` open against an
/// `https://`-allowlisted policy must surface a classified `Policy` error
/// (not a generic engine error / opaque string). This is the wire-model
/// guarantee `vakd doctor` does not currently cover.
#[tokio::test]
async fn dom_backend_policy_blocks_navigation_classification() {
    let manager = SessionManager::new(
        Policy {
            url_allow_prefixes: vec!["https://allowed/".into()],
        },
        Arc::new(DomLauncher),
    );
    let resp = manager
        .handle(Request::Open {
            options: SessionOptions {
                url: Some(fixture_url("form.html")),
                ..SessionOptions::default()
            },
        })
        .await
        .unwrap();
    match resp {
        ResponsePayload::Error(ServiceError::Policy(_)) => {}
        other => panic!("expected Policy error, got {other:?}"),
    }
}

/// `Request::Batch` fails fast: the first failing action aborts the batch and
/// the whole frame surfaces that action's `ServiceError` kind (here a
/// `Timeout` from `wait_url` against a never-matching pattern), NOT a partial
/// `Results` payload.
#[tokio::test]
async fn dom_backend_batch_fail_fast_surfaces_error_kind() {
    let manager = dom_manager();
    let opened = manager
        .handle(Request::Open {
            options: SessionOptions {
                url: Some(fixture_url("form.html")),
                ..SessionOptions::default()
            },
        })
        .await
        .expect("open");
    let session = match opened {
        ResponsePayload::Opened(info) => info.id,
        other => panic!("{other:?}"),
    };
    let resp = manager
        .handle(Request::Batch {
            session,
            actions: vec![
                Action::Snapshot, // ok
                Action::WaitForUrl {
                    pattern: "never-matches".into(),
                    timeout_ms: 50,
                },
            ],
        })
        .await
        .unwrap();
    match resp {
        ResponsePayload::Error(ServiceError::Timeout(_)) => {}
        other => panic!("expected Timeout error from fail-fast batch, got {other:?}"),
    }
}

/// CSS selector resolution on the DOM backend: `find_by_css` returns refs in
/// snapshot order, and a found ref drives a real `click` → navigation. Uses
/// `links.html`'s `<a href="form.html">` found by an exact attribute matcher,
/// proving the one-command model (find → act → click) works chrome-free.
#[cfg(feature = "dom-backend")]
#[tokio::test]
async fn dom_backend_find_by_css_drives_click_to_navigate() {
    let manager = SessionManager::with_policy_and_backend(Policy::default(), Backend::Dom);
    let opened = manager
        .handle(Request::Open {
            options: SessionOptions {
                url: Some(fixture_url("links.html")),
                ..SessionOptions::default() // backend omitted -> server default (Dom)
            },
        })
        .await
        .expect("open");
    let session = match opened {
        ResponsePayload::Opened(info) => info.id,
        other => panic!("{other:?}"),
    };

    // Exact attribute value: only the `Go to the form` link matches.
    let found = manager
        .act(
            &session,
            Action::FindByCss {
                selector: "a[href=\"form.html\"]".into(),
            },
        )
        .await
        .expect("find_by_css");
    let refs = match found {
        ActionResult::Elements { refs } => refs,
        other => panic!("expected Elements, got {other:?}"),
    };
    assert_eq!(refs.len(), 1, "exactly one link matches: {refs:?}");
    assert_eq!(refs[0].0, "@e1");

    // The returned ref is immediately clickable and navigates to form.html.
    let clicked = manager
        .act(
            &session,
            Action::Click {
                r#ref: refs[0].0.clone(),
            },
        )
        .await
        .expect("click");
    match clicked {
        ActionResult::Clicked { navigated, url } => {
            assert!(navigated, "click should navigate");
            assert!(
                url.as_deref().unwrap_or("").ends_with("form.html"),
                "landed URL should be form.html, got {url:?}"
            );
        }
        other => panic!("expected Clicked, got {other:?}"),
    }
}

/// `find_by_css` honors combinators + comma groups against the snapshot turn:
/// `a, button` matches every link and button; descendant `body a` scopes to
/// anchors; a non-matching selector returns an empty ref set (NOT an error).
#[cfg(feature = "dom-backend")]
#[tokio::test]
async fn dom_backend_find_by_css_combinator_and_empty_semantics() {
    let manager = SessionManager::with_policy_and_backend(Policy::default(), Backend::Dom);
    let opened = manager
        .handle(Request::Open {
            options: SessionOptions {
                url: Some(fixture_url("links.html")),
                ..SessionOptions::default()
            },
        })
        .await
        .expect("open");
    let session = match opened {
        ResponsePayload::Opened(info) => info.id,
        other => panic!("{other:?}"),
    };

    let all = manager
        .act(
            &session,
            Action::FindByCss {
                selector: "a, button".into(),
            },
        )
        .await
        .expect("find");
    match all {
        ActionResult::Elements { refs } => {
            assert_eq!(refs.len(), 2, "two anchors in links.html: {refs:?}")
        }
        other => panic!("expected Elements, got {other:?}"),
    }

    // no matches -> empty results, not an error
    let none = manager
        .act(
            &session,
            Action::FindByCss {
                selector: "main input[type=text]".into(),
            },
        )
        .await
        .expect("find empty");
    match none {
        ActionResult::Elements { refs } => assert!(refs.is_empty()),
        other => panic!("expected Elements, got {other:?}"),
    }

    // invalid selector (pseudo) -> classified engine error, not a panic
    let bad = manager
        .act(
            &session,
            Action::FindByCss {
                selector: "a:hover".into(),
            },
        )
        .await;
    assert!(
        bad.is_err(),
        "unsupported selector must surface an error: {bad:?}"
    );
}

/// `source` returns the page HTML via the DOM backend (chrome-free).
#[cfg(feature = "dom-backend")]
#[tokio::test]
async fn dom_backend_source_returns_html() {
    let manager = SessionManager::with_policy_and_backend(Policy::default(), Backend::Dom);
    let opened = manager
        .handle(Request::Open {
            options: SessionOptions {
                url: Some(fixture_url("links.html")),
                ..SessionOptions::default()
            },
        })
        .await
        .expect("open");
    let session = match opened {
        ResponsePayload::Opened(info) => info.id,
        other => panic!("{other:?}"),
    };

    let source = manager
        .act(&session, Action::Source)
        .await
        .expect("source");
    match source {
        ActionResult::Text { text } => {
            assert!(
                text.contains("<html") && text.contains("</html>"),
                "source should contain full HTML document: {text}"
            );
            assert!(
                text.contains("Go to the form"),
                "source should contain link text from fixture: {text}"
            );
        }
        other => panic!("expected Text, got {other:?}"),
    }
}

/// `downloads` on the DOM backend returns `Unsupported` (no chrome = no download dir).
#[tokio::test]
async fn dom_backend_downloads_unsupported() {
    let manager = dom_manager();
    let opened = manager
        .handle(Request::Open {
            options: SessionOptions {
                url: Some(fixture_url("links.html")),
                ..SessionOptions::default()
            },
        })
        .await
        .expect("open");
    let session = match opened {
        ResponsePayload::Opened(info) => info.id,
        other => panic!("{other:?}"),
    };

    let resp = manager.act(&session, Action::Downloads).await;
    assert!(
        matches!(resp, Err(VakError::Unsupported(_))),
        "downloads on DOM backend should be Unsupported, got {resp:?}"
    );
}

/// `set_file_chooser` on the DOM backend returns `Unsupported` (no CDP).
#[tokio::test]
async fn dom_backend_set_file_chooser_unsupported() {
    let manager = dom_manager();
    let opened = manager
        .handle(Request::Open {
            options: SessionOptions {
                url: Some(fixture_url("form.html")),
                ..SessionOptions::default()
            },
        })
        .await
        .expect("open");
    let session = match opened {
        ResponsePayload::Opened(info) => info.id,
        other => panic!("{other:?}"),
    };

    let resp = manager
        .act(
            &session,
            Action::SetFileChooser {
                r#ref: "@e0".into(),
                paths: vec!["/tmp/test.txt".into()],
            },
        )
        .await;
    assert!(
        matches!(resp, Err(VakError::Unsupported(_))),
        "set_file_chooser on DOM backend should be Unsupported, got {resp:?}"
    );
}

/// Multi-session concurrency: spawn N DOM sessions in parallel, each
/// independent, each producing its own snapshot ref numbering. Proves the
/// session table is isolated no matter which backend is injected.
#[tokio::test]
async fn dom_backend_multi_session_concurrency() {
    let manager = dom_manager();
    let urls = [
        fixture_url("form.html"),
        fixture_url("links.html"),
        fixture_url("links.html"),
    ];

    // Open all sessions concurrently.
    let opens: Vec<_> = urls
        .iter()
        .map(|url| {
            manager.handle(Request::Open {
                options: SessionOptions {
                    url: Some(url.clone()),
                    ..SessionOptions::default()
                },
            })
        })
        .collect();
    let results = futures::future::join_all(opens).await;

    let mut session_ids = Vec::new();
    for resp in results {
        match resp.unwrap() {
            ResponsePayload::Opened(info) => session_ids.push(info.id),
            other => panic!("expected Opened, got {other:?}"),
        }
    }
    assert_eq!(session_ids.len(), 3);

    // Snapshot all concurrently — each must be independent.
    let snaps: Vec<_> = session_ids
        .iter()
        .map(|sid| manager.act(sid, Action::Snapshot))
        .collect();
    let snap_results = futures::future::join_all(snaps).await;

    for (i, result) in snap_results.iter().enumerate() {
        let snap = result.as_ref().expect("snapshot {i}");
        match snap {
            ActionResult::Snapshot { snapshot } => {
                // form.html has a button "Send application"; links.html has no form.
                if i == 0 {
                    assert!(
                        snapshot.elements.iter().any(|e| e.role == "button"),
                        "session 0 (form.html) should have a button"
                    );
                } else {
                    assert!(
                        !snapshot.elements.iter().any(|e| e.role == "button"),
                        "session {i} (links.html) should NOT have a button"
                    );
                }
            }
            other => panic!("expected Snapshot, got {other:?}"),
        }
    }

    // Clean up all sessions.
    for sid in &session_ids {
        let _ = manager.handle(Request::Close { session: sid.clone() }).await;
    }
}

/// Multi-session concurrency via batch: open two sessions, batch-navigate both
/// in one round-trip each, verify they land on different pages.
#[tokio::test]
async fn dom_backend_multi_session_batch_isolation() {
    let manager = dom_manager();
    let form = fixture_url("form.html");
    let links = fixture_url("links.html");

    let s1_id = match manager
        .handle(Request::Open {
            options: SessionOptions {
                url: Some(form),
                ..SessionOptions::default()
            },
        })
        .await
        .unwrap()
        {
        ResponsePayload::Opened(info) => info.id,
        other => panic!("{other:?}"),
    };
    let s2_id = match manager
        .handle(Request::Open {
            options: SessionOptions {
                url: Some(links),
                ..SessionOptions::default()
            },
        })
        .await
        .unwrap()
        {
        ResponsePayload::Opened(info) => info.id,
        other => panic!("{other:?}"),
    };

    // Run a batch on each session in parallel.
    let b1 = manager.handle(Request::Batch {
        session: s1_id.clone(),
        actions: vec![Action::Snapshot, Action::Source],
    });
    let b2 = manager.handle(Request::Batch {
        session: s2_id.clone(),
        actions: vec![Action::Snapshot, Action::Source],
    });
    let (r1, r2) = futures::join!(b1, b2);

    let r1 = match r1.unwrap() {
        ResponsePayload::Results(v) => v,
        other => panic!("expected Results s1, got {other:?}"),
    };
    let r2 = match r2.unwrap() {
        ResponsePayload::Results(v) => v,
        other => panic!("expected Results s2, got {other:?}"),
    };
    assert_eq!(r1.len(), 2);
    assert_eq!(r2.len(), 2);

    // s1 (form.html) source must contain "Adopt a pet"; s2 (links.html) must not.
    match &r1[1] {
        ActionResult::Text { text } => assert!(text.contains("Adopt"), "s1 source: {text}"),
        other => panic!("expected Text, got {other:?}"),
    }
    match &r2[1] {
        ActionResult::Text { text } => assert!(!text.contains("Adopt"), "s2 should not be form: {text}"),
        other => panic!("expected Text, got {other:?}"),
    }

    // Clean up.
    let _ = manager.handle(Request::Close { session: s1_id }).await;
    let _ = manager.handle(Request::Close { session: s2_id }).await;
}

/// Cookie round-trip through the `DomLauncher` wire model: `Action::SetCookie`
/// → `Action::Cookies` must echo back the set cookie. This closes the gap
/// where the only cookie test was a CDP smoke test that asserted nothing (it
/// couldn't, on `file://` origins) — here the DOM backend's in-memory cookie
/// jar makes the round-trip hermetically verifiable through `SessionManager`.
#[tokio::test]
async fn dom_backend_cookie_set_get_roundtrip() {
    let manager = dom_manager();
    let opened = manager
        .handle(Request::Open {
            options: SessionOptions {
                url: Some(fixture_url("form.html")),
                ..SessionOptions::default()
            },
        })
        .await
        .expect("open");
    let session = match opened {
        ResponsePayload::Opened(info) => info.id,
        other => panic!("{other:?}"),
    };

    manager
        .act(
            &session,
            Action::SetCookie {
                cookie: vakbrowse_core::CookieInput {
                    name: "session".into(),
                    value: "abc123".into(),
                    domain: "localhost".into(),
                    path: "/".into(),
                    secure: false,
                    http_only: true,
                    same_site: Some("Strict".into()),
                },
            },
        )
        .await
        .expect("set_cookie");

    let got = manager
        .act(&session, Action::Cookies)
        .await
        .expect("cookies");
    match got {
        ActionResult::Cookies { cookies } => {
            let c = cookies
                .iter()
                .find(|c| c.name == "session")
                .unwrap_or_else(|| panic!("session cookie not persisted: {cookies:?}"));
            assert_eq!(c.value, "abc123", "cookie value mismatch");
            assert_eq!(c.path, "/", "cookie path mismatch");
            // SameSite must survive the round-trip.
            assert_eq!(
                c.same_site.as_deref(),
                Some("Strict"),
                "same_site must survive round-trip"
            );
        }
        other => panic!("expected Cookies, got {other:?}"),
    }

    // Clearing removes it.
    manager
        .act(&session, Action::ClearCookies)
        .await
        .expect("clear_cookies");
    let empty = manager
        .act(&session, Action::Cookies)
        .await
        .expect("cookies after clear");
    match empty {
        ActionResult::Cookies { cookies } => {
            assert!(
                !cookies.iter().any(|c| c.name == "session"),
                "cookie not cleared: {cookies:?}"
            );
        }
        other => panic!("expected Cookies, got {other:?}"),
    }
}
