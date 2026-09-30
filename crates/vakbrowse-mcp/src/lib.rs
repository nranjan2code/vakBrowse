//! MCP surface for vakBrowse. Speaks stdio so any MCP client
//! (Claude Code/Cursor/opencode/…) can drive a real browser.
//!
//! Tool logic lives in `VakMcp::tool_call`, free of protocol plumbing, so
//! it is directly unit-testable without a transport.
pub mod stdio_framer;

use std::sync::Arc;

use rmcp::{
    ErrorData as McpError, ServerHandler,
    model::{
        CallToolRequestParams, CallToolResponse, CallToolResult, ContentBlock, Implementation,
        JsonObject, ListToolsResult, PaginatedRequestParams, ProtocolVersion, ServerCapabilities,
        ServerInfo, Tool,
    },
    service::{RequestContext, RoleServer},
};
use serde_json::{Value, json};
use vakbrowse_core::{ProfileId, SessionId};
use vakbrowse_server::{Action, ActionResult, Policy, Request, ResponsePayload, SessionManager};

fn schema(props: Vec<(&str, Value)>, required: &[&str]) -> JsonObject {
    let mut properties = serde_json::Map::new();
    for (name, def) in props {
        properties.insert(name.to_string(), def);
    }
    let mut obj = serde_json::Map::new();
    obj.insert("type".into(), json!("object"));
    obj.insert("properties".into(), Value::Object(properties));
    if !required.is_empty() {
        obj.insert(
            "required".into(),
            Value::Array(required.iter().map(|r| json!(r)).collect()),
        );
    }
    obj
}

const SESSION: &str = "string session id from browser_open/browser_sessions";
const REF: &str = "string element ref like @e3 from browser_snapshot";

