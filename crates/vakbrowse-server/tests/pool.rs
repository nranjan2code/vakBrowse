//! Pool behavior: session cap enforcement and idle reaping.

use vakbrowse_server::{PoolConfig, Request, ResponsePayload, SessionManager};

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

    let err = manager
        .handle(Request::Open {
            options: Default::default(),
        })
        .await
        .expect_err("third open must hit the cap");
    assert!(err.contains("cap reached"), "{err}");

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
