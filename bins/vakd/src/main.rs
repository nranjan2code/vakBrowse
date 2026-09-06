use clap::{Parser, Subcommand};
use std::sync::Arc;
use vakbrowse_engine::cft;
use vakbrowse_engine::{CdpLauncher, EngineLauncher, LaunchOptions};
use vakbrowse_server::{Backend, Policy, Request, SessionManager};

#[derive(Parser)]
#[command(name = "vakd", version, about = "vakBrowse daemon")]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Verify the engine: resolve/download chrome-headless-shell and probe a page.
    Doctor {
        #[arg(long)]
        no_probe: bool,
    },
    /// Run the daemon (foreground), owning all browser sessions.
    Serve {
        #[arg(long, default_value = "/tmp/vakd.sock")]
        socket: String,
        /// URL allow prefixes; repeatable. Empty = allow everything.
        #[arg(long = "allow")]
        allow_prefixes: Vec<String>,
        /// Hard cap on concurrent browser sessions.
        #[arg(long, default_value_t = 32)]
        max_sessions: usize,
        /// Close sessions idle longer than this many seconds (omit = never reap).
        #[arg(long)]
        idle_timeout_secs: Option<u64>,
    },
    /// Ask the daemon what sessions are live.
    Status {
        #[arg(long, default_value = "/tmp/vakd.sock")]
        socket: String,
    },
}

#[tokio::main]
async fn main() -> vakbrowse_core::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();

    match Cli::parse().command {
        Some(Commands::Doctor { no_probe }) => doctor(no_probe).await,
        Some(Commands::Serve {
            socket,
            allow_prefixes,
            max_sessions,
            idle_timeout_secs,
        }) => {
            serve(
                &socket,
                Policy {
                    url_allow_prefixes: allow_prefixes,
                },
                vakbrowse_server::PoolConfig {
                    max_sessions,
                    idle_timeout_secs,
                },
            )
            .await
        }
        Some(Commands::Status { socket }) => status(&socket).await,
        None => {
            eprintln!("vakd: try `vakd doctor`, `vakd serve` or `vakd status`");
            Ok(())
        }
    }
}

async fn serve(
    socket: &str,
    policy: Policy,
    pool: vakbrowse_server::PoolConfig,
) -> vakbrowse_core::Result<()> {
    let default_backend = std::env::var("VAKBROWSE_BACKEND")
        .ok()
        .map(|s| match s.to_ascii_lowercase().as_str() {
            "dom" => Backend::Dom,
            _ => Backend::Cdp,
        })
        .unwrap_or(Backend::Cdp);
    let manager =
        Arc::new(SessionManager::with_policy_and_backend(policy, default_backend).with_pool(pool));
    manager.spawn_reaper();
    let path = std::path::PathBuf::from(socket);

    // Graceful shutdown: race the UDS server against SIGINT/SIGTERM. On
    // signal, close all sessions (drops CDP browser handles → tears down
    // chrome) and remove the socket file before exiting. Previously Ctrl-C
    // killed the process abruptly, orphaning chrome processes.
    let serve_fut = vakbrowse_server::uds::serve(&path, manager.clone());
    let shutdown = async {
        let _ = tokio::signal::ctrl_c().await;
    };
    tokio::select! {
        result = serve_fut => {
            // Server exited on its own (listener error). Clean up the socket.
            let _ = std::fs::remove_file(&path);
            result
        }
        _ = shutdown => {
            tracing::info!("shutdown signal received; closing {} session(s)", manager.stats().await.0);
            manager.close_all().await;
            let _ = std::fs::remove_file(&path);
            tracing::info!("vakd stopped");
            Ok(())
        }
    }
}

async fn status(socket: &str) -> vakbrowse_core::Result<()> {
    let response =
        vakbrowse_server::uds::call(std::path::Path::new(socket), Request::ListSessions).await?;
    match response {
        Ok(vakbrowse_server::ResponsePayload::Sessions(sessions)) => {
            if sessions.is_empty() {
                println!("no live sessions");
            }
            for s in sessions {
                println!(
                    "{} profile={:?} url={}",
                    s.id,
                    s.profile.as_ref().map(|p| p.0.clone()),
                    s.url
                );
            }
            Ok(())
        }
        other => {
            println!("{other:?}");
            Ok(())
        }
    }
}