pub(crate) fn tool_definitions() -> Vec<Tool> {
    vec![
        Tool::new(
            "browser_open",
            "Open a new browser session, optionally navigating to a URL. Returns a session id.",
            schema(
                vec![
                    (
                        "url",
                        json!({"type": "string", "description": "initial URL"}),
                    ),
                    (
                        "profile",
                        json!({"type": "string", "description": "persistent profile id; keeps cookies across calls"}),
                    ),
                    (
                        "headed",
                        json!({"type": "boolean", "description": "show window (default false)"}),
                    ),
                    (
                        "stealth",
                        json!({"type": "boolean", "description": "launch with a stealth fingerprint for bot-walled sites"}),
                    ),
                    (
                        "stealth_seed",
                        json!({"type": "string", "description": "deterministic fingerprint seed for stealth mode (defaults to the profile id or 'default'); pass a distinct string per identity/agent so MCP sessions don't all share the same fingerprint"}),
                    ),
                    (
                        "proxy",
                        json!({"type": "string", "description": "proxy for this session, e.g. http://user:pass@host:port or socks5://host:port"}),
                    ),
                    (
                        "proxies",
                        json!({"type":"array","items":{"type":"string"},"description":"proxy rotation pool (2+ endpoints); rotate-proxy cycles them to escape IP-reputation walls"}),
                    ),
                    (
                        "human_timing",
                        json!({"type":"boolean","description":"inject randomized delays before input actions to mask robotic cadence"}),
                    ),
                ],
                &[],
            ),
        ),
        Tool::new(
            "browser_sessions",
            "List live browser sessions.",
            schema(vec![], &[]),
        ),
        Tool::new(
            "browser_close",
            "Close a browser session and its browser process.",
            schema(
                vec![("session", json!({"type": "string", "description": SESSION}))],
                &["session"],
            ),
        ),
        Tool::new(
            "browser_rotate_proxy",
            "Re-launch the browser on the next proxy in the session's pool (open \
             with proxies=[a,b,…]). Honest IP-reputation rotation — NOT TLS \
             spoofing. Requires a pool of 2+ endpoints.",
            schema(
                vec![("session", json!({"type": "string", "description": SESSION}))],
                &["session"],
            ),
        ),
        Tool::new(
            "browser_navigate",
            "Navigate the session to a URL.",
            schema(
                vec![
                    ("session", json!({"type": "string", "description": SESSION})),
                    ("url", json!({"type": "string"})),
                ],
                &["session", "url"],
            ),
        ),
        Tool::new(
            "browser_snapshot",
            "Accessibility snapshot with stable @eN refs. This is how you SEE the page.",
            schema(
                vec![("session", json!({"type": "string", "description": SESSION}))],
                &["session"],
            ),
        ),
        Tool::new(
            "browser_click",
            "Click an element by @eN ref from the last snapshot.",
            schema(
                vec![
                    ("session", json!({"type": "string", "description": SESSION})),
                    ("ref", json!({"type": "string", "description": REF})),
                ],
                &["session", "ref"],
            ),
        ),
        Tool::new(
            "browser_fill",
            "Set an input/textarea value by ref (fires input+change).",
            schema(
                vec![
                    ("session", json!({"type": "string", "description": SESSION})),
                    ("ref", json!({"type": "string", "description": REF})),
                    ("text", json!({"type": "string"})),
                ],
                &["session", "ref", "text"],
            ),
        ),
        Tool::new(
            "browser_select",
            "Select an <option> value on a dropdown by ref.",
            schema(
                vec![
                    ("session", json!({"type": "string", "description": SESSION})),
                    ("ref", json!({"type": "string", "description": REF})),
                    ("value", json!({"type": "string"})),
                ],
                &["session", "ref", "value"],
            ),
        ),
        Tool::new(
            "browser_press_key",
            "Press Enter/Tab/Escape/arrows or a single character.",
            schema(
                vec![
                    ("session", json!({"type": "string", "description": SESSION})),
                    ("key", json!({"type": "string"})),
                ],
                &["session", "key"],
            ),
        ),
        Tool::new(
            "browser_scroll",
            "Scroll by CSS pixels.",
            schema(
                vec![
                    ("session", json!({"type": "string", "description": SESSION})),
                    ("dx", json!({"type": "number"})),
                    ("dy", json!({"type": "number"})),
                ],
                &["session"],
            ),
        ),
        Tool::new(
            "browser_eval",
            "Evaluate JavaScript and return the stringified result.",
            schema(
                vec![
                    ("session", json!({"type": "string", "description": SESSION})),
                    ("expression", json!({"type": "string"})),
                ],
                &["session", "expression"],
            ),
        ),
        Tool::new(
            "browser_find_element",
            "Resolve a CSS selector to stable @eN refs the page exposes for click/fill/select (no intermediate snapshot needed). Example selector: `a[href*=\"iana\"]` or `form input[type=\"text\"]`.",
            schema(
                vec![
                    ("session", json!({"type": "string", "description": SESSION})),
                    (
                        "selector",
                        json!({"type": "string", "description": "CSS selector (compound + descendant/child combinators + comma groups)"}),
                    ),
                ],
                &["session", "selector"],
            ),
        ),
        Tool::new(
            "browser_screenshot",
            "Capture a PNG screenshot of the page (vision fallback when the a11y tree has no refs).",
            schema(
                vec![
                    ("session", json!({"type": "string", "description": SESSION})),
                    ("full_page", json!({"type": "boolean"})),
                ],
                &["session"],
            ),
        ),
        Tool::new(
            "browser_click_at",
            "Click raw viewport coordinates (pair with browser_screenshot).",
            schema(
                vec![
                    ("session", json!({"type": "string", "description": SESSION})),
                    ("x", json!({"type": "number"})),
                    ("y", json!({"type": "number"})),
                ],
                &["session", "x", "y"],
            ),
        ),
        Tool::new(
            "browser_webmcp_tools",
            "List tools the page declares via WebMCP (navigator.modelContext), if any.",
            schema(
                vec![("session", json!({"type": "string", "description": SESSION}))],
                &["session"],
            ),
        ),
        Tool::new(
            "browser_webmcp_invoke",
            "Invoke a page-declared WebMCP tool by name.",
            schema(
                vec![
                    ("session", json!({"type": "string", "description": SESSION})),
                    ("name", json!({"type": "string"})),
                    ("arguments_json", json!({"type": "string"})),
                ],
                &["session", "name"],
            ),
        ),
        Tool::new(
            "browser_extract",
            "Extract the page's readable main content as text (token-cheap reading; prefer over eval).",
            schema(
                vec![("session", json!({"type": "string", "description": SESSION}))],
                &["session"],
            ),
        ),
        Tool::new(
            "browser_back",
            "Go back one history entry.",
            schema(
                vec![("session", json!({"type": "string", "description": SESSION}))],
                &["session"],
            ),
        ),
        Tool::new(
            "browser_forward",
            "Go forward one history entry.",
            schema(
                vec![("session", json!({"type": "string", "description": SESSION}))],
                &["session"],
            ),
        ),
        Tool::new(
            "browser_reload",
            "Reload the current document.",
            schema(
                vec![("session", json!({"type": "string", "description": SESSION}))],
                &["session"],
            ),
        ),
        Tool::new(
            "browser_tabs",
            "List tabs of the session (active first).",
            schema(
                vec![("session", json!({"type": "string", "description": SESSION}))],
                &["session"],
            ),
        ),
        Tool::new(
            "browser_new_tab",
            "Open a new tab (optionally navigating) and make it active.",
            schema(
                vec![
                    ("session", json!({"type": "string", "description": SESSION})),
                    ("url", json!({"type": "string"})),
                ],
                &["session"],
            ),
        ),
        Tool::new(
            "browser_switch_tab",
            "Make an existing tab active (refs belong to tabs).",
            schema(
                vec![
                    ("session", json!({"type": "string", "description": SESSION})),
                    ("tab", json!({"type": "string"})),
                ],
                &["session", "tab"],
            ),
        ),
        Tool::new(
            "browser_close_tab",
            "Close a tab (the last one cannot be closed).",
            schema(
                vec![
                    ("session", json!({"type": "string", "description": SESSION})),
                    ("tab", json!({"type": "string"})),
                ],
                &["session", "tab"],
            ),
        ),
        Tool::new(
            "browser_wait",
            "Wait until a JS predicate becomes truthy.",
            schema(
                vec![
                    ("session", json!({"type": "string", "description": SESSION})),
                    ("expression", json!({"type": "string"})),
                    ("timeout_ms", json!({"type": "integer"})),
                ],
                &["session", "expression"],
            ),
        ),
        Tool::new(
            "browser_wait_url",
            "SPA-safe URL wait: poll location.href for a substring until it matches (or \
             time out). Use this for SPA navigations — wait on the URL, not \
             document.readyState, which stays 'complete' across client-side routes.",
            schema(
                vec![
                    ("session", json!({"type": "string", "description": SESSION})),
                    (
                        "pattern",
                        json!({"type": "string", "description": "substring to wait for in location.href"}),
                    ),
                    ("timeout_ms", json!({"type": "integer"})),
                ],
                &["session", "pattern"],
            ),
        ),
        Tool::new(
            "browser_batch",
            "Run a sequence of actions in one round-trip (fail-fast on the first \
             error). Pass `actions` as a JSON array of action objects, each with \
             a `type` key (e.g. {\"type\":\"navigate\",\"url\":...}, \
             {\"type\":\"extract\"}, {\"type\":\"wait_url\",\"pattern\":...}). \
             Cuts agent latency: open+extract+close, fill+press+wait, etc., \
             as a single request.",
            schema(
                vec![
                    ("session", json!({"type": "string", "description": SESSION})),
                    (
                        "actions",
                        json!({
                            "type": "array",
                            "items": {"type": "object"},
                            "description": "ordered list of Action objects (see Action enum)"
                        }),
                    ),
                ],
                &["session", "actions"],
            ),
        ),
        Tool::new(
            "browser_set_file_chooser",
            "Set files on a file input element (by @eN ref) — bypasses browser \
             security that blocks programmatic .files assignment.",
            schema(
                vec![
                    ("session", json!({"type": "string", "description": SESSION})),
                    ("ref", json!({"type": "string", "description": "element ref like @e42"})),
                    (
                        "paths",
                        json!({
                            "type": "array",
                            "items": {"type": "string"},
                            "description": "absolute file paths to set"
                        }),
                    ),
                ],
                &["session", "ref", "paths"],
            ),
        ),
        Tool::new(
            "browser_source",
            "Return the current page HTML source (document.documentElement.outerHTML). \
             Useful when the a11y snapshot loses details (canvas, collapsed elements, \
             content behind CSP).",
            schema(vec![("session", json!({"type": "string", "description": SESSION}))], &["session"]),
        ),
        Tool::new(
            "browser_downloads",
            "List completed downloads in the session's download directory.",
            schema(vec![("session", json!({"type": "string", "description": SESSION}))], &["session"]),
        ),
        Tool::new(
            "browser_cookies",
            "List cookies stored in the session's browser context.",
            schema(vec![("session", json!({"type": "string", "description": SESSION}))], &["session"]),
        ),
        Tool::new(
            "browser_set_cookie",
            "Set a cookie in the session's browser context.",
            schema(
                vec![
                    ("session", json!({"type": "string", "description": SESSION})),
                    ("name", json!({"type": "string", "description": "cookie name"})),
                    ("value", json!({"type": "string", "description": "cookie value"})),
                    ("domain", json!({"type": "string", "description": "cookie domain"})),
                    ("path", json!({"type": "string", "description": "cookie path (default /)"})),
                    ("secure", json!({"type": "boolean", "description": "secure flag"})),
                    ("http_only", json!({"type": "boolean", "description": "httpOnly flag"})),
                    ("same_site", json!({"type": "string", "description": "Strict, Lax, or None"})),
                ],
                &["session", "name", "value", "domain"],
            ),
        ),
        Tool::new(
            "browser_clear_cookies",
            "Clear all cookies stored in the session's browser context.",
            schema(vec![("session", json!({"type": "string", "description": SESSION}))], &["session"]),
        ),
        Tool::new(
            "browser_set_download_dir",
            "Set the download directory path for the session.",
            schema(
                vec![
                    ("session", json!({"type": "string", "description": SESSION})),
                    ("dir", json!({"type": "string", "description": "directory path"})),
                ],
                &["session", "dir"],
            ),
        ),
    ]
}

