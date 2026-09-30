//! Pool behavior: session cap enforcement and idle reaping.

use vakbrowse_server::{PoolConfig, Request, ResponsePayload, ServiceError, SessionManager};

#[tokio::test]
async fn max_sessions_cap_is_enforced() {
    let manager = SessionManager::default().with_pool(PoolConfig {
        max_sessions: 2,
        idle_timeout_secs: None,
    });

    for i in 0..2 {
        let r = manager
            .handle(Request::Open {
                options: Default::default(),
            })
            .await
            .expect("open within cap");
        assert!(matches!(r, ResponsePayload::Opened(_)), "open #{i}");
    }

    let resp = manager
        .handle(Request::Open {
            options: Default::default(),
        })
        .await
        .unwrap();
    assert!(
        matches!(resp, ResponsePayload::Error(ServiceError::Policy(_))),
        "expected cap/policy error, got {resp:?}"
    );

    // Closing frees a slot.
    let sessions = manager.list().await;
    manager.close(&sessions[0].id).await.unwrap();
    let r = manager
        .handle(Request::Open {
            options: Default::default(),
        })
        .await;
    assert!(matches!(r, Ok(ResponsePayload::Opened(_))));
}

#[tokio::test]
async fn reaper_closes_idle_sessions() {
    let manager = std::sync::Arc::new(SessionManager::default().with_pool(PoolConfig {
        max_sessions: 8,
        idle_timeout_secs: Some(1), // 1s idle timeout
    }));
    manager.spawn_reaper();

    let r = manager
        .handle(Request::Open {
            options: Default::default(),
        })
        .await
        .unwrap();
    assert!(matches!(r, ResponsePayload::Opened(_)));
    assert_eq!(manager.list().await.len(), 1);

    // After >1s idle + a reaper tick (5s cadence) the session is gone.
    // Keep total test time bounded: wait up to ~9s.
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(9);
    while tokio::time::Instant::now() < deadline {
        if manager.list().await.is_empty() {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(250)).await;
    }
    panic!("idle session was not reaped");
}

/// The cap check used to run before a multi-second Chrome launch and the
/// insert after it, so N concurrent opens all passed the check.
#[tokio::test]
async fn concurrent_opens_cannot_exceed_cap() {
    let manager = std::sync::Arc::new(SessionManager::default().with_pool(PoolConfig {
        max_sessions: 2,
        idle_timeout_secs: None,
    }));
    let mut tasks = Vec::new();
    for _ in 0..5 {
        let m = manager.clone();
        tasks.push(tokio::spawn(async move {
            m.handle(Request::Open {
                options: Default::default(),
            })
            .await
            .unwrap()
        }));
    }
    let mut opened = 0;
    for t in tasks {
        match t.await.unwrap() {
            ResponsePayload::Opened(_) => opened += 1,
            ResponsePayload::Error(ServiceError::Policy(_)) => {}
            other => panic!("unexpected {other:?}"),
        }
    }
    assert_eq!(opened, 2, "exactly the cap may open");
    assert_eq!(manager.list().await.len(), 2);
}

/// A rotation that overlaps an in-flight action used to deadlock: `act` held
/// the page mutex and wanted `sessions`, `rotate_proxy` held `sessions` and
/// wanted the page mutex.
#[tokio::test]
async fn rotate_during_inflight_action_does_not_deadlock() {
    use vakbrowse_server::{Action, SessionOptions};
    let manager = std::sync::Arc::new(SessionManager::default());
    let ResponsePayload::Opened(info) = manager
        .handle(Request::Open {
            options: SessionOptions {
                proxies: vec!["http://127.0.0.1:9".into(), "http://127.0.0.1:10".into()],
                ..SessionOptions::default()
            },
        })
        .await
        .unwrap()
    else {
        panic!("open failed")
    };

    let m = manager.clone();
    let id = info.id.clone();
    let slow = tokio::spawn(async move {
        m.act(
            &id,
            Action::WaitForTruthy {
                // Succeeds after ~2s, so `act` reaches its post-action
                // `sessions` lock while a rotation is queued on the page.
                expression: "(window.__t ??= Date.now(), Date.now() - window.__t > 2000)".into(),
                timeout_ms: 10_000,
            },
        )
        .await
    });
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;

    let both = async {
        let rot = manager.act(&info.id, Action::RotateProxy).await;
        let waited = slow.await.unwrap();
        (rot, waited)
    };
    let (rot, waited) = tokio::time::timeout(std::time::Duration::from_secs(30), both)
        .await
        .expect("deadlock: rotate + in-flight action never finished");
    rot.expect("rotate");
    waited.expect("in-flight wait should complete");
    // The manager-wide lock must still be usable.
    assert_eq!(manager.list().await.len(), 1);
}
