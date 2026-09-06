import React from 'react';
import type { PageTab } from '../components/Header';
import schematicImg from '../assets/schematic.jpg';

interface ArchitecturePageProps {
  onNavigate: (tab: PageTab) => void;
}

export function ArchitecturePage({ onNavigate }: ArchitecturePageProps) {
  return (
    <div className="max-w-7xl mx-auto px-4 sm:px-6 py-10 space-y-14">
      {/* Header */}
      <div className="space-y-3">
        <div className="badge-dim">SYSTEM BLUEPRINT // SPECIFICATION</div>
        <h1 className="text-3xl sm:text-4xl font-bold font-sans text-bone">
          Architectural Topology & The Sacred Engine Seam
        </h1>
        <p className="text-sm sm:text-base text-text-dim font-sans max-w-3xl">
          vakBrowse is engineered from the ground up in Rust around strict trait boundaries.
          All surface APIs (Daemon, CLI, REST, WebSocket, MCP, and C FFI) communicate through an identical command model, decoupled from underlying browser engines.
        </p>
      </div>

      {/* Main Schematic Diagram Image */}
      <div className="te-panel rounded-xs border-border p-6 space-y-4">
        <div className="flex items-center justify-between pb-3 border-b border-border">
          <span className="font-mono text-xs text-bone font-bold uppercase tracking-wider">
            SCHEMATIC FIG 1.0 // DUAL-ENGINE TOPOLOGY
          </span>
          <span className="badge-orange">RUST CORE // TRAIT-BOUND</span>
        </div>

        <div className="relative aspect-[16/9] bg-surface rounded-xs overflow-hidden border border-border">
          <img
            src={schematicImg}
            alt="vakBrowse Software Browser Engine Schematic"
            className="w-full h-full object-contain p-4"
          />
        </div>

        <div className="text-[11px] font-mono text-text-muted flex justify-between">
          <span>ENGINE SEAM TRAITS: EngineLauncher + PageOps</span>
          <span>DISPATCH MODEL: Request::Act / Request::Batch</span>
        </div>
      </div>

      {/* 4 Architectural Cornerstones */}
      <div className="grid grid-cols-1 md:grid-cols-2 gap-6">
        {/* Cornerstone 1: The Engine Seam */}
        <div className="te-panel rounded-xs border-border p-6 space-y-3">
          <div className="flex items-center justify-between pb-2 border-b border-border">
            <span className="text-xs font-mono text-accent font-bold uppercase">
              01 // THE SACRED ENGINE SEAM
            </span>
            <span className="badge-dim">crates/vakbrowse-engine</span>
          </div>
          <h3 className="text-lg font-bold font-sans text-bone">
            Trait-Driven Backend Agnosticism
          </h3>
          <p className="text-xs sm:text-sm text-text-dim leading-relaxed">
            All capabilities pass through the <code className="text-accent bg-bg px-1 rounded-xs">EngineLauncher</code> and <code className="text-accent bg-bg px-1 rounded-xs">PageOps</code> traits in <code className="text-text">vakbrowse-engine</code>.
            <code className="text-bone">SessionManager</code> holds an <code className="text-text">Arc&lt;dyn EngineLauncher&gt;</code>. Neither the CLI, the REST server, the Python FFI, nor the MCP server ever touch or leak CDP or chromiumoxide types.
          </p>
          <div className="p-3 bg-bg border border-border rounded-xs text-[11px] font-mono text-emerald-400">
            SessionManager::new(policy, Arc::new(DomLauncher)) // Chrome-free test!
          </div>
        </div>

        {/* Cornerstone 2: The Perception Engine */}
        <div className="te-panel rounded-xs border-border p-6 space-y-3">
          <div className="flex items-center justify-between pb-2 border-b border-border">
            <span className="text-xs font-mono text-accent font-bold uppercase">
              02 // ACCESSIBILITY PERCEPTION UNIT
            </span>
            <span className="badge-dim">crates/vakbrowse-perception</span>
          </div>
          <h3 className="text-lg font-bold font-sans text-bone">
            Pure AX-Tree to Token Compression
          </h3>
          <p className="text-xs sm:text-sm text-text-dim leading-relaxed">
            Instead of stringifying the DOM tree, vakBrowse extracts the Chromium Accessibility Tree, filters out non-semantic and hidden nodes, and assigns stable element references like <code className="text-yellow bg-bg px-1 rounded-xs">@e1</code>, <code className="text-yellow bg-bg px-1 rounded-xs">@e2</code>. Stale references return classified <code className="text-danger">NotFound</code> errors, ensuring zero silent misclicks.
          </p>
          <div className="p-3 bg-bg border border-border rounded-xs text-[11px] font-mono text-yellow">
            Perception target: &lt;500 tokens per page (98.6% compression ratio)
          </div>
        </div>

        {/* Cornerstone 3: Trusted Input Dispatch */}
        <div className="te-panel rounded-xs border-border p-6 space-y-3">
          <div className="flex items-center justify-between pb-2 border-b border-border">
            <span className="text-xs font-mono text-accent font-bold uppercase">
              03 // TRUSTED HARDWARE INPUTS
            </span>
            <span className="badge-dim">Box-Model Center Dispatch</span>
          </div>
          <h3 className="text-lg font-bold font-sans text-bone">
            Real Mouse Coordinates & Auto-Scroll
          </h3>
          <p className="text-xs sm:text-sm text-text-dim leading-relaxed">
            Clicks are real <code className="text-accent bg-bg px-1 rounded-xs">Input.dispatchMouseEvent</code> sequences dispatched at exact element box-model centers. Before clicking, the engine executes an automated <code className="text-bone">scrollIntoView({'{'}block:'center'{'}'})</code>, ensuring below-the-fold links and long-scroll SERP items are always in viewport before dispatch.
          </p>
          <div className="p-3 bg-bg border border-border rounded-xs text-[11px] font-mono text-text-dim">
            Fills invoke native prototype value setters + input/change synthetic events
          </div>
        </div>

        {/* Cornerstone 4: Anti-Detection & Proxy Persistence */}
        <div className="te-panel rounded-xs border-border p-6 space-y-3">
          <div className="flex items-center justify-between pb-2 border-b border-border">
            <span className="text-xs font-mono text-accent font-bold uppercase">
              04 // HARDWARE STEALTH & PROXY CYCLING
            </span>
            <span className="badge-dim">crates/vakbrowse-stealth</span>
          </div>
          <h3 className="text-lg font-bold font-sans text-bone">
            Deterministic Fingerprints & URL Recovery
          </h3>
          <p className="text-xs sm:text-sm text-text-dim leading-relaxed">
            Stealth profiles defeat <code className="text-accent bg-bg px-1 rounded-xs">navigator.webdriver</code> and inject genuine plugin lists, speech synthesis biometrics, and Bézier mouse paths. When an IP is throttled, <code className="text-bone">Action::RotateProxy</code> re-launches Chrome on the next endpoint and automatically restores the session's active URL.
          </p>
          <div className="p-3 bg-bg border border-border rounded-xs text-[11px] font-mono text-text-dim">
            Site isolation left intact; no broken cross-frame security
          </div>
        </div>
      </div>

      {/* Experimental DOM Backend Highlight */}
      <div className="te-panel rounded-xs border-accent/60 p-6 space-y-4 bg-surface/30">
        <div className="flex items-center gap-2">
          <span className="badge-yellow">EXPERIMENTAL PROOF OF SEAM</span>
          <span className="font-mono font-bold text-sm text-bone">
            vakbrowse-dom // PURE RUST DOM + QUICKJS
          </span>
        </div>

        <p className="text-xs sm:text-sm text-text-dim leading-relaxed">
          Proving that our engine seam is truly swappable: <code className="text-accent">vakbrowse-dom</code> is an in-process, single-doc DOM tree with an embedded QuickJS engine running on a dedicated OS thread behind an MPSC bridge. It executes inline <code className="text-text">&lt;script&gt;</code> tags, resolves <code className="text-text">document.querySelector</code> via a dependency-free matcher, and evaluates JS expressions with ZERO browser processes.
        </p>

        <div className="flex items-center gap-4 text-xs font-mono text-text-muted">
          <span>RUNS REAL JS</span>
          <span>•</span>
          <span>NO CHROMIUM REQUIRED</span>
          <span>•</span>
          <span>SUB-MILLISECOND EXECUTION</span>
        </div>
      </div>
    </div>
  );
}