fn arg<'a>(args: Option<&'a JsonObject>, key: &str) -> Option<&'a Value> {
    args?.get(key)
}

fn arg_str(args: Option<&JsonObject>, key: &str) -> Result<String, McpError> {
    arg(args, key)
        .and_then(|v| v.as_str())
        .map(str::to_string)
        .ok_or_else(|| McpError::invalid_params(format!("missing string param '{key}'"), None))
}

/// The MCP server. Owns its own SessionManager (in-process).
pub struct VakMcp {
    manager: Arc<SessionManager>,
}

impl Default for VakMcp {
    fn default() -> Self {
        Self::new(Policy::default())
    }
}

impl VakMcp {
    pub fn new(policy: Policy) -> Self {
        Self {
            manager: Arc::new(SessionManager::with_policy(policy)),
        }
    }

    /// Shared handle to the session manager (for graceful shutdown).
    pub fn manager(&self) -> Arc<SessionManager> {
        Arc::clone(&self.manager)
    }

    /// Transport-free tool dispatch; `call_tool` delegates here.
    pub async fn tool_call(
        &self,
        name: &str,
        args: Option<&JsonObject>,
    ) -> Result<CallToolResult, McpError> {
        let request = self.map_tool(name, args)?;
        let response = self.manager.handle(request).await;
        match response {
            // Transport-level panic that escaped the handler (UDS path only;
            // the in-process MCP path surfaces app errors as Ok(Error)).
            Err(err) => Err(McpError::internal_error(err, None)),
            Ok(ResponsePayload::Error(e)) => Ok(CallToolResult::error(vec![ContentBlock::text(
                format!("{e}"),
            )])),
            // Vision fallback: hand the agent the raw pixels, not a placeholder
            // string, so screenshot-driven click_at loops actually work.
            Ok(ResponsePayload::Result(ActionResult::Image { png_base64 })) => Ok(
                CallToolResult::success(vec![ContentBlock::image(png_base64.clone(), "image/png")]),
            ),
            Ok(payload) => Ok(CallToolResult::success(vec![ContentBlock::text(
                render_payload(&payload),
            )])),
        }
    }