async fn doctor(no_probe: bool) -> vakbrowse_core::Result<()> {
    println!("vakBrowse doctor");
    println!(
        "  platform : {}/{}",
        std::env::consts::OS,
        std::env::consts::ARCH
    );
    println!(
        "  cft key  : {}",
        cft::platform_key().unwrap_or("<unsupported>")
    );

    if let Some(system) = cft::find_system_chrome() {
        println!("  system   : {}", system.display());
    } else {
        println!("  system   : none found");
    }

    let launcher = CdpLauncher::default();
    let artifact = cft::ensure_headless_shell(&launcher.cft).await?;
    println!(
        "  managed  : v{} @ {}",
        artifact.version,
        artifact.executable.display()
    );

    if !no_probe {
        println!("  probing  : launching + navigating data: URL ...");
        let mut session = launcher.launch(&LaunchOptions::default()).await?;
        let nav = session
            .navigate("data:text/html,<title>probe</title><button>ok</button>")
            .await?;
        let h1 = session
            .eval_text("document.body.firstElementChild.textContent")
            .await?;
        let snap = session.snapshot().await?;
        println!("             title={:?}", nav.title);
        println!("             body-text={h1}");
        println!(
            "             snapshot: {} interactive element(s)",
            snap.elements.len()
        );
        drop(session);

        // Wire-model self-test: drive the *daemon* dispatch (SessionManager ->
        // Request/Action/Policy -> ResponsePayload), not just the raw engine.
        // A `data:` URL keeps it self-contained (no fixture files shipped with
        // the binary) and offline. Catches regressions in policy enforcement,
        // batch fail-fast, and ActionResult classification that the raw engine
        // probe above cannot see.
        wire_self_test().await?;
        println!("  probe    : ok");
    }

    println!("all checks passed");
    Ok(())
}

/// Exercise the same `SessionManager::handle` path the daemon serves, against a
/// `data:` page — open/snapshot/click/extract/batch/close, asserting each wire
/// shape. The button here has no navigation handler, so the click is expected
/// to return `Done`; this tests dispatch/classification, not navigation.
async fn wire_self_test() -> vakbrowse_core::Result<()> {
    use vakbrowse_core::{SessionId, VakError};
    use vakbrowse_server::{Action, ActionResult, Request, ResponsePayload, SessionManager};

    let manager = SessionManager::default();
    let url = "data:text/html,<title>probe</title><button id=b>Click me</button>";

    let opened = manager
        .handle(Request::Open {
            options: vakbrowse_server::SessionOptions {
                url: Some(url.to_string()),
                ..vakbrowse_server::SessionOptions::default()
            },
        })
        .await
        .map_err(|e| VakError::Engine(format!("wire open: {e}")))?;
    let id = match opened {
        ResponsePayload::Opened(info) => info.id,
        other => return Err(VakError::Engine(format!("wire open shape: {other:?}"))),
    };

    // Snapshot: find the button by a11y role/name, then click its @eN ref.
    let snap = manager
        .act(&id, Action::Snapshot)
        .await
        .map_err(|e| VakError::Engine(format!("wire snapshot: {e}")))?;
    let button_ref = match snap {
        ActionResult::Snapshot { snapshot } => snapshot
            .elements
            .iter()
            .find(|e| e.role == "button" && e.name == "Click me")
            .map(|e| e.r#ref.0.clone()),
        other => return Err(VakError::Engine(format!("wire snapshot shape: {other:?}"))),
    }
    .ok_or_else(|| VakError::Engine("wire: button not in snapshot".into()))?;

    let clicked = manager
        .act(&id, Action::Click { r#ref: button_ref })
        .await
        .map_err(|e| VakError::Engine(format!("wire click: {e}")))?;
    assert!(
        matches!(
            clicked,
            ActionResult::Clicked {
                navigated: false,
                url: None
            }
        ),
        "click -> Clicked{{navigated=false}}, got {clicked:?}"
    );

    let extracted = manager
        .act(&id, Action::Extract)
        .await
        .map_err(|e| VakError::Engine(format!("wire extract: {e}")))?;
    match extracted {
        ActionResult::Text { text } if text.contains("Click me") => {}
        other => return Err(VakError::Engine(format!("wire extract shape: {other:?}"))),
    }

    // Batch: one result per action, fail-fast on first error.
    let batched = manager
        .handle(Request::Batch {
            session: id.clone(),
            actions: vec![Action::Snapshot, Action::Extract],
        })
        .await
        .map_err(|e| VakError::Engine(format!("wire batch: {e}")))?;
    match batched {
        ResponsePayload::Results(results) if results.len() == 2 => {}
        other => return Err(VakError::Engine(format!("wire batch shape: {other:?}"))),
    }

    let closed = manager
        .handle(Request::Close { session: id })
        .await
        .unwrap();
    assert!(matches!(closed, ResponsePayload::Closed(true)));

    println!("             wire   : open/snapshot/click/extract/batch/close ok");
    let _ = SessionId::new("wire");
    Ok(())
}
