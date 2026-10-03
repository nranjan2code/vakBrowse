//! Compute-aware governance: hibernation keeps state, the admission queue
//! waits/evicts/answers instead of dropping work, the private-network guard
//! covers redirects/iframes/fetch, and lean mode skips images.

use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use url::Url;
use vakbrowse_core::{CookieInput, SessionId};
use vakbrowse_server::governor::{Capacity, Governor, GovernorConfig};
use vakbrowse_server::{
    Action, ActionResult, Policy, Request, ResponsePayload, ServiceError, SessionManager,
    SessionOptions,
};

/// Chrome launches are heavy; run this binary's tests one at a time.
static LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

fn fixture_url(name: &str) -> String {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures")
        .join(name);
    Url::from_file_path(path.canonicalize().expect("fixture exists"))
        .unwrap()
        .to_string()
}

fn small_governor(evict_min_idle_secs: u64) -> Arc<Governor> {
    let cap = Capacity {
        memory_mb: 2048,
        memory_source: "assumed".into(),
        cpus: 2,
    };
    let mut cfg = GovernorConfig::for_capacity(&cap);
    cfg.budget_mb = 200; // room for exactly one 150 MB session
    cfg.session_cost_mb = 150;
    cfg.queue_timeout_secs = 3;
    cfg.evict_min_idle_secs = evict_min_idle_secs;
    cfg.hibernate_after_secs = None;
    let g = Governor::new(cap, cfg);
    g.set_live_available_for_test(None);
    g
}

async fn open(m: &SessionManager, url: Option<String>) -> ResponsePayload {
    m.handle(Request::Open {
        options: SessionOptions {
            url,
            ..Default::default()
        },
    })
    .await
    .unwrap()
}

fn opened(p: ResponsePayload) -> SessionId {
    match p {
        ResponsePayload::Opened(info) => info.id,
        other => panic!("expected open, got {other:?}"),
    }
}

#[tokio::test]
async fn hibernation_round_trips_tabs_urls_and_cookies() {
    let _g = LOCK.lock().await;
    let m = SessionManager::default();
    let id = opened(open(&m, Some(fixture_url("hello.html"))).await);
    // Chrome's own startup tab must not be adopted as a phantom tab.
    let ActionResult::Tabs { tabs: fresh } = m.act(&id, Action::Tabs).await.unwrap() else {
        panic!("tabs")
    };
    assert_eq!(fresh.len(), 1, "fresh session has one tab: {fresh:?}");
    let ActionResult::TabOpened { tab } = m
        .act(
            &id,
            Action::NewTab {
                url: Some(fixture_url("links.html")),
            },
        )
        .await
        .unwrap()
    else {
        panic!("new tab")
    };
    m.act(
        &id,
        Action::SetCookie {
            cookie: CookieInput {
                name: "vb_keep".into(),
                value: "1".into(),
                domain: "example.test".into(),
                path: "/".into(),
                secure: false,
                http_only: false,
                same_site: None,
                expires: Some(4_000_000_000.0),
            },
        },
    )
    .await
    .unwrap();
    let ActionResult::Tabs { tabs: before } = m.act(&id, Action::Tabs).await.unwrap() else {
        panic!("tabs")
    };
    let used_live = m.status().await.used_mb;
    assert!(used_live > 0);

    assert!(m.hibernate(&id).await, "idle session must hibernate");
    let st = m.status().await;
    assert_eq!((st.hibernated, st.used_mb), (1, 0), "memory released");
    assert!(m.list().await[0].hibernated);

    // The next action restores transparently, under the same tab ids.
    let ActionResult::Tabs { tabs: after } = m.act(&id, Action::Tabs).await.unwrap() else {
        panic!("tabs")
    };
    let key = |t: &vakbrowse_core::TabInfo| (t.id.clone(), t.url.clone());
    let mut b: Vec<_> = before.iter().map(key).collect();
    let mut a: Vec<_> = after.iter().map(key).collect();
    b.sort();
    a.sort();
    assert_eq!(a, b, "tabs restored with ids and urls");
    assert_eq!(after[0].id, tab.id, "active tab restored");
    let ActionResult::Cookies { cookies } = m.act(&id, Action::Cookies).await.unwrap() else {
        panic!("cookies")
    };
    assert!(
        cookies.iter().any(|c| c.name == "vb_keep"),
        "cookie restored"
    );
    assert!(!m.list().await[0].hibernated);
    assert_eq!(
        m.status().await.used_mb,
        used_live,
        "lease re-taken on restore"
    );
}

#[tokio::test]
async fn full_budget_waits_then_answers_busy_without_eviction() {
    let _g = LOCK.lock().await;
    let m = SessionManager::default().with_governor(small_governor(3600));
    let _s1 = opened(open(&m, None).await);
    let t = Instant::now();
    let r = open(&m, None).await;
    assert!(
        matches!(r, ResponsePayload::Error(ServiceError::Busy(_))),
        "got {r:?}"
    );
    assert!(
        t.elapsed() >= Duration::from_millis(2500),
        "it queued first"
    );
    assert_eq!(m.list().await.len(), 1, "nothing half-opened");
}