    fn map_tool(&self, name: &str, args: Option<&JsonObject>) -> Result<Request, McpError> {
        let request = match name {
            "browser_open" => Request::Open {
                options: vakbrowse_server::SessionOptions {
                    profile: arg(args, "profile")
                        .and_then(|v| v.as_str())
                        .map(ProfileId::new),
                    headless: !arg(args, "headed")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false),
                    url: arg(args, "url").and_then(|v| v.as_str()).map(String::from),
                    stealth_seed: {
                        // Prefer an explicit seed; fall back to the profile id
                        // (matches CLI behavior), then a default. Previously
                        // this was hardcoded to "mcp" — every MCP stealth
                        // session got the *same* fingerprint.
                        if arg(args, "stealth")
                            .and_then(|v| v.as_bool())
                            .unwrap_or(false)
                        {
                            arg(args, "stealth_seed")
                                .and_then(|v| v.as_str())
                                .map(String::from)
                                .or_else(|| {
                                    arg(args, "profile")
                                        .and_then(|v| v.as_str())
                                        .map(String::from)
                                })
                                .or_else(|| Some("default".to_string()))
                        } else {
                            None
                        }
                    },
                    proxy: arg(args, "proxy")
                        .and_then(|v| v.as_str())
                        .map(String::from),
                    proxies: arg(args, "proxies")
                        .and_then(|v| v.as_array())
                        .map(|arr| {
                            arr.iter()
                                .filter_map(|v| v.as_str().map(String::from))
                                .collect()
                        })
                        .unwrap_or_default(),
                    human_timing: arg(args, "human_timing")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false),
                },
            },
            "browser_close" => Request::Close {
                session: SessionId(arg_str(args, "session")?),
            },
            "browser_rotate_proxy" => Request::Act {
                session: SessionId(arg_str(args, "session")?),
                action: Action::RotateProxy,
            },
            "browser_sessions" => Request::ListSessions,
            "browser_navigate" => Request::Act {
                session: SessionId(arg_str(args, "session")?),
                action: Action::Navigate {
                    url: arg_str(args, "url")?,
                },
            },
            "browser_snapshot" => Request::Act {
                session: SessionId(arg_str(args, "session")?),
                action: Action::Snapshot,
            },
            "browser_click" => Request::Act {
                session: SessionId(arg_str(args, "session")?),
                action: Action::Click {
                    r#ref: arg_str(args, "ref")?,
                },
            },
            "browser_fill" => Request::Act {
                session: SessionId(arg_str(args, "session")?),
                action: Action::Fill {
                    r#ref: arg_str(args, "ref")?,
                    text: arg_str(args, "text")?,
                },
            },
            "browser_select" => Request::Act {
                session: SessionId(arg_str(args, "session")?),
                action: Action::SelectOption {
                    r#ref: arg_str(args, "ref")?,
                    value: arg_str(args, "value")?,
                },
            },
            "browser_press_key" => Request::Act {
                session: SessionId(arg_str(args, "session")?),
                action: Action::PressKey {
                    key: arg_str(args, "key")?,
                },
            },
            "browser_scroll" => Request::Act {
                session: SessionId(arg_str(args, "session")?),
                action: Action::Scroll {
                    dx: arg(args, "dx").and_then(|v| v.as_f64()).unwrap_or(0.0),
                    dy: arg(args, "dy").and_then(|v| v.as_f64()).unwrap_or(0.0),
                },
            },
            "browser_eval" => Request::Act {
                session: SessionId(arg_str(args, "session")?),
                action: Action::EvalText {
                    expression: arg_str(args, "expression")?,
                },
            },
            "browser_find_element" => Request::Act {
                session: SessionId(arg_str(args, "session")?),
                action: Action::FindByCss {
                    selector: arg_str(args, "selector")?,
                },
            },
            "browser_screenshot" => Request::Act {
                session: SessionId(arg_str(args, "session")?),
                action: Action::Screenshot {
                    full_page: arg(args, "full_page")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false),
                },
            },
            "browser_click_at" => Request::Act {
                session: SessionId(arg_str(args, "session")?),
                action: Action::ClickAt {
                    x: arg(args, "x").and_then(|v| v.as_f64()).unwrap_or(0.0),
                    y: arg(args, "y").and_then(|v| v.as_f64()).unwrap_or(0.0),
                },
            },
            "browser_webmcp_tools" => Request::Act {
                session: SessionId(arg_str(args, "session")?),
                action: Action::WebMcpTools,
            },
            "browser_webmcp_invoke" => Request::Act {
                session: SessionId(arg_str(args, "session")?),
                action: Action::WebMcpInvoke {
                    name: arg_str(args, "name")?,
                    arguments_json: arg(args, "arguments_json")
                        .and_then(|v| v.as_str())
                        .unwrap_or("{}")
                        .to_string(),
                },
            },
            "browser_extract" => Request::Act {
                session: SessionId(arg_str(args, "session")?),
                action: Action::Extract,
            },
            "browser_back" => Request::Act {
                session: SessionId(arg_str(args, "session")?),
                action: Action::Back,
            },
            "browser_forward" => Request::Act {
                session: SessionId(arg_str(args, "session")?),
                action: Action::Forward,
            },
            "browser_reload" => Request::Act {
                session: SessionId(arg_str(args, "session")?),
                action: Action::Reload,
            },
            "browser_tabs" => Request::Act {
                session: SessionId(arg_str(args, "session")?),
                action: Action::Tabs,
            },
            "browser_new_tab" => Request::Act {
                session: SessionId(arg_str(args, "session")?),
                action: Action::NewTab {
                    url: arg(args, "url").and_then(|v| v.as_str()).map(String::from),
                },
            },
            "browser_switch_tab" => Request::Act {
                session: SessionId(arg_str(args, "session")?),
                action: Action::SwitchTab {
                    tab: vakbrowse_core::TabId(arg_str(args, "tab")?),
                },
            },
            "browser_close_tab" => Request::Act {
                session: SessionId(arg_str(args, "session")?),
                action: Action::CloseTab {
                    tab: vakbrowse_core::TabId(arg_str(args, "tab")?),
                },
            },
            "browser_wait" => Request::Act {
                session: SessionId(arg_str(args, "session")?),
                action: Action::WaitForTruthy {
                    expression: arg_str(args, "expression")?,
                    timeout_ms: arg(args, "timeout_ms")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(5_000),
                },
            },
            "browser_wait_url" => Request::Act {
                session: SessionId(arg_str(args, "session")?),
                action: Action::WaitForUrl {
                    pattern: arg_str(args, "pattern")?,
                    timeout_ms: arg(args, "timeout_ms")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(5_000),
                },
            },
            "browser_set_file_chooser" => {
                let paths = arg(args, "paths")
                    .and_then(|v| v.as_array())
                    .ok_or_else(|| {
                        McpError::invalid_params("browser_set_file_chooser requires 'paths'", None)
                    })?
                    .iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect::<Vec<_>>();
                if paths.is_empty() {
                    return Err(McpError::invalid_params(
                        "browser_set_file_chooser 'paths' must be non-empty".to_string(),
                        None,
                    ));
                }
                Request::Act {
                    session: SessionId(arg_str(args, "session")?),
                    action: Action::SetFileChooser {
                        r#ref: arg_str(args, "ref")?,
                        paths,
                    },
                }
            }
            "browser_source" => Request::Act {
                session: SessionId(arg_str(args, "session")?),
                action: Action::Source,
            },
            "browser_downloads" => Request::Act {
                session: SessionId(arg_str(args, "session")?),
                action: Action::Downloads,
            },
            "browser_cookies" => Request::Act {
                session: SessionId(arg_str(args, "session")?),
                action: Action::Cookies,
            },
            "browser_set_cookie" => {
                let name = arg_str(args, "name")?;
                let value = arg_str(args, "value")?;
                let domain = arg_str(args, "domain")?;
                let path = arg(args, "path").and_then(|v| v.as_str()).unwrap_or("/").to_string();
                let secure = arg(args, "secure").and_then(|v| v.as_bool()).unwrap_or(false);
                let http_only = arg(args, "http_only").and_then(|v| v.as_bool()).unwrap_or(false);
                let same_site = arg(args, "same_site").and_then(|v| v.as_str()).map(String::from);
                Request::Act {
                    session: SessionId(arg_str(args, "session")?),
                    action: Action::SetCookie {
                        cookie: vakbrowse_core::CookieInput {
                            name,
                            value,
                            domain,
                            path,
                            secure,
                            http_only,
                            same_site,
                        },
                    },
                }
            }
            "browser_clear_cookies" => Request::Act {
                session: SessionId(arg_str(args, "session")?),
                action: Action::ClearCookies,
            },
            "browser_set_download_dir" => Request::Act {
                session: SessionId(arg_str(args, "session")?),
                action: Action::SetDownloadDir {
                    dir: arg_str(args, "dir")?,
                },
            },
            "browser_batch" => {
                let actions_val = arg(args, "actions").ok_or_else(|| {
                    McpError::invalid_params("browser_batch requires 'actions'", None)
                })?;
                let actions: Vec<Action> = serde_json::from_value(actions_val.clone())
                    .map_err(|e| McpError::invalid_params(format!("actions: {e}"), None))?;
                if actions.is_empty() {
                    return Err(McpError::invalid_params(
                        "browser_batch 'actions' must be non-empty".to_string(),
                        None,
                    ));
                }
                Request::Batch {
                    session: SessionId(arg_str(args, "session")?),
                    actions,
                }
            }
            other => {
                return Err(McpError::invalid_params(
                    format!("unknown tool {other:?}"),
                    None,
                ));
            }
        };
        Ok(request)
    }
}

