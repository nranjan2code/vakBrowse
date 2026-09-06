import React, { useState } from 'react';
import type { PageTab } from '../components/Header';

interface UseCasesPageProps {
  onNavigate: (tab: PageTab) => void;
}

interface UseCaseItem {
  id: string;
  code: string;
  badge: string;
  title: string;
  subtitle: string;
  problem: string;
  solution: string;
  metrics: { label: string; value: string }[];
  codeExample: {
    lang: string;
    snippet: string;
  };
}

export function UseCasesPage({ onNavigate }: UseCasesPageProps) {
  const [activeCase, setActiveCase] = useState<string>('research');

  const useCases: UseCaseItem[] = [
    {
      id: 'research',
      code: 'UC-01',
      badge: 'DEEP RESEARCH',
      title: 'Autonomous Research & Fact Extraction',
      subtitle: 'Multi-tab synthesis, citation following, and token-frugal content reading.',
      problem: 'Agent research loops that crawl 10 pages in Playwright blow through context limits (500,000+ tokens), costing dollars per query and inducing LLM hallucinations from irrelevant navbars, footers, and advertising scripts.',
      solution: 'vakBrowse uses readability-style extraction (`Action::Extract`) returning clean markdown summaries (20KB from a 500KB page), coupled with compact accessibility snapshots (<500 tokens) that allow the agent to click citations and follow links with stable `@eN` references.',
      metrics: [
        { label: 'TOKEN REDUCTION', value: '-96.4%' },
        { label: 'MEDIAN EXTRACT TIME', value: '38ms' },
        { label: 'MULTI-TAB CONTEXT', value: 'ISOLATED REFS' },
      ],
      codeExample: {
        lang: 'Python SDK',
        snippet: `from vakbrowse import Session

session = Session()
sid, url = session.open("https://en.wikipedia.org/wiki/Artificial_intelligence")

# 1. Compact perception snapshot (<500 tokens)
snapshot = session.snapshot(sid)
print(f"Interactive refs found: {len(snapshot['elements'])}")

# 2. Extract readability-clean markdown article text
content = session.extract(sid)
print(f"Title: {content['title']}")
print(f"Clean Text Length: {len(content['text'])} bytes")

# 3. Follow citation link @e42 seamlessly
session.click(sid, "@e42")
session.close(sid)`,
      },
    },
    {
      id: 'scraping',
      code: 'UC-02',
      badge: 'MARKET INTELLIGENCE',
      title: 'Anti-Bot Scraping & Proxy Rotation',
      subtitle: 'Stealth hardware profiles and proxy pool rotation with automatic session URL recovery.',
      problem: 'Commercial platforms detect headless Chrome through `navigator.webdriver`, missing plugins, and automated timing. Rotating proxies in Puppeteer drops the active session state, forcing painful re-authentications.',
      solution: 'vakBrowse strips `--enable-automation` via stealth profiles, injects realistic plugin/language biometrics, and features `Action::RotateProxy`: it re-launches Chrome on the next proxy endpoint and automatically re-navigates to the session\'s last URL.',
      metrics: [
        { label: 'BOT WALL EVASION', value: 'HARDENED STEALTH' },
        { label: 'PROXY ROTATION', value: 'URL-PRESERVING' },
        { label: 'ROUNDTRIP OVERHEAD', value: '<120ms' },
      ],
      codeExample: {
        lang: 'CLI / Daemon Protocol',
        snippet: `# Launch session with custom stealth profile and proxy pool
vak open https://store.example.com/prices \\
  --stealth \\
  --proxies "http://p1.net:8080,http://p2.net:8080"

# Scrape prices...
vak extract s1

# IP blocked or rate-limited? Rotate immediately:
# Re-launches on p2.net and restores https://store.example.com/prices
vak rotate-proxy s1`,
      },
    },
    {
      id: 'rpa',
      code: 'UC-03',
      badge: 'WEB-ACTION RPA',
      title: 'React-Safe Form Automation & Submissions',
      subtitle: 'Reliable form autofill, native input setters, and trusted mouse coordinate dispatch.',
      problem: 'Simulated JavaScript `element.value = "..."` clicks fail to trigger React or Vue internal state bindings, leaving form submit buttons disabled. Below-the-fold links miss mouse clicks because viewport coordinates are calculated before scrolling.',
      solution: 'vakBrowse fills elements by focusing the node and invoking the native property value setter followed by synthetic `input` and `change` events. Clicks dispatch real CDP `Input.dispatchMouseEvent` at box-model centers with automatic `scrollIntoView({block:\'center\'})`.',
      metrics: [
        { label: 'REACT/VUE FIDELITY', value: '100% DISPATCH' },
        { label: 'BELOW-FOLD HIT RATE', value: '100% AUTO-SCROLL' },
        { label: 'FILE UPLOADS', value: 'SET_FILE_CHOOSER' },
      ],
      codeExample: {
        lang: 'JSON RPC Batch',
        snippet: `// Dispatched atomically to POST /playground/rpc or /sessions/{sid}/batch
{
  "type": "batch",
  "session": "s1",
  "actions": [
    { "type": "fill", "ref": "@e2", "value": "Agent Enterprise Fleet" },
    { "type": "fill", "ref": "@e4", "value": "agent@vakbrowse.dev" },
    { "type": "press_key", "key": "Enter" },
    { "type": "wait_for_url", "pattern": "success", "timeout_ms": 5000 }
  ]
}`,
      },
    },
    {
      id: 'mcp',
      code: 'UC-04',
      badge: 'AI AGENT MCP',
      title: 'Native Tool Calling for Claude & Cursor',
      subtitle: 'Zero-config Model Context Protocol server exposing 34 specialized browser tools.',
      problem: 'Agent developers spend weeks wrapping Puppeteer into fragile custom JSON schemas, dealing with stdio delimiter mismatches between different MCP clients (Claude Code uses NDJSON, while some spec clients enforce Content-Length headers).',
      solution: 'vak-mcp includes a byte-level framing normalizer (`StdioFramer`) that accepts both NDJSON and Content-Length seamlessly. It exposes 34 high-level browser tools (`browser_open`, `browser_snapshot`, `browser_click`, `browser_extract`, `browser_rotate_proxy`).',
      metrics: [
        { label: 'EXPOSED TOOLS', value: '34 TOOLS' },
        { label: 'FRAMING PROTOCOLS', value: 'NDJSON + CONTENT-LENGTH' },
        { label: 'CLIENT SUPPORT', value: 'CLAUDE, CURSOR, OPENCODE' },
      ],
      codeExample: {
        lang: 'MCP Client Configuration',
        snippet: `// ~/.cursor/mcp.json or claude_desktop_config.json
{
  "mcpServers": {
    "vakbrowse": {
      "command": "/usr/local/bin/vak-mcp",
      "env": {
        "VAKBROWSE_ALLOW_PREFIXES": "https://example.com,https://wikipedia.org"
      }
    }
  }
}`,
      },
    },
    {
      id: 'testing',
      code: 'UC-05',
      badge: 'SYNTHETIC QA',
      title: 'Zero-Chromium CI Testing Engine',
      subtitle: 'Pure-Rust DOM backend with embedded QuickJS for millisecond hermetic verification.',
      problem: 'Running Chromium in Docker CI pipelines requires bloated 1.5GB images, privileged sandbox configurations (`--no-sandbox` security risks), and slow 5-second cold starts that make test suites crawl.',
      solution: 'The experimental `vakbrowse-dom` backend implements `EngineLauncher` and `PageOps` using an in-process pure-Rust HTML tokenizer and QuickJS runtime on a dedicated OS thread. It runs real JS, evaluates selectors, and validates workflows with ZERO browser processes.',
      metrics: [
        { label: 'IMAGE OVERHEAD', value: '0MB (NO CHROMIUM)' },
        { label: 'COLD START TIME', value: '<3ms' },
        { label: 'SECURITY RISKS', value: 'NO CHROMIUM PRIVILEGES' },
      ],
      codeExample: {
        lang: 'Rust Hermetic Test',
        snippet: `use vakbrowse_server::{SessionManager, Policy, Request, SessionOptions};
use vakbrowse_dom::DomLauncher;
use std::sync::Arc;

// Spin up a browser session without launching any external process
let manager = SessionManager::new(Policy::default(), Arc::new(DomLauncher));
let res = manager.handle(Request::Open {
    options: SessionOptions {
        url: "file:///tests/fixtures/app.html".into(),
        backend: "dom".into(),
        ..Default::default()
    }
}).await;`,
      },
    },
  ];

  const selectedCase = useCases.find((c) => c.id === activeCase) || useCases[0];

  return (
    <div className="max-w-7xl mx-auto px-4 sm:px-6 py-10 space-y-12">
      {/* Header */}
      <div className="space-y-3">
        <div className="badge-orange">AGENT BLUEPRINTS</div>
        <h1 className="text-3xl sm:text-4xl font-bold font-sans text-bone">
          Engineered For Mission-Critical Workflows
        </h1>
        <p className="text-sm sm:text-base text-text-dim font-sans max-w-3xl">
          Explore real-world architecture patterns where vakBrowse delivers 10x speed, 95% token savings, and rock-solid reliability compared to legacy automation stacks.
        </p>
      </div>

      {/* Selector pills */}
      <div className="flex items-center gap-2 overflow-x-auto pb-2 border-b border-border">
        {useCases.map((c) => (
          <button
            key={c.id}
            onClick={() => setActiveCase(c.id)}
            className={`px-3 py-2 rounded-xs font-mono text-xs tracking-wider transition-all whitespace-nowrap flex items-center gap-2 ${
              activeCase === c.id
                ? 'bg-accent text-white font-bold shadow-te-button'
                : 'bg-card border border-border text-text-dim hover:text-text hover:bg-surface'
            }`}
          >
            <span className="text-[10px] opacity-70">{c.code}</span>
            <span>{c.badge}</span>
          </button>
        ))}
      </div>

      {/* Main Selected Detail Panel */}
      <div className="te-panel rounded-xs border-border p-6 md:p-8 space-y-8">
        <div className="flex flex-col lg:flex-row lg:items-center justify-between gap-4 pb-6 border-b border-border">
          <div>
            <div className="badge-yellow mb-2">{selectedCase.code} // {selectedCase.badge}</div>
            <h2 className="text-2xl sm:text-3xl font-bold font-sans text-bone">
              {selectedCase.title}
            </h2>
            <p className="text-sm text-text-dim font-sans mt-1">
              {selectedCase.subtitle}
            </p>
          </div>

          <button
            onClick={() => onNavigate('playground')}
            className="btn-primary self-start lg:self-center"
          >
            TEST THIS IN PLAYGROUND ›
          </button>
        </div>

        {/* Problem vs Solution */}
        <div className="grid grid-cols-1 md:grid-cols-2 gap-6">
          <div className="p-5 bg-surface/50 border border-border rounded-xs space-y-2">
            <div className="text-xs font-mono text-danger font-bold uppercase">
              THE CONVENTIONAL CHALLENGE
            </div>
            <p className="text-xs sm:text-sm text-text-dim leading-relaxed">
              {selectedCase.problem}
            </p>
          </div>

          <div className="p-5 bg-card border border-accent/40 rounded-xs space-y-2">
            <div className="text-xs font-mono text-accent font-bold uppercase">
              THE VAKBROWSE ADVANTAGE
            </div>
            <p className="text-xs sm:text-sm text-bone leading-relaxed">
              {selectedCase.solution}
            </p>
          </div>
        </div>

        {/* Metrics Ticker */}
        <div className="grid grid-cols-1 sm:grid-cols-3 gap-4">
          {selectedCase.metrics.map((m, idx) => (
            <div key={idx} className="p-4 bg-surface border border-border rounded-xs text-center font-mono">
              <div className="text-[10px] text-text-muted uppercase tracking-wider">{m.label}</div>
              <div className="text-xl font-bold text-yellow mt-1">{m.value}</div>
            </div>
          ))}
        </div>

        {/* Code implementation */}
        <div className="space-y-2">
          <div className="flex items-center justify-between text-xs font-mono">
            <span className="text-text-dim uppercase tracking-wider">
              PRODUCTION IMPLEMENTATION // {selectedCase.codeExample.lang}
            </span>
            <span className="text-[10px] text-text-muted">COPY TO INTEGRATE</span>
          </div>
          <pre className="p-4 bg-bg border border-border rounded-xs text-xs font-mono text-emerald-400 overflow-x-auto leading-relaxed shadow-te-inset">
            {selectedCase.codeExample.snippet}
          </pre>
        </div>
      </div>
    </div>
  );
}
