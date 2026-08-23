use clap::{Parser, Subcommand};
use std::sync::Arc;
use vakbrowse_engine::cft;
use vakbrowse_engine::{CdpLauncher, EngineLauncher, LaunchOptions};
use vakbrowse_server::{Policy, Request, SessionManager};

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
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info".into()),
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
    let manager = Arc::new(SessionManager::new(policy).with_pool(pool));
    manager.spawn_reaper();
    let path = std::path::PathBuf::from(socket);
    // Ctrl-C kills the process; the socket file is removed on next start.
    vakbrowse_server::uds::serve(&path, manager).await
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
                println!("{} profile={:?} url={}", s.id, s.profile.as_ref().map(|p| p.0.clone()), s.url);
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
        println!("  probe    : ok");
    }

    println!("all checks passed");
    Ok(())
}
