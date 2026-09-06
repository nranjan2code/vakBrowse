import React from 'react';
import { TokenCalculator } from '../components/TokenCalculator';
import type { PageTab } from '../components/Header';
import terminalImg from '../assets/terminal.jpg';
import schematicImg from '../assets/schematic.svg';

interface HomePageProps {
  onNavigate: (tab: PageTab) => void;
}

export function HomePage({ onNavigate }: HomePageProps) {
  return (
    <div className="space-y-16 pb-20">
      {/* Hero Section */}
      <section className="relative pt-8 md:pt-14">
        {/* Subtle millimeter background grid */}
        <div className="absolute inset-0 bg-millimeter-grid bg-grid-mm opacity-15 pointer-events-none" />

        <div className="max-w-7xl mx-auto px-4 sm:px-6 relative z-10">
          <div className="flex flex-col lg:flex-row items-center gap-10">
            {/* Left: Manifesto & Headline */}
            <div className="flex-1 space-y-6 text-left">
              <div className="inline-flex items-center gap-2 px-2.5 py-1 rounded-xs bg-surface border border-border text-[11px] font-mono">
                <span className="w-2 h-2 rounded-full bg-accent animate-ping" />
                <span className="text-bone font-semibold">VAKBROWSE RUNTIME // INDUSTRIAL RELEASE v0.4</span>
              </div>

              <h1 className="text-4xl sm:text-5xl lg:text-6xl font-bold tracking-tight text-bone font-sans leading-[1.08]">
                THE AGENT-NATIVE BROWSER RUNTIME.
                <span className="block text-accent font-mono text-3xl sm:text-4xl lg:text-5xl mt-1">
                  REAL BROWSER POWER. ZERO AGENT BLOAT.
                </span>
              </h1>

              <p className="text-base sm:text-lg text-text-dim leading-relaxed font-sans max-w-2xl">
                Standard browsers and test drivers waste 50,000 tokens of HTML noise on every page turn.
                Powered by a headless Chromium core, <span className="text-bone font-medium">vakBrowse</span> is a high-speed Rust runtime that compresses the live accessibility tree into <span className="text-yellow font-mono font-semibold">&lt;500 tokens</span> of actionable references (<code className="text-accent bg-card px-1 py-0.5 rounded-xs">@e42</code>) with trusted hardware mouse events and 35 native MCP tools.
              </p>

              {/* Action buttons */}
              <div className="flex flex-wrap items-center gap-3 pt-2">
                <button
                  onClick={() => onNavigate('playground')}
                  className="px-6 py-3 rounded-xs bg-accent hover:bg-accent-hover text-white text-xs font-mono font-bold tracking-wider uppercase shadow-te-button active:translate-y-0.5 transition-all flex items-center gap-2"
                >
                  <span className="w-2 h-2 bg-white rounded-full" />
                  <span>LAUNCH INTERACTIVE PLAYGROUND</span>
                  <span className="text-xs">⚡</span>
                </button>

                <button
                  onClick={() => onNavigate('docs')}
                  className="px-5 py-3 rounded-xs bg-surface hover:bg-surface-elevated border border-border hover:border-border-strong text-bone text-xs font-mono tracking-wider uppercase transition-all flex items-center gap-2"
                >
                  <span>QUICKSTART GUIDE</span>
                  <span className="text-text-muted">›</span>
                </button>
              </div>

              {/* Hardware Spec Ticker */}
              <div className="grid grid-cols-3 gap-3 pt-4 border-t border-border/80 font-mono text-xs max-w-lg">
                <div>
                  <div className="text-[10px] text-text-muted">TOKEN FOOTPRINT</div>
                  <div className="text-bone font-bold text-sm sm:text-base text-yellow">&lt;500 TOKENS</div>
                </div>
                <div>
                  <div className="text-[10px] text-text-muted">ENGINE</div>
                  <div className="text-bone font-bold text-sm sm:text-base">CDP (CHROMIUM)</div>
                </div>
                <div>
                  <div className="text-[10px] text-text-muted">NATIVE PROTOCOL</div>
                  <div className="text-bone font-bold text-sm sm:text-base text-accent">35 MCP TOOLS</div>
                </div>
              </div>
            </div>

            {/* Right: Hardware Industrial Visual */}
            <div className="flex-1 w-full max-w-xl">
              <div className="te-panel rounded-xs border-border overflow-hidden shadow-2xl group">
                <div className="te-panel-header">
                  <span className="text-bone font-mono font-bold">
                    CONTROLLER SPEC // HARDWARE TERMINAL
                  </span>
                  <span className="badge-orange">MODEL BAC-1</span>
                </div>
                <div className="relative aspect-[16/9] bg-surface overflow-hidden">
                  <img
                    src={terminalImg}
                    alt="vakBrowse BAC-1 Hardware Automation Controller"
                    className="w-full h-full object-cover group-hover:scale-[1.02] transition-transform duration-700"
                  />
                  {/* Subtle technical overlay */}
                  <div className="absolute bottom-2 left-2 px-2 py-1 bg-bg/85 backdrop-blur border border-border text-[10px] font-mono text-text-dim rounded-xs">
                    PHYSICAL AUTOMATION METAPHOR // TACTILE TE AESTHETIC
                  </div>
                </div>
              </div>
            </div>
          </div>
        </div>
      </section>

      {/* Live Perception Comparison: Raw HTML vs vakBrowse */}
      <section className="max-w-7xl mx-auto px-4 sm:px-6">
        <div className="text-center max-w-3xl mx-auto mb-8 space-y-2">
          <div className="badge-yellow">CORE EFFICIENCY ADVANTAGE</div>
          <h2 className="text-2xl sm:text-3xl font-bold font-sans text-bone">
            Why Standard Headless Browsers Bankrupt Agent Loops
          </h2>
          <p className="text-sm text-text-dim font-sans">
            AI agents don't need megabytes of inline SVG paths, CSS styling rules, or nested DIV containers.
            vakBrowse strips the rendering noise and generates high-density semantic action graphs.
          </p>
        </div>

        <div className="grid grid-cols-1 lg:grid-cols-2 gap-6">
          {/* Left: The Raw DOM Problem */}
          <div className="te-panel rounded-xs border-border p-5 space-y-4">
            <div className="flex items-center justify-between pb-3 border-b border-border">
              <span className="font-mono text-xs text-danger font-bold flex items-center gap-1.5">
                <span className="w-2 h-2 rounded-full bg-danger" />
                CONVENTIONAL PUPPETEER / PLAYWRIGHT
              </span>
              <span className="text-[10px] font-mono px-2 py-0.5 bg-danger/10 border border-danger/30 text-danger rounded-xs font-semibold">
                ~48,500 TOKENS
              </span>
            </div>

            <p className="text-xs text-text-dim">
              Dumping raw HTML crashes context windows, burns dollars on noisy scripts, and confuses models with invisible elements:
            </p>

            <pre className="p-3 bg-bg border border-border rounded-xs text-[11px] font-mono text-text-muted overflow-x-auto leading-relaxed max-h-56">
{`<div class="header-nav-v2_wrapper__29s8a" style="display:flex;">
  <script type="text/javascript">window.__PRELOADED_STATE__={...}</script>
  <style>.header-nav-v2_wrapper__29s8a{box-sizing:border-box}</style>
  <svg width="24" height="24" viewBox="0 0 24 24" fill="none">
    <path d="M12 2L2 7L12 12L22 7L12 2Z" stroke="#4A5568"/>
  </svg>
  <div class="sc-fzoLsD kGzWxy"><span class="label">Search</span></div>
  <!-- 2,400 more lines of CSS-in-JS hashes, trackers & SVG markup -->
</div>`}
            </pre>

            <div className="text-[11px] font-mono text-danger/80 space-y-1">
              <div>✗ Context Window Exhaustion (40k+ tokens per cycle)</div>
              <div>✗ Unstable CSS selectors break on every UI rebuild</div>
              <div>✗ Inability to click obscured or below-the-fold nodes</div>
            </div>
          </div>

          {/* Right: The vakBrowse Solution */}
          <div className="te-panel rounded-xs border-2 border-accent p-5 space-y-4 relative">
            <div className="absolute top-0 right-0 bg-accent text-white font-mono text-[9px] px-2 py-0.5 uppercase tracking-widest font-bold">
              VAKBROWSE A11Y PERCEPTION
            </div>

            <div className="flex items-center justify-between pb-3 border-b border-border">
              <span className="font-mono text-xs text-accent font-bold flex items-center gap-1.5">
                <span className="w-2 h-2 rounded-full bg-accent" />
                VAKBROWSE ACCESSIBILITY GRAPH
              </span>
              <span className="text-[10px] font-mono px-2 py-0.5 bg-accent/20 border border-accent/40 text-accent rounded-xs font-semibold">
                382 TOKENS (99.2% LESS)
              </span>
            </div>

            <p className="text-xs text-text-dim">
              The agent receives only interactive, semantically meaningful elements with guaranteed-stable target handles:
            </p>

            <pre className="p-3 bg-bg border border-border rounded-xs text-[11px] font-mono text-emerald-400 overflow-x-auto leading-relaxed max-h-56">
{`Page: Wikipedia — Artificial Intelligence
URL:  https://en.wikipedia.org/wiki/Artificial_intelligence

[Interactive Elements]
@e1  textbox  "Search Wikipedia"  value=""
@e2  button   "Search"
@e3  link     "Machine learning"
@e4  link     "Deep learning"
@e5  link     "Neural networks"
@e6  combobox "Language selection"  value="English"

-> Dispatch action: { "type": "click", "ref": "@e3" }`}
            </pre>

            <div className="text-[11px] font-mono text-emerald-400 space-y-1">
              <div>✓ 380 tokens fits into any reasoning agent budget</div>
              <div>✓ Trusted click uses box-model center + auto-scroll</div>
              <div>✓ Stale references return NotFound, never silent misfires</div>
            </div>
          </div>
        </div>
      </section>

      {/* Architecture & Engineering Schematic Showcase */}
      <section className="max-w-7xl mx-auto px-4 sm:px-6">
        <div className="te-panel rounded-xs border-border p-6 space-y-6">
          <div className="flex flex-col md:flex-row md:items-center justify-between pb-4 border-b border-border gap-4">
            <div>
              <div className="badge-dim mb-1">SYSTEM SCHEMATIC // REV 0.4</div>
              <h3 className="text-xl font-bold font-sans text-bone">
                The Sacred Engine Seam
              </h3>
              <p className="text-xs text-text-dim font-sans mt-0.5">
                Powered by Chromium via Chrome DevTools Protocol (CDP). One engine, one command model for every surface.
              </p>
            </div>
            <button
              onClick={() => onNavigate('architecture')}
              className="btn-secondary self-start"
            >
              EXPLORE FULL ARCHITECTURE SPECS ›
            </button>
          </div>

          <div className="relative aspect-[16/9] bg-surface rounded-xs overflow-hidden border border-border">
            <img
              src={schematicImg}
              alt="vakBrowse Browser Runtime Architecture Schematic"
              className="w-full h-full object-contain p-2"
            />
          </div>

          <div className="grid grid-cols-1 md:grid-cols-4 gap-4 text-xs font-mono">
            <div className="p-3 bg-surface border border-border rounded-xs">
              <div className="text-accent font-bold">01 // ENGINE SEAM</div>
              <div className="text-text-dim text-[11px] mt-1">
                Trait-level abstraction over `EngineLauncher` & `PageOps`. Never leaks CDP types outside the engine crate.
              </div>
            </div>
            <div className="p-3 bg-surface border border-border rounded-xs">
              <div className="text-accent font-bold">02 // TRUSTED INPUTS</div>
              <div className="text-text-dim text-[11px] mt-1">
                Dispatches trusted OS-level mouse click events at box-model centers with automatic `scrollIntoView`.
              </div>
            </div>
            <div className="p-3 bg-surface border border-border rounded-xs">
              <div className="text-accent font-bold">03 // PROXY POOLS</div>
              <div className="text-text-dim text-[11px] mt-1">
                Rotate through residential/datacenter IPs with automatic session URL state restoration.
              </div>
            </div>
            <div className="p-3 bg-surface border border-border rounded-xs">
              <div className="text-accent font-bold">04 // 35 MCP TOOLS</div>
              <div className="text-text-dim text-[11px] mt-1">
                Stdio transducer supporting both NDJSON and spec-literal Content-Length framings for Claude Code & Cursor.
              </div>
            </div>
          </div>
        </div>
      </section>

      {/* Interactive Token Economics Calculator */}
      <section className="max-w-7xl mx-auto px-4 sm:px-6">
        <div className="text-center max-w-2xl mx-auto mb-8 space-y-2">
          <div className="badge-orange">FINANCIAL TELEMETRY</div>
          <h2 className="text-2xl sm:text-3xl font-bold font-sans text-bone">
            Calculate Your Agent Fleet Savings
          </h2>
          <p className="text-sm text-text-dim font-sans">
            See how much LLM token spend you save every month by running vakBrowse perception over raw DOM dumping.
          </p>
        </div>

        <TokenCalculator />
      </section>

      {/* Use Cases Grid Teaser */}
      <section className="max-w-7xl mx-auto px-4 sm:px-6">
        <div className="flex flex-col sm:flex-row sm:items-end justify-between mb-6 pb-3 border-b border-border gap-4">
          <div>
            <div className="badge-dim mb-1">APPLICATIONS</div>
            <h2 className="text-2xl font-bold font-sans text-bone">
              Engineered For Five Core Agent Workflows
            </h2>
          </div>
          <button
            onClick={() => onNavigate('use-cases')}
            className="btn-orange-outline self-start"
          >
            VIEW ALL DETAILED BLUEPRINTS ›
          </button>
        </div>

        <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
          <div
            onClick={() => onNavigate('use-cases')}
            className="p-5 bg-card border border-border hover:border-accent/60 rounded-xs cursor-pointer transition-all group"
          >
            <div className="text-[10px] font-mono text-accent font-bold uppercase mb-2">
              [01] DEEP RESEARCH
            </div>
            <h4 className="text-base font-bold font-sans text-bone group-hover:text-accent transition-colors">
              Autonomous Web Research & Fact Extraction
            </h4>
            <p className="text-xs text-text-dim mt-2 leading-relaxed">
              Synthesize 20KB of clean markdown from massive 500KB article trees. Follow multi-tab references without losing context budget.
            </p>
          </div>

          <div
            onClick={() => onNavigate('use-cases')}
            className="p-5 bg-card border border-border hover:border-accent/60 rounded-xs cursor-pointer transition-all group"
          >
            <div className="text-[10px] font-mono text-accent font-bold uppercase mb-2">
              [02] LARGE-SCALE EXTRACTION
            </div>
            <h4 className="text-base font-bold font-sans text-bone group-hover:text-accent transition-colors">
              Anti-Bot Scraping & Proxy Rotation
            </h4>
            <p className="text-xs text-text-dim mt-2 leading-relaxed">
              Defeat automation walls with custom stealth profiles and rotate proxies on the fly while retaining the exact session navigation state.
            </p>
          </div>

          <div
            onClick={() => onNavigate('use-cases')}
            className="p-5 bg-card border border-border hover:border-accent/60 rounded-xs cursor-pointer transition-all group"
          >
            <div className="text-[10px] font-mono text-accent font-bold uppercase mb-2">
              [03] WEB-ACTION RPA
            </div>
            <h4 className="text-base font-bold font-sans text-bone group-hover:text-accent transition-colors">
              React-Safe Form Automation & Submissions
            </h4>
            <p className="text-xs text-text-dim mt-2 leading-relaxed">
              Native JavaScript value setters trigger synthetic change events cleanly without triggering SPA validation traps or orphaned keystrokes.
            </p>
          </div>
        </div>
      </section>

      {/* Call to Action Bar */}
      <section className="max-w-7xl mx-auto px-4 sm:px-6">
        <div className="te-panel rounded-xs border-accent p-8 text-center space-y-4 bg-gradient-to-b from-card to-surface">
          <div className="badge-orange mx-auto">READY FOR PRODUCTION</div>
          <h2 className="text-3xl font-bold font-sans text-bone">
            Give Your Agents A Real Web Browser Today.
          </h2>
          <p className="text-sm text-text-dim max-w-xl mx-auto font-sans">
            Install the native Python library, run the UDS daemon, or plug `vak-mcp` into your Claude Code or Cursor environment in 60 seconds.
          </p>
          <div className="flex flex-wrap items-center justify-center gap-3 pt-2">
            <button
              onClick={() => onNavigate('playground')}
              className="btn-primary px-6 py-2.5 text-xs"
            >
              TEST IN PLAYGROUND
            </button>
            <button
              onClick={() => onNavigate('docs')}
              className="btn-secondary px-6 py-2.5 text-xs"
            >
              READ DOCS & SDKS
            </button>
          </div>
        </div>
      </section>
    </div>
  );
}
