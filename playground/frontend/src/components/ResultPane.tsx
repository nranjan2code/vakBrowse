import React, { useState } from 'react';
import type { ActionResult } from '../lib/types';

interface Props {
  lastResult: ActionResult | null;
  loading: boolean;
}

type TabType = 'OUTPUT' | 'WIRE_JSON';

export function ResultPane({ lastResult, loading }: Props) {
  const [activeTab, setActiveTab] = useState<TabType>('OUTPUT');

  return (
    <div className="w-84 lg:w-96 border-l border-border bg-card flex flex-col h-full font-mono text-xs select-none">
      {/* Pane Header */}
      <div className="p-3 border-b border-border bg-surface flex items-center justify-between">
        <div className="flex items-center gap-2">
          <span className="w-2 h-2 rounded-xs bg-yellow" />
          <span className="font-bold text-bone uppercase tracking-wider text-xs">
            TELEMETRY & RESULTS
          </span>
        </div>
        {loading && (
          <span className="badge-orange animate-pulse">
            DISPATCHING...
          </span>
        )}
      </div>

      {/* Tabs */}
      <div className="flex items-center border-b border-border bg-surface/50 text-[11px]">
        <button
          onClick={() => setActiveTab('OUTPUT')}
          className={`flex-1 py-1.5 text-center font-bold tracking-wider transition-colors ${
            activeTab === 'OUTPUT'
              ? 'bg-card text-bone border-b-2 border-accent'
              : 'text-text-dim hover:text-text'
          }`}
        >
          VIEWPORT OUTPUT
        </button>
        <button
          onClick={() => setActiveTab('WIRE_JSON')}
          className={`flex-1 py-1.5 text-center font-bold tracking-wider transition-colors ${
            activeTab === 'WIRE_JSON'
              ? 'bg-card text-bone border-b-2 border-accent'
              : 'text-text-dim hover:text-text'
          }`}
        >
          WIRE PROTOCOL JSON
        </button>
      </div>

      {/* Main Content Area */}
      <div className="flex-1 overflow-y-auto p-3">
        {!lastResult ? (
          <div className="p-6 text-center text-text-dim space-y-2">
            <div className="text-text-muted text-xs">NO DISPATCH YET</div>
            <p className="text-[11px] leading-relaxed font-sans">
              Click an element, extract text, or run any action from the top toolbar to view the live result stream.
            </p>
          </div>
        ) : activeTab === 'WIRE_JSON' ? (
          <div className="space-y-2">
            <div className="text-[10px] text-text-muted uppercase">
              RAW RESPONSE FRAME:
            </div>
            <pre className="p-3 bg-bg border border-border rounded-xs text-[11px] font-mono text-emerald-400 overflow-x-auto leading-relaxed max-h-[600px] shadow-te-inset">
              {JSON.stringify(lastResult, null, 2)}
            </pre>
          </div>
        ) : (
          <div className="space-y-3">
            {/* Extracted Text / Source / Eval Output */}
            {lastResult.type === 'text' && (
              <div className="space-y-2">
                <div className="flex items-center justify-between">
                  <span className="text-[10px] text-text-muted uppercase">
                    CONTENT STREAM ({lastResult.text.length} BYTES):
                  </span>
                  <button
                    onClick={() => navigator.clipboard.writeText(lastResult.text)}
                    className="text-[10px] text-accent hover:underline uppercase"
                  >
                    COPY TEXT
                  </button>
                </div>
                <pre className="p-3 bg-bg border border-border rounded-xs text-[11px] font-sans text-bone whitespace-pre-wrap leading-relaxed max-h-[500px] overflow-y-auto shadow-te-inset">
                  {lastResult.text}
                </pre>
              </div>
            )}

            {/* Navigated Result */}
            {lastResult.type === 'navigated' && (
              <div className="te-panel rounded-xs border-border p-3 space-y-2">
                <div className="badge-green">NAVIGATION CONFIRMED</div>
                <div className="text-xs text-bone font-sans font-medium">
                  {lastResult.title}
                </div>
                <div className="text-[11px] text-text-dim break-all font-mono">
                  {lastResult.url}
                </div>
              </div>
            )}

            {/* Click Navigation Outcome */}
            {lastResult.type === 'clicked' && (
              <div className="te-panel rounded-xs border-border p-3 space-y-2">
                <div className="flex items-center justify-between">
                  <span className="text-xs font-bold text-bone">CLICK DISPATCHED</span>
                  <span className={lastResult.navigated ? 'badge-green' : 'badge-dim'}>
                    NAVIGATED: {lastResult.navigated ? 'TRUE' : 'FALSE'}
                  </span>
                </div>
                {lastResult.url && (
                  <div className="text-[11px] text-text-dim break-all font-mono mt-1">
                    TARGET URL: {lastResult.url}
                  </div>
                )}
              </div>
            )}

            {/* Screenshot Image Preview */}
            {lastResult.type === 'image' && (
              <div className="space-y-2">
                <div className="text-[10px] text-text-muted uppercase">
                  SCREENSHOT CAPTURE (PNG):
                </div>
                <div className="border border-border rounded-xs overflow-hidden bg-bg">
                  <img
                    src={`data:image/png;base64,${lastResult.png_base64}`}
                    alt="Page Screenshot"
                    className="w-full object-contain"
                  />
                </div>
              </div>
            )}

            {/* CSS Found Elements */}
            {lastResult.type === 'elements' && (
              <div className="space-y-2">
                <div className="text-[10px] text-text-muted uppercase">
                  MATCHING SNAPSHOT NODES ({lastResult.refs.length}):
                </div>
                <div className="flex flex-wrap gap-1">
                  {lastResult.refs.map((r) => (
                    <span key={r} className="badge-orange font-bold">
                      {r}
                    </span>
                  ))}
                </div>
              </div>
            )}

            {/* Cookies */}
            {lastResult.type === 'cookies' && (
              <div className="space-y-2">
                <div className="text-[10px] text-text-muted uppercase">
                  ACTIVE COOKIE JAR ({lastResult.cookies.length}):
                </div>
                <div className="space-y-1 max-h-96 overflow-y-auto">
                  {lastResult.cookies.map((c, i) => (
                    <div key={i} className="p-2 bg-surface border border-border rounded-xs text-[11px]">
                      <div className="text-accent font-bold">{c.name}</div>
                      <div className="text-text-muted truncate">{c.domain}</div>
                    </div>
                  ))}
                </div>
              </div>
            )}

            {/* Flag / Done */}
            {lastResult.type === 'flag' && (
              <div className="te-panel rounded-xs border-border p-3">
                <span className="text-bone">FLAG OUTCOME: </span>
                <span className={lastResult.ok ? 'text-emerald-400 font-bold' : 'text-danger font-bold'}>
                  {lastResult.ok ? 'TRUE' : 'FALSE'}
                </span>
              </div>
            )}

            {lastResult.type === 'done' && (
              <div className="te-panel rounded-xs border-border p-3 text-emerald-400 font-bold">
                ✓ ACTION COMPLETED SUCCESSFULLY
              </div>
            )}
          </div>
        )}
      </div>
    </div>
  );
}