#[tokio::test]
async fn pressure_hibernates_idle_sessions_and_nothing_is_lost() {
    let _g = LOCK.lock().await;
    let m = Arc::new(SessionManager::default().with_governor(small_governor(0)));
    let s1 = opened(open(&m, Some(fixture_url("hello.html"))).await);

    // Three more opens at once, budget for one: they queue FIFO and each
    // gets room by hibernating an idle session.
    let mut tasks = Vec::new();
    for _ in 0..3 {
        let m = m.clone();
        tasks.push(tokio::spawn(async move { open(&m, None).await }));
    }
    for t in tasks {
        opened(t.await.unwrap());
    }
    let list = m.list().await;
    assert_eq!(list.len(), 4, "every request was served");
    assert_eq!(list.iter().filter(|s| s.hibernated).count(), 3);
    assert_eq!(m.status().await.used_mb, 150, "only one browser resident");

    // The first session comes back on demand, with its page.
    let ActionResult::Text { text } = m
        .act(
            &s1,
            Action::Extract {
                offset: 0,
                max_chars: None,
            },
        )
        .await
        .unwrap()
    else {
        panic!("extract")
    };
    assert!(text.contains("hello.html"), "{text}");
    assert_eq!(m.status().await.used_mb, 150);
}

#[tokio::test]
async fn lean_mode_skips_images() {
    let _g = LOCK.lock().await;
    let m = SessionManager::default();
    let mut widths = Vec::new();
    for lean in [true, false] {
        let id = opened(
            m.handle(Request::Open {
                options: SessionOptions {
                    url: Some(fixture_url("image.html")),
                    lean: Some(lean),
                    ..Default::default()
                },
            })
            .await
            .unwrap(),
        );
        m.act(
            &id,
            Action::WaitForTruthy {
                expression: "document.readyState === 'complete'".into(),
                timeout_ms: 5000,
            },
        )
        .await
        .unwrap();
        let ActionResult::Text { text } = m
            .act(
                &id,
                Action::EvalText {
                    expression: "document.images[0].naturalWidth".into(),
                },
            )
            .await
            .unwrap()
        else {
            panic!("eval")
        };
        widths.push(text);
        m.close(&id).await.unwrap();
    }
    assert_eq!(
        widths,
        ["0", "1"],
        "lean blocks the image, default loads it"
    );
}

/// A tiny local origin: `/` serves `body` (CORS-open), `/redirect` 302s to
/// `redirect_to`.
async fn origin(body: String, redirect_to: Option<String>) -> u16 {
    let l = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = l.local_addr().unwrap().port();
    tokio::spawn(async move {
        while let Ok((mut s, _)) = l.accept().await {
            let (body, redirect_to) = (body.clone(), redirect_to.clone());
            tokio::spawn(async move {
                let mut buf = vec![0u8; 4096];
                let n = s.read(&mut buf).await.unwrap_or(0);
                let req = String::from_utf8_lossy(&buf[..n]);
                let resp = match (&redirect_to, req.starts_with("GET /redirect")) {
                    (Some(to), true) => format!(
                        "HTTP/1.1 302 Found\r\nLocation: {to}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                    ),
                    _ => format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nAccess-Control-Allow-Origin: *\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    ),
                };
                let _ = s.write_all(resp.as_bytes()).await;
            });
        }
    });
    port
}

#[tokio::test]
async fn guard_blocks_private_targets_through_every_path() {
    let _g = LOCK.lock().await;
    let secret = origin("<h1>INTERNAL-SECRET</h1>".into(), None).await;
    let b = format!("http://127.0.0.1:{secret}/");
    let page = format!(
        r#"<h1>Public page</h1><iframe src="{b}"></iframe><p id="f">pending</p>
<script>fetch("{b}").then(r=>r.text()).then(t=>f.textContent=t).catch(()=>f.textContent="fetch-blocked")</script>"#
    );
    let allowed = origin(page, Some(b.clone())).await;
    let a = format!("http://127.0.0.1:{allowed}/");
    let m = SessionManager::new(Policy {
        block_private: true,
        private_allow: vec![format!("127.0.0.1:{allowed}")],
        ..Policy::default()
    });
    assert!(m.status().await.private_network_guard);

    // Direct: refused before the browser tries.
    let r = open(&m, Some(b.clone())).await;
    assert!(
        matches!(r, ResponsePayload::Error(ServiceError::Policy(ref e)) if e.contains("private")),
        "got {r:?}"
    );

    // Exempt page loads; its iframe and fetch to the private origin do not.
    let id = opened(open(&m, Some(a.clone())).await);
    m.act(
        &id,
        Action::WaitForTruthy {
            expression: "document.getElementById('f').textContent !== 'pending'".into(),
            timeout_ms: 10_000,
        },
    )
    .await
    .unwrap();
    let ActionResult::Text { text } = m
        .act(
            &id,
            Action::EvalText {
                expression: "document.getElementById('f').textContent".into(),
            },
        )
        .await
        .unwrap()
    else {
        panic!("eval")
    };
    assert!(!text.contains("INTERNAL-SECRET"), "fetch leaked: {text}");
    let ActionResult::Snapshot { snapshot } = m.act(&id, Action::Snapshot).await.unwrap() else {
        panic!("snapshot")
    };
    let rendered = vakbrowse_server::render::snapshot_text(&snapshot);
    assert!(
        !rendered.contains("INTERNAL-SECRET"),
        "iframe leaked: {rendered}"
    );

    // Redirect from the exempt origin into the private one.
    let r = m
        .act(
            &id,
            Action::Navigate {
                url: format!("{a}redirect"),
            },
        )
        .await;
    assert!(
        matches!(r, Err(vakbrowse_core::VakError::Policy(_))),
        "got {r:?}"
    );
    let ActionResult::Text { text } = m
        .act(
            &id,
            Action::EvalText {
                expression: "document.body ? document.body.innerText : ''".into(),
            },
        )
        .await
        .unwrap()
    else {
        panic!("eval")
    };
    assert!(!text.contains("INTERNAL-SECRET"), "redirect leaked: {text}");
}
