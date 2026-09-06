import React from 'react';
import type { PageTab } from './Header';

interface FooterProps {
  onSelectTab: (tab: PageTab) => void;
}

export function Footer({ onSelectTab }: FooterProps) {
  return (
    <footer className="border-t border-border bg-bg text-text-dim text-xs font-mono select-none">
      {/* Millimeter grid ruler bar */}
      <div className="h-3 border-b border-border/60 bg-[linear-gradient(to_right,#33333f_1px,transparent_1px)] bg-[size:10px_100%] opacity-40" />

      <div className="max-w-7xl mx-auto px-6 py-12 grid grid-cols-1 md:grid-cols-4 gap-8">
        {/* Col 1: Brand & Spec */}
        <div className="space-y-3">
          <div className="flex items-center gap-2">
            <div className="w-5 h-5 rounded-xs bg-surface border border-accent text-accent font-mono font-bold text-xs flex items-center justify-center">
              vB
            </div>
            <span className="font-mono font-bold text-sm text-bone">vakBrowse</span>
          </div>
          <p className="text-text-muted text-[11px] leading-relaxed">
            The high-precision, token-frugal browser runtime engineered specifically for AI agents, LLM tool loops, and autonomous workflows.
          </p>
          <div className="text-[10px] text-text-muted space-y-0.5">
            <div>ARCHITECTURE: RUST SEAM (CDP + DOM-QUICKJS)</div>
            <div>LICENSE: APACHE-2.0 / MIT</div>
            <div>PLATFORMS: LINUX AMD64, MACOS ARM/X86, DOCKER</div>
          </div>
        </div>

        {/* Col 2: Navigation */}
        <div className="space-y-2">
          <div className="text-[11px] font-bold text-bone tracking-wider uppercase">
            SPECIFICATION
          </div>
          <ul className="space-y-1.5 text-[11px]">
            <li>
              <button onClick={() => onSelectTab('overview')} className="hover:text-accent transition-colors">
                [01] Overview & Core Tenets
              </button>
            </li>
            <li>
              <button onClick={() => onSelectTab('use-cases')} className="hover:text-accent transition-colors">
                [02] Production Use Cases
              </button>
            </li>
            <li>
              <button onClick={() => onSelectTab('architecture')} className="hover:text-accent transition-colors">
                [03] Dual-Engine Architecture
              </button>
            </li>
            <li>
              <button onClick={() => onSelectTab('economics')} className="hover:text-accent transition-colors">
                [04] Token Economics & ROI
              </button>
            </li>
            <li>
              <button onClick={() => onSelectTab('docs')} className="hover:text-accent transition-colors">
                [05] Developer Quickstart
              </button>
            </li>
            <li>
              <button onClick={() => onSelectTab('pricing')} className="hover:text-accent transition-colors">
                [06] Commercial Fleet Tiers
              </button>
            </li>
          </ul>
        </div>

        {/* Col 3: Integrations & Surfaces */}
        <div className="space-y-2">
          <div className="text-[11px] font-bold text-bone tracking-wider uppercase">
            INTERFACES
          </div>
          <ul className="space-y-1.5 text-[11px]">
            <li className="flex items-center gap-1.5">
              <span className="text-accent font-bold">›</span>
              <span>Python SDK (`pip install vakbrowse`)</span>
            </li>
            <li className="flex items-center gap-1.5">
              <span className="text-accent font-bold">›</span>
              <span>vak-mcp (Claude Code / Cursor stdio)</span>
            </li>
            <li className="flex items-center gap-1.5">
              <span className="text-accent font-bold">›</span>
              <span>CLI `vak` & Daemon `vakd` (UDS socket)</span>
            </li>
            <li className="flex items-center gap-1.5">
              <span className="text-accent font-bold">›</span>
              <span>REST API & WebSocket Bridge (`vakd-rest`)</span>
            </li>
            <li className="flex items-center gap-1.5">
              <span className="text-accent font-bold">›</span>
              <span>C FFI Shared Library (`libvakbrowse_ffi`)</span>
            </li>
          </ul>
        </div>

        {/* Col 4: Hardware Specs */}
        <div className="space-y-3">
          <div className="text-[11px] font-bold text-bone tracking-wider uppercase">
            INDUSTRIAL METRICS
          </div>
          <div className="p-3 bg-card border border-border rounded-xs space-y-2 font-mono text-[11px]">
            <div className="flex justify-between">
              <span className="text-text-muted">TOKEN PERCEPTION:</span>
              <span className="text-yellow font-semibold">&lt;500 TOKENS</span>
            </div>
            <div className="flex justify-between">
              <span className="text-text-muted">SNAPSHOT SPEED:</span>
              <span className="text-emerald-400 font-semibold">~42MS</span>
            </div>
            <div className="flex justify-between">
              <span className="text-text-muted">MCP TOOLS EXPOSED:</span>
              <span className="text-bone font-semibold">34 DISPATCHABLE</span>
            </div>
            <div className="flex justify-between">
              <span className="text-text-muted">MEMORY FOOTPRINT:</span>
              <span className="text-bone font-semibold">&lt;85MB IDLE</span>
            </div>
          </div>
          <div className="text-[10px] text-text-muted">
            ENGINEERED WITH ARCHITECTURAL PRECISION & HIGH UTILITY.
          </div>
        </div>
      </div>

      <div className="border-t border-border px-6 py-4 flex flex-col md:flex-row items-center justify-between text-[11px] text-text-muted gap-2">
        <div>
          © 2026 vakBrowse Project. High-performance browser engine for machine intelligence.
        </div>
        <div className="flex items-center gap-4">
          <span>PORT: 7788</span>
          <span>PROTOCOL: UDS / NDJSON</span>
          <span>REVISION: v0.4.0-STABLE</span>
        </div>
      </div>
    </footer>
  );
}
