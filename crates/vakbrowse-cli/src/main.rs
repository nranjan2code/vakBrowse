use clap::{Parser, Subcommand};
use vakbrowse_core::SessionId;
use vakbrowse_server::{Action, Request, ResponsePayload, SessionOptions, uds};

#[derive(Parser)]
#[command(
    name = "vak",
    version,
    about = "vakBrowse CLI: drive the agent-native browser from a shell"
)]
struct Cli {
    /// Daemon socket.
    #[arg(long, global = true, default_value = "/tmp/vakd.sock")]
    socket: String,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Open a new browser session, optionally navigating immediately.
    Open {
        url: Option<String>,
        /// Persistent profile id (keeps cookies/storage across runs).
        #[arg(long)]
        profile: Option<String>,
        /// Show the browser window (default: headless).
        #[arg(long)]
        headed: bool,
        /// Launch with a stealth fingerprint (seed defaults to the profile
        /// id or "default").
        #[arg(long)]
        stealth: bool,
        /// Proxy for this session (http://user:pass@host:port | socks5://host:port).
        #[arg(long)]
        proxy: Option<String>,
        /// Proxy rotation pool (comma-separated: a,b,c). `rotate-proxy` cycles
        /// these endpoints to escape IP-reputation walls honestly.
        #[arg(long, value_delimiter = ',')]
        proxies: Vec<String>,
        /// Inject small randomized delays before input actions so the agent's
        /// timing isn't robotic (cadence-based behavioral tell).
        #[arg(long)]
        human_timing: bool,
        /// Engine backend: `cdp` (Chrome) or `dom` (experimental pure-Rust
        /// QuickJS backend, no Chrome process). Omit to use the daemon's default
        /// (`VAKBROWSE_BACKEND`, CDP unless overridden).
        #[arg(long)]
        backend: Option<String>,
    },
    /// Close a session.
    Close { session: String },
    /// List live sessions on the daemon.
    Sessions,
    /// Navigate a session to a URL and print title/url.
    Navigate { session: String, url: String },
    /// Print an a11y snapshot with @eN refs (the agent's eyes).
    Snapshot { session: String },
    /// Click an element by ref (e.g. @e3).
    Click { session: String, r#ref: String },
    /// Set an input's value by ref.
    Fill {
        session: String,
        r#ref: String,
        text: String,
    },
    /// Select an <option> value on a dropdown by ref.
    Select {
        session: String,
        r#ref: String,
        value: String,
    },
    /// Press a key ("Enter", "Tab", "Escape", ...).
    #[command(visible_alias = "press_key")]
    Key { session: String, key: String },
    /// Scroll the page (CSS pixels).
    Scroll {
        session: String,
        #[arg(default_value = "0")]
        dx: f64,
        dy: f64,
    },
    /// Evaluate JS and print the string result.
    Eval { session: String, expression: String },
    /// Resolve a CSS selector to stable `@eN` refs (clickable by `click`).
    Find { session: String, selector: String },
    /// Wait until an expression becomes truthy.
    Wait {
        session: String,
        expression: String,
        #[arg(long, default_value = "5000")]
        timeout_ms: u64,
    },
    /// Wait until location.href contains a substring (SPA-safe).
    #[command(visible_alias = "wait_url")]
    WaitUrl {
        session: String,
        pattern: String,
        #[arg(long, default_value = "5000")]
        timeout_ms: u64,
    },
    /// Capture a PNG screenshot (vision fallback).
    Shot {
        session: String,
        #[arg(long)]
        full: bool,
    },
    /// Click raw viewport coordinates (vision fallback).
    ClickAt { session: String, x: f64, y: f64 },
    /// List WebMCP tools declared by the page (if any).
    WebMcpTools { session: String },
    /// Invoke a page-declared WebMCP tool.
    WebMcpInvoke {
        session: String,
        name: String,
        #[arg(default_value = "{}")]
        arguments_json: String,
    },
    /// Go back one history entry.
    Back { session: String },
    /// Go forward one history entry.
    Forward { session: String },
    /// Reload the current document.
    Reload { session: String },
    /// Extract readable main-content text (title + url + body).
    Extract { session: String },
    /// List tabs of a session (active first).
    Tabs { session: String },
    /// Open a new tab (becomes active).
    NewTab {
        session: String,
        url: Option<String>,
    },
    /// Switch the active tab.
    Switch { session: String, tab: String },
    /// Close a tab.
    CloseTab { session: String, tab: String },
    /// Re-launch the browser on the next proxy in the session's pool
    /// (open with --proxies a,b). Honest anti-bot rotation.
    #[command(visible_alias = "rotate_proxy")]
    RotateProxy { session: String },
    /// Set files on a file input element (by @eN ref).
    SetFileChooser {
        session: String,
        r#ref: String,
        paths: Vec<String>,
    },
    /// Return the current page HTML source.
    Source { session: String },
    /// List completed downloads.
    Downloads { session: String },
    /// Run a batch of actions in one request (fail-fast on first error).
    /// `actions` is a JSON array of action objects, e.g.:
    /// '[{"type":"navigate","url":"https://example.com"},{"type":"extract"}]'
    Batch { session: String, actions: String },
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    let path = std::path::PathBuf::from(&cli.socket);

    let request = match to_request(cli.command) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("error: {e}");
            std::process::exit(2);
        }
    };

    match uds::call(&path, request).await {
        Ok(response) => match render(response) {
            Ok(text) => println!("{text}"),
            Err(code) => std::process::exit(code),
        },
        Err(e) => {
            eprintln!("error: {e}");
            std::process::exit(1);
        }
    }
}

