//! SessionManager integration: full agent flow against a local fixture,
//! plus policy enforcement. Uses the real engine, fully offline.

use std::path::PathBuf;

use url::Url;
use vakbrowse_core::{ProfileId, SessionId};
use vakbrowse_server::{
    Action, ActionResult, Policy, Request, ResponsePayload, ServiceError, SessionManager,
    SessionOptions,
};

fn fixture_url(name: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures")
        .join(name);
    Url::from_file_path(path.canonicalize().expect("fixture exists"))
        .expect("absolute")
        .to_string()
}

#[tokio::test]
async fn open_navigate_snapshot_act() {
    let manager = SessionManager::default();

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
        other => panic!("unexpected response {other:?}"),
    };

    let snapshot = manager
        .act(&session, Action::Snapshot)
        .await
        .expect("snapshot");
    let elements = match snapshot {
        ActionResult::Snapshot { snapshot } => snapshot.elements,
        other => panic!("unexpected {other:?}"),
    };
    let send_ref = elements
        .iter()
        .find(|e| e.role == "button" && e.name.contains("Send"))
        .map(|e| e.r#ref.clone())
        .expect("button in snapshot");

    // Fill via wire-shaped action (serde-tagged path exercised end to end).
    let name_ref = elements
        .iter()
        .find(|e| e.role == "textbox" && e.name.contains("Name"))
        .map(|e| e.r#ref.0.clone())
        .unwrap();

    manager
        .act(
            &session,
            Action::Fill {
                r#ref: name_ref,
                text: "Grace".into(),
            },
        )
        .await
        .expect("fill");

    manager
        .act(
            &session,
            Action::Click {
                r#ref: send_ref.0.clone(),
            },
        )
        .await
        .expect("click");

    manager
        .act(
            &session,
            Action::WaitForTruthy {
                expression: "document.getElementById('out').textContent.includes('name=Grace')"
                    .into(),
                timeout_ms: 3_000,
            }
            ,
        )
        .await
        .expect("wait");

    let closed = manager.handle(Request::Close { session }).await.unwrap();
    assert!(matches!(closed, ResponsePayload::Closed(true)));
}

#[tokio::test]
async fn policy_blocks_navigation() {
    let manager = SessionManager::with_policy(Policy {
        url_allow_prefixes: vec!["https://allowed.example/".into()],
    });
    let resp = manager
        .handle(Request::Open {
            options: SessionOptions {
                url: Some("https://blocked.example/page".into()),
                ..SessionOptions::default()
            },
        })
        .await
        .unwrap();
    assert!(
        matches!(resp, ResponsePayload::Error(ServiceError::Policy(_))),
        "expected policy error, got {resp:?}"
    );
}

#[tokio::test]
async fn unknown_session_is_clean_error() {
    let manager = SessionManager::default();
    let resp = manager
        .handle(Request::Act {
            session: SessionId::new("nope"),
            action: Action::Snapshot,
        })
        .await
        .unwrap();
    assert!(
        matches!(resp, ResponsePayload::Error(ServiceError::NotFound(_))),
        "expected not-found error, got {resp:?}"
    );
}

#[tokio::test]
#[cfg(unix)]
async fn persistent_profile_reuses_dir() {
    let root = tempfile::tempdir().unwrap();
    let manager = SessionManager::default().with_profiles_root(root.path().to_path_buf());
    let opened = manager
        .handle(Request::Open {
            options: SessionOptions {
                profile: Some(ProfileId::new("agent-1")),
                ..SessionOptions::default()
            },
        })
        .await
        .unwrap();
    let info = match opened {
        ResponsePayload::Opened(i) => i,
        other => panic!("{other:?}"),
    };
    assert!(
        std::path::Path::new(&root.path().join("agent-1")).is_dir()
    );
    manager.close(&info.id).await.unwrap();
}

#[tokio::test]
async fn stealth_seed_patches_navigator_through_manager() {
    use std::path::PathBuf;
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/hello.html");
    let url = url::Url::from_file_path(path.canonicalize().unwrap())
        .unwrap()
        .to_string();

    let manager = SessionManager::default();
    let opened = manager
        .handle(Request::Open {
            options: SessionOptions {
                url: Some(url),
                stealth_seed: Some("dogfood".into()),
                ..SessionOptions::default()
            },
        })
        .await
        .unwrap();
    let session = match opened {
        ResponsePayload::Opened(info) => info.id,
        other => panic!("{other:?}"),
    };
    let _ = manager.act(&session, Action::Snapshot).await;
    let r = manager
        .act(&session, Action::EvalText { expression: "String(navigator.webdriver)".into() })
        .await
        .unwrap();
    assert!(
        matches!(&r, ActionResult::Text { text } if text == "false"),
        "webdriver must be patched through the server path, got {r:?}"
    );
}

