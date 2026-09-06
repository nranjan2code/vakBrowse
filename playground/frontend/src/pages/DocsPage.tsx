import React, { useState } from 'react';
import type { PageTab } from '../components/Header';

interface DocsPageProps {
  onNavigate: (tab: PageTab) => void;
}

type SdkTab = 'python' | 'cli' | 'mcp' | 'rest' | 'rust';

export function DocsPage({ onNavigate }: DocsPageProps) {
  const [activeSdk, setActiveSdk] = useState<SdkTab>('python');

  const sdkTabs: { id: SdkTab; label: string; badge: string }[] = [
    { id: 'python', label: 'PYTHON SDK', badge: 'pip install' },
    { id: 'cli', label: 'CLI & DAEMON', badge: 'cargo / binary' },
    { id: 'mcp', label: 'MCP SERVER', badge: 'Claude / Cursor' },
    { id: 'rest', label: 'REST & WS', badge: 'HTTP :7788' },
    { id: 'rust', label: 'RUST CRATE', badge: 'cargo add' },
  ];

  return (
    <div className="max-w-7xl mx-auto px-4 sm:px-6 py-10 space-y-12">
      {/* Header */}
      <div className="space-y-3">
        <div className="badge-dim">DEVELOPER MANUAL // INTEGRATION</div>
        <h1 className="text-3xl sm:text-4xl font-bold font-sans text-bone">
          Developer Quickstart & SDK Reference
        </h1>
        <p className="text-sm sm:text-base text-text-dim font-sans max-w-3xl">
          Integrate vakBrowse into your agent framework in under 2 minutes. Choose your surface: embedded Python library, standard stdio MCP server, CLI client, or HTTP REST daemon.
        </p>
      </div>

      {/* Surface Selector Tabs */}
      <div className="flex items-center gap-2 overflow-x-auto pb-2 border-b border-border">
        {sdkTabs.map((tab) => (
          <button
            key={tab.id}
            onClick={() => setActiveSdk(tab.id)}
            className={`px-3 py-2 rounded-xs font-mono text-xs tracking-wider transition-all flex items-center gap-2 ${
              activeSdk === tab.id
                ? 'bg-accent text-white font-bold shadow-te-button'
                : 'bg-card border border-border text-text-dim hover:text-text hover:bg-surface'
            }`}
          >
            <span>{tab.label}</span>
            <span className="text-[10px] opacity-70">({tab.badge})</span>
          </button>
        ))}
      </div>

      {/* Surface Content Area */}
      <div className="te-panel rounded-xs border-border p-6 md:p-8 space-y-8">
        {/* Python SDK */}
        {activeSdk === 'python' && (
          <div className="space-y-6">
            <div className="flex items-center justify-between pb-4 border-b border-border">
              <div>
                <h3 className="text-xl font-bold font-sans text-bone">
                  Python SDK (`vakbrowse`)
                </h3>
                <p className="text-xs text-text-dim font-sans mt-0.5">
                  Ships a bundled native cdylib with an embedded Tokio runtime. No daemon process required.
                </p>
              </div>
              <span className="badge-green">EMBEDDED C-FFI</span>
            </div>

            <div className="space-y-2">
              <div className="text-xs font-mono text-text-dim uppercase">01 // INSTALLATION</div>
              <pre className="p-3 bg-bg border border-border rounded-xs text-xs font-mono text-emerald-400">
pip install vakbrowse
              </pre>
            </div>

            <div className="space-y-2">
              <div className="text-xs font-mono text-text-dim uppercase">02 // AGENT RESEARCH SCRIPT</div>
              <pre className="p-4 bg-bg border border-border rounded-xs text-xs font-mono text-text overflow-x-auto leading-relaxed">
{`from vakbrowse import Session

# Initialize session manager (owns its background runtime)
session = Session()

# Open URL with optional stealth & proxy
sid, url = session.open("https://en.wikipedia.org/wiki/Artificial_intelligence", stealth=True)
print(f"Session {sid} opened at {url}")

# Take a compact accessibility snapshot (<500 tokens)
snapshot = session.snapshot(sid)
for el in snapshot["elements"][:5]:
    print(f"[{el['ref']}] {el['role'].upper()}: {el.get('name')}")

# Extract readability-clean markdown text
text = session.extract(sid)["text"]
print(f"Extracted {len(text)} characters of clean article content")

# Batch multiple actions in a single atomic round-trip (fail-fast)
results = session.batch(sid, [
    {"type": "navigate", "url": "https://example.com"},
    {"type": "extract"}
])

# Close browser session
session.close(sid)`}
              </pre>
            </div>
          </div>
        )}

        {/* CLI & Daemon */}
        {activeSdk === 'cli' && (
          <div className="space-y-6">
            <div className="flex items-center justify-between pb-4 border-b border-border">
              <div>
                <h3 className="text-xl font-bold font-sans text-bone">
                  CLI (`vak`) & Daemon (`vakd`)
                </h3>
                <p className="text-xs text-text-dim font-sans mt-0.5">
                  High-speed Unix Domain Socket (UDS) daemon managing persistent headless browser sessions.
                </p>
              </div>
              <span className="badge-orange">UDS IPC</span>
            </div>

            <div className="space-y-2">
              <div className="text-xs font-mono text-text-dim uppercase">01 // START THE DAEMON</div>
              <pre className="p-3 bg-bg border border-border rounded-xs text-xs font-mono text-emerald-400">
# Start UDS daemon in background
vakd serve --socket /tmp/vakd.sock &
              </pre>
            </div>

            <div className="space-y-2">
              <div className="text-xs font-mono text-text-dim uppercase">02 // SCRIPTING WITH VAK CLI</div>
              <pre className="p-4 bg-bg border border-border rounded-xs text-xs font-mono text-text overflow-x-auto leading-relaxed">
{`# Open a target URL
vak --socket /tmp/vakd.sock open https://news.ycombinator.com

# Capture compact perception snapshot
vak --socket /tmp/vakd.sock snapshot s1

# Click an element by its stable @eN ref
vak --socket /tmp/vakd.sock click s1 @e1

# Fill a search box and submit with Enter
vak --socket /tmp/vakd.sock fill s1 @e2 "agent browser"
vak --socket /tmp/vakd.sock key s1 Enter

# Extract readable content
vak --socket /tmp/vakd.sock extract s1`}
              </pre>
            </div>
          </div>
        )}

        {/* MCP Server */}
        {activeSdk === 'mcp' && (
          <div className="space-y-6">
            <div className="flex items-center justify-between pb-4 border-b border-border">
              <div>
                <h3 className="text-xl font-bold font-sans text-bone">
                  Model Context Protocol (`vak-mcp`)
                </h3>
                <p className="text-xs text-text-dim font-sans mt-0.5">
                  34 browser automation tools over stdio. Built-in StdioFramer handles both NDJSON and Content-Length protocols.
                </p>
              </div>
              <span className="badge-yellow">34 TOOLS</span>
            </div>

            <div className="space-y-2">
              <div className="text-xs font-mono text-text-dim uppercase">01 // CLIENT CONFIGURATION</div>
              <pre className="p-4 bg-bg border border-border rounded-xs text-xs font-mono text-emerald-400 overflow-x-auto leading-relaxed">
{`// Claude Desktop: ~/Library/Application Support/Claude/claude_desktop_config.json
// Cursor: ~/.cursor/mcp.json
{
  "mcpServers": {
    "vakbrowse": {
      "command": "/Users/user/.cargo/bin/vak-mcp",
      "env": {
        "VAKBROWSE_ALLOW_PREFIXES": "https://en.wikipedia.org,https://github.com"
      }
    }
  }
}`}
              </pre>
            </div>

            <div className="space-y-2">
              <div className="text-xs font-mono text-text-dim uppercase">02 // AVAILABLE MCP TOOLS</div>
              <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-2 text-xs font-mono">
                <div className="p-2 bg-surface border border-border rounded-xs">
                  <span className="text-accent font-bold">browser_open</span>: Open session
                </div>
                <div className="p-2 bg-surface border border-border rounded-xs">
                  <span className="text-accent font-bold">browser_snapshot</span>: Accessibility graph
                </div>
                <div className="p-2 bg-surface border border-border rounded-xs">
                  <span className="text-accent font-bold">browser_click</span>: Box-model click
                </div>
                <div className="p-2 bg-surface border border-border rounded-xs">
                  <span className="text-accent font-bold">browser_fill</span>: Native input setter
                </div>
                <div className="p-2 bg-surface border border-border rounded-xs">
                  <span className="text-accent font-bold">browser_extract</span>: Readability text
                </div>
                <div className="p-2 bg-surface border border-border rounded-xs">
                  <span className="text-accent font-bold">browser_rotate_proxy</span>: Cycle IP
                </div>
              </div>
            </div>
          </div>
        )}

        {/* REST & WebSocket */}
        {activeSdk === 'rest' && (
          <div className="space-y-6">
            <div className="flex items-center justify-between pb-4 border-b border-border">
              <div>
                <h3 className="text-xl font-bold font-sans text-bone">
                  REST & WebSocket Server (`vakd-rest`)
                </h3>
                <p className="text-xs text-text-dim font-sans mt-0.5">
                  Single-binary HTTP daemon with unified JSON RPC and bidirectional WebSocket bridge.
                </p>
              </div>
              <span className="badge-dim">PORT 7788</span>
            </div>

            <div className="space-y-2">
              <div className="text-xs font-mono text-text-dim uppercase">01 // START REST SERVER</div>
              <pre className="p-3 bg-bg border border-border rounded-xs text-xs font-mono text-emerald-400">
cargo run -p vakd-rest
# Or run with the built-in Playground frontend:
cargo run -p vakd-rest --features playground
              </pre>
            </div>

            <div className="space-y-2">
              <div className="text-xs font-mono text-text-dim uppercase">02 // UNIFIED PLAYGROUND RPC</div>
              <pre className="p-4 bg-bg border border-border rounded-xs text-xs font-mono text-text overflow-x-auto leading-relaxed">
{`# Unified endpoint accepts the same Request model as the UDS daemon:
curl -X POST http://localhost:7788/playground/rpc \\
  -H "Content-Type: application/json" \\
  -d '{
    "type": "open",
    "options": {
      "url": "https://example.com",
      "stealth": true
    }
  }'`}
              </pre>
            </div>
          </div>
        )}

        {/* Rust Crate */}
        {activeSdk === 'rust' && (
          <div className="space-y-6">
            <div className="flex items-center justify-between pb-4 border-b border-border">
              <div>
                <h3 className="text-xl font-bold font-sans text-bone">
                  Rust Native Crates
                </h3>
                <p className="text-xs text-text-dim font-sans mt-0.5">
                  Direct integration via `vakbrowse-engine` and `vakbrowse-server` crates for custom Rust applications.
                </p>
              </div>
              <span className="badge-orange">CARGO.TOML</span>
            </div>

            <div className="space-y-2">
              <div className="text-xs font-mono text-text-dim uppercase">01 // CARGO DEPENDENCY</div>
              <pre className="p-3 bg-bg border border-border rounded-xs text-xs font-mono text-emerald-400">
vakbrowse-server = &#123; path = "crates/vakbrowse-server" &#125;
              </pre>
            </div>

            <div className="space-y-2">
              <div className="text-xs font-mono text-text-dim uppercase">02 // RUST SESSION INVOCATION</div>
              <pre className="p-4 bg-bg border border-border rounded-xs text-xs font-mono text-text overflow-x-auto leading-relaxed">
{`use vakbrowse_server::{SessionManager, Policy, Request, SessionOptions};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let manager = SessionManager::with_policy(Policy::default());
    
    let res = manager.handle(Request::Open {
        options: SessionOptions {
            url: "https://example.com".into(),
            stealth: true,
            ..Default::default()
        }
    }).await?;
    
    println!("Session opened: {:?}", res);
    Ok(())
}`}
              </pre>
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