fn to_request(cmd: Command) -> Result<Request, String> {
    Ok(match cmd {
        Command::Open {
            url,
            profile,
            headed,
            stealth,
            proxy,
            proxies,
            human_timing,
            backend,
        } => Request::Open {
            options: SessionOptions {
                profile: profile.clone().map(vakbrowse_core::ProfileId::new),
                headless: !headed,
                url,
                backend: match backend.as_deref() {
                    None => None,
                    Some("cdp") => Some(vakbrowse_server::Backend::Cdp),
                    Some("dom") => Some(vakbrowse_server::Backend::Dom),
                    Some(other) => {
                        return Err(format!("unrecognized backend `{other}` (use cdp|dom)"));
                    }
                },
                stealth_seed: stealth.then(|| profile.unwrap_or_else(|| "default".to_string())),
                proxy,
                proxies,
                human_timing,
            },
        },
        Command::Close { session } => Request::Close {
            session: SessionId(session),
        },
        Command::Sessions => Request::ListSessions,
        Command::Navigate { session, url } => act(session, Action::Navigate { url }),
        Command::Snapshot { session } => act(session, Action::Snapshot),
        Command::Click { session, r#ref } => act(session, Action::Click { r#ref }),
        Command::Fill {
            session,
            r#ref,
            text,
        } => act(session, Action::Fill { r#ref, text }),
        Command::Select {
            session,
            r#ref,
            value,
        } => act(session, Action::SelectOption { r#ref, value }),
        Command::Key { session, key } => act(session, Action::PressKey { key }),
        Command::Scroll { session, dx, dy } => act(session, Action::Scroll { dx, dy }),
        Command::Eval {
            session,
            expression,
        } => act(session, Action::EvalText { expression }),
        Command::Find { session, selector } => act(session, Action::FindByCss { selector }),
        Command::Wait {
            session,
            expression,
            timeout_ms,
        } => act(
            session,
            Action::WaitForTruthy {
                expression,
                timeout_ms,
            },
        ),
        Command::WaitUrl {
            session,
            pattern,
            timeout_ms,
        } => act(
            session,
            Action::WaitForUrl {
                pattern,
                timeout_ms,
            },
        ),
        Command::Shot { session, full } => act(session, Action::Screenshot { full_page: full }),
        Command::ClickAt { session, x, y } => act(session, Action::ClickAt { x, y }),
        Command::WebMcpTools { session } => act(session, Action::WebMcpTools),
        Command::WebMcpInvoke {
            session,
            name,
            arguments_json,
        } => act(
            session,
            Action::WebMcpInvoke {
                name,
                arguments_json,
            },
        ),
        Command::Back { session } => act(session, Action::Back),
        Command::Forward { session } => act(session, Action::Forward),
        Command::Reload { session } => act(session, Action::Reload),
        Command::Extract { session } => act(session, Action::Extract),
        Command::Tabs { session } => act(session, Action::Tabs),
        Command::NewTab { session, url } => act(session, Action::NewTab { url }),
        Command::Switch { session, tab } => act(
            session,
            Action::SwitchTab {
                tab: vakbrowse_core::TabId(tab),
            },
        ),
        Command::CloseTab { session, tab } => act(
            session,
            Action::CloseTab {
                tab: vakbrowse_core::TabId(tab),
            },
        ),
        Command::RotateProxy { session } => act(session, Action::RotateProxy),
        Command::SetFileChooser {
            session, r#ref, paths,
        } => act(
            session,
            Action::SetFileChooser {
                r#ref,
                paths,
            },
        ),
        Command::Source { session } => act(session, Action::Source),
        Command::Downloads { session } => act(session, Action::Downloads),
        Command::Batch { session, actions } => {
            let acts: Vec<Action> = serde_json::from_str(&actions)
                .map_err(|e| format!("batch actions JSON parse error: {e}"))?;
            if acts.is_empty() {
                return Err("batch actions must be non-empty".into());
            }
            Request::Batch {
                session: SessionId(session),
                actions: acts,
            }
        }
    })
}

fn act(session: String, action: Action) -> Request {
    Request::Act {
        session: SessionId(session),
        action,
    }
}

fn render(response: vakbrowse_server::Response) -> Result<String, i32> {
    match response {
        Ok(vakbrowse_server::ResponsePayload::Error(e)) => {
            eprintln!("error: {}", e);
            Err(1)
        }
        Ok(payload) => Ok(render_payload(payload)),
        Err(err) => {
            eprintln!("error: {err}");
            Err(1)
        }
    }
}

fn render_payload(p: ResponsePayload) -> String {
    match p {
        ResponsePayload::Opened(info) => {
            format!("session {} open ({})", info.id, info.url)
        }
        ResponsePayload::Closed(removed) => {
            if removed {
                "closed".into()
            } else {
                "no such session".into()
            }
        }
        ResponsePayload::Sessions(sessions) => {
            if sessions.is_empty() {
                return "(no sessions)".into();
            }
            sessions
                .iter()
                .map(|s| {
                    let profile = s
                        .profile
                        .as_ref()
                        .map(|p| p.0.clone())
                        .unwrap_or_else(|| "-".into());
                    format!("{} profile={} {}", s.id, profile, s.url)
                })
                .collect::<Vec<_>>()
                .join("\n")
        }
        ResponsePayload::Result(action) => format_action_result(action),
        ResponsePayload::Results(results) => results
            .iter()
            .enumerate()
            .map(|(i, a)| format!("[{i}] {}", format_action_result(a.clone())))
            .collect::<Vec<_>>()
            .join("\n"),
        ResponsePayload::Error(e) => format!("error: {e}"),
    }
}

/// Render one `ActionResult` to CLI text (shared by single-result and batch).
fn format_action_result(action: vakbrowse_server::ActionResult) -> String {
    match action {
        vakbrowse_server::ActionResult::Navigated { url, title } => format!("{title}\n{url}"),
        vakbrowse_server::ActionResult::Snapshot { snapshot } => {
            vakbrowse_server::render::snapshot_text(&snapshot)
        }
        vakbrowse_server::ActionResult::Text { text } => text,
        vakbrowse_server::ActionResult::Elements { refs } => refs
            .iter()
            .map(|r| r.0.clone())
            .collect::<Vec<_>>()
            .join("\n"),
        vakbrowse_server::ActionResult::Flag { ok: true } => "ok".into(),
        vakbrowse_server::ActionResult::Flag { ok: false } => "not applied".into(),
        vakbrowse_server::ActionResult::Cookies { cookies } => {
            serde_json::to_string_pretty(&cookies).unwrap_or_else(|_| "[]".into())
        }
        vakbrowse_server::ActionResult::Tabs { tabs } => tabs
            .iter()
            .map(|t| format!("{}\t{}", t.id, t.url))
            .collect::<Vec<_>>()
            .join("\n"),
        vakbrowse_server::ActionResult::TabOpened { tab } => {
            format!("tab {} open ({})", tab.id, tab.url)
        }
        vakbrowse_server::ActionResult::Done => "done".into(),
        vakbrowse_server::ActionResult::Clicked { navigated, url } => {
            if navigated {
                format!("navigated to {}", url.as_deref().unwrap_or(""))
            } else {
                "no navigation (possible bot wall / JS-handler click)".into()
            }
        }
        vakbrowse_server::ActionResult::Image { png_base64 } => save_screenshot(&png_base64),
        vakbrowse_server::ActionResult::Tools { tools } => {
            if tools.is_empty() {
                "(no WebMCP tools on this page)".into()
            } else {
                tools
                    .iter()
                    .map(|t| format!("{}\t{}", t.name, t.description))
                    .collect::<Vec<_>>()
                    .join("\n")
            }
        }
    }
}

fn save_screenshot(png_base64: &str) -> String {
    use base64::Engine as _;
    let png = match base64::engine::general_purpose::STANDARD.decode(png_base64) {
        Ok(bytes) => bytes,
        Err(e) => return format!("error decoding screenshot: {e}"),
    };
    for n in 0..10_000 {
        let path = std::path::PathBuf::from(format!("vak-shot-{n}.png"));
        if path.exists() {
            continue;
        }
        match std::fs::write(&path, &png) {
            Ok(_) => return format!("saved {}", path.display()),
            Err(e) => return format!("error writing {}: {e}", path.display()),
        }
    }
    "error: no free vak-shot-N.png slot".into()
}