fn render_payload(p: &ResponsePayload) -> String {
    match p {
        ResponsePayload::Opened(info) => {
            format!("session {} open ({})", info.id, info.url)
        }
        ResponsePayload::Closed(removed) => {
            if *removed {
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
                .map(|s| format!("{} {}", s.id, s.url))
                .collect::<Vec<_>>()
                .join("\n")
        }
        ResponsePayload::Result(action) => fmt_action(action),
        ResponsePayload::Results(results) => results
            .iter()
            .enumerate()
            .map(|(i, a)| format!("[{i}] {}", fmt_action(a)))
            .collect::<Vec<_>>()
            .join("\n"),
        ResponsePayload::Error(e) => format!("error: {e}"),
    }
}

/// Render one `ActionResult` for MCP text (shared by single-result and batch).
fn fmt_action(a: &vakbrowse_server::ActionResult) -> String {
    match a {
        vakbrowse_server::ActionResult::Navigated { url, title } => {
            format!("navigated\n{title}\n{url}")
        }
        vakbrowse_server::ActionResult::Snapshot { snapshot } => {
            vakbrowse_server::render::snapshot_text(snapshot)
        }
        vakbrowse_server::ActionResult::Text { text } => text.clone(),
        vakbrowse_server::ActionResult::Elements { refs } => refs
            .iter()
            .map(|r| r.0.clone())
            .collect::<Vec<_>>()
            .join("\n"),
        vakbrowse_server::ActionResult::Flag { ok: true } => "ok".into(),
        vakbrowse_server::ActionResult::Flag { ok: false } => "not applied".into(),
        vakbrowse_server::ActionResult::Cookies { cookies } => {
            serde_json::to_string_pretty(cookies).unwrap_or_else(|_| "[]".into())
        }
        vakbrowse_server::ActionResult::Done => "done".into(),
        vakbrowse_server::ActionResult::Clicked { navigated, url } => {
            if *navigated {
                format!("navigated to {}", url.as_deref().unwrap_or(""))
            } else {
                "no navigation (possible bot wall / JS-handler click)".to_string()
            }
        }
        vakbrowse_server::ActionResult::Image { .. } => "(screenshot)".into(),
        vakbrowse_server::ActionResult::Tabs { tabs } => tabs
            .iter()
            .map(|t| format!("{} {}", t.id, t.url))
            .collect::<Vec<_>>()
            .join("\n"),
        vakbrowse_server::ActionResult::TabOpened { tab } => {
            format!("tab {} open ({})", tab.id, tab.url)
        }
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

impl ServerHandler for VakMcp {
    fn get_info(&self) -> ServerInfo {
        let mut info = ServerInfo::default();
        info.protocol_version = ProtocolVersion::LATEST;
        info.capabilities = ServerCapabilities::builder().enable_tools().build();
        let mut implementation = Implementation::new("vakBrowse", env!("CARGO_PKG_VERSION"));
        implementation.title = Some("vakBrowse agent-native browser".into());
        info.server_info = implementation;
        info.instructions = Some(
            "Drive a real headless browser. Workflow: browser_open -> browser_navigate \
             -> browser_snapshot (read @eN refs) -> act via browser_click/fill/select/press_key \
             -> browser_snapshot again to verify. Refs go stale after navigation."
                .into(),
        );
        info
    }

    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, McpError> {
        Ok(ListToolsResult {
            tools: tool_definitions(),
            ..Default::default()
        })
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, McpError> {
        let name = request.name;
        let arguments = request.arguments;
        let result = self.tool_call(name.as_ref(), arguments.as_ref()).await?;
        Ok(result.into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_definitions_have_unique_names() {
        let mut names: Vec<_> = tool_definitions()
            .into_iter()
            .map(|t| t.name.to_string())
            .collect();
        names.sort();
        let len = names.len();
        names.dedup();
        assert_eq!(names.len(), len, "duplicate tool names");
    }
}
