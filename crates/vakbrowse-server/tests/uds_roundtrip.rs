//! UDS wire roundtrip: daemon-side serve task + one-shot client calls,
//! against a real (offline) browser session.

use std::path::PathBuf;
use std::sync::Arc;

use url::Url;
use vakbrowse_server::{Request, ResponsePayload, SessionManager, uds};

fn fixture_url(name: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures")
        .join(name);
    Url::from_file_path(path.canonicalize().expect("fixture exists"))
        .expect("absolute")
        .to_string()
}

#[tokio::test]
async fn wire_roundtrip_open_snapshot_close() {
    let dir = tempfile::tempdir().unwrap(); // keep alive for the whole test
    let socket = dir.path().join("t.sock");
    let manager = Arc::new(SessionManager::default());
    let server_manager = manager.clone();
    let serve_path = socket.clone();
    let _handle = tokio::spawn(async move {
        uds::serve(&serve_path, server_manager)
            .await
            .expect("serve ends with error only");
    });
    // Wait for the socket to appear (sockets are not regular files).
    for _ in 0..100 {
        if socket.exists() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    assert!(socket.exists(), "socket never appeared");

    let response = uds::call(
        &socket,
        Request::Open {
            options: Default::default(),
        },
    )
    .await
    .expect("call open");
    let session = match response {
        Ok(ResponsePayload::Opened(info)) => info.id,
        other => panic!("unexpected {other:?}"),
    };

    let response = uds::call(
        &socket,
        Request::Act {
            session: session.clone(),
            action: vakbrowse_server::Action::Navigate {
                url: fixture_url("hello.html"),
            },
        },
    )
    .await
    .expect("call navigate");
    assert!(
        matches!(&response, Ok(ResponsePayload::Result(vakbrowse_server::ActionResult::Navigated{title,..})) if title.contains("vakBrowse")),
        "got {response:?}"
    );

    let response = uds::call(
        &socket,
        Request::Act {
            session: session.clone(),
            action: vakbrowse_server::Action::Snapshot,
        },
    )
    .await
    .expect("call snapshot");
    assert!(matches!(response, Ok(ResponsePayload::Result(_))));

    let response = uds::call(&socket, Request::Close { session })
        .await
        .expect("call close");
    assert!(matches!(response, Ok(ResponsePayload::Closed(true))));
}
