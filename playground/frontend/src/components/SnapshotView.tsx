import React, { useState, useMemo } from 'react';
import type { Snapshot, SnapshotNode } from '../lib/types';
import type { ActivityLogItem } from '../hooks/useSession';

interface Props {
  snapshot: Snapshot | null;
  liveScreenshot: string | null;
  rawSourceBytes: number;
  activityLogs: ActivityLogItem[];
  onElementClick: (node: SnapshotNode) => void;
  selectedRef: string | null;
  loading: boolean;
}

type ViewMode = 'SPLIT' | 'CANVAS' | 'NODES' | 'LOGS';
type RoleFilter = 'ALL' | 'LINKS' | 'INPUTS' | 'BUTTONS' | 'OTHER';

export function SnapshotView({
  snapshot,
  liveScreenshot,
  rawSourceBytes,
  activityLogs,
  onElementClick,
  selectedRef,
  loading,
}: Props) {
  const [viewMode, setViewMode] = useState<ViewMode>('SPLIT');
  const [filter, setFilter] = useState<RoleFilter>('ALL');
  const [searchQuery, setSearchQuery] = useState('');

  const elements = snapshot?.elements || [];

  // Filter elements by category and search query
  const filteredElements = useMemo(() => {
    return elements.filter((el) => {
      if (filter === 'LINKS' && el.role !== 'link') return false;
      if (filter === 'INPUTS' && el.role !== 'textbox' && el.role !== 'combobox' && el.role !== 'checkbox' && el.role !== 'radio') return false;
      if (filter === 'BUTTONS' && el.role !== 'button') return false;
      if (filter === 'OTHER' && (el.role === 'link' || el.role === 'button' || el.role === 'textbox' || el.role === 'combobox')) return false;

      if (searchQuery.trim()) {
        const q = searchQuery.toLowerCase();
        const matchesRef = el.ref.toLowerCase().includes(q);
        const matchesName = (el.name || '').toLowerCase().includes(q);
        const matchesRole = el.role.toLowerCase().includes(q);
        const matchesVal = (el.value || '').toLowerCase().includes(q);
        return matchesRef || matchesName || matchesRole || matchesVal;
      }
      return true;
    });
  }, [elements, filter, searchQuery]);

  const estimatedTokens = elements.length ? Math.round(elements.length * 12 + 30) : 0;

  if (!snapshot) {
    return (
      <div className="flex-1 min-h-0 flex flex-col items-center justify-center p-8 text-center select-none bg-bg">
        <div className="te-panel rounded-xs border-dashed border-border p-8 max-w-md space-y-3">
          <div className="w-8 h-8 mx-auto rounded-full bg-surface border border-border flex items-center justify-center font-mono text-accent text-sm">
            ◎
          </div>
          <div className="font-mono text-xs font-bold text-bone uppercase">
            AWAITING BROWSER RUNTIME STREAM
          </div>
          <p className="text-xs text-text-dim font-sans leading-relaxed">
            Select a verified preset scenario above or click "+ NEW BROWSER NODE" to stream live visual renders and perception nodes.
          </p>
        </div>
      </div>
    );
  }

  return (
    <div className="flex-1 min-h-0 flex flex-col overflow-hidden font-mono text-xs select-none bg-bg">
      {/* Studio Viewport Header */}
      <div className="p-2 border-b border-border bg-card flex flex-wrap items-center justify-between gap-2 shrink-0">
        {/* URL and Title info */}
        <div className="min-w-0 max-w-md">
          <div className="flex items-center gap-2">
            <span className={`w-2 h-2 rounded-full ${loading ? 'bg-accent animate-ping' : 'bg-emerald-400'}`} />
            <span className="font-bold text-bone text-xs truncate" title={snapshot.title}>
              {snapshot.title || '(no title)'}
            </span>
          </div>
          <div className="text-[11px] text-text-dim truncate mt-0.5 font-mono" title={snapshot.url}>
            {snapshot.url}
          </div>
        </div>

        {/* View Mode Switcher */}
        <div className="flex items-center gap-1 bg-surface border border-border p-0.5 rounded-xs text-[10px]">
          <button
            onClick={() => setViewMode('SPLIT')}
            className={`px-2.5 py-1 rounded-xs transition-colors ${
              viewMode === 'SPLIT' ? 'bg-accent text-white font-bold' : 'text-text-dim hover:text-text'
            }`}
          >
            SPLIT VIEW
          </button>
          <button
            onClick={() => setViewMode('CANVAS')}
            className={`px-2.5 py-1 rounded-xs transition-colors ${
              viewMode === 'CANVAS' ? 'bg-accent text-white font-bold' : 'text-text-dim hover:text-text'
            }`}
          >
            LIVE CANVAS
          </button>
          <button
            onClick={() => setViewMode('NODES')}
            className={`px-2.5 py-1 rounded-xs transition-colors ${
              viewMode === 'NODES' ? 'bg-accent text-white font-bold' : 'text-text-dim hover:text-text'
            }`}
          >
            NODES ({elements.length})
          </button>
          <button
            onClick={() => setViewMode('LOGS')}
            className={`px-2.5 py-1 rounded-xs transition-colors ${
              viewMode === 'LOGS' ? 'bg-accent text-white font-bold' : 'text-text-dim hover:text-text'
            }`}
          >
            LOGS ({activityLogs.length})
          </button>
        </div>

        {/* Token telemetry meter */}
        <div className="flex items-center gap-2">
          <div className="badge-yellow">
            ~{estimatedTokens} TOKENS
          </div>
          <div className="badge-dim">
            {rawSourceBytes > 0 && estimatedTokens > 0
              ? `-${((1 - estimatedTokens / Math.round(rawSourceBytes / 4)) * 100).toFixed(1)}% VS DOM`
              : estimatedTokens > 0 ? 'MEASURING...' : 'IDLE'}
          </div>
        </div>
      </div>

      {/* Main Studio Viewport Body */}
      <div className="flex-1 min-h-0 flex flex-col md:flex-row overflow-hidden">
        {/* Visual Live Canvas (Shown in SPLIT or CANVAS mode) */}
        {(viewMode === 'SPLIT' || viewMode === 'CANVAS') && (
          <div className={`flex flex-col min-h-0 border-b md:border-b-0 md:border-r border-border bg-[#0a0a0c] overflow-hidden ${
            viewMode === 'SPLIT' ? 'flex-1 md:w-1/2' : 'flex-1'
          }`}>
            <div className="px-3 py-1.5 border-b border-border/80 bg-surface/90 flex items-center justify-between text-[10px] shrink-0">
              <span className="flex items-center gap-1.5 text-bone font-bold uppercase">
                <span className="w-1.5 h-1.5 rounded-full bg-emerald-400 animate-pulse" />
                LIVE BROWSER RENDER VIEWPORT
              </span>
              <span className="text-text-muted">REAL-TIME CHROMIUM SCREEN</span>
            </div>

            <div className="flex-1 min-h-0 overflow-y-auto p-3 flex items-start justify-center bg-[radial-gradient(#1f1f26_1px,transparent_1px)] bg-[size:16px_16px]">
              {liveScreenshot ? (
                <div className="border border-border-strong rounded-xs shadow-2xl overflow-hidden max-w-full relative group">
                  <img
                    src={`data:image/png;base64,${liveScreenshot}`}
                    alt="Live Browser Page"
                    className="max-w-full h-auto object-contain block"
                  />
                  <div className="absolute top-2 right-2 bg-bg/85 backdrop-blur px-2 py-0.5 rounded-xs border border-border text-[9px] font-mono text-emerald-400">
                    LIVE STREAM
                  </div>
                </div>
              ) : (
                <div className="p-8 text-center text-text-dim space-y-2 my-auto">
                  <div className="text-xs font-bold text-bone">STREAMING BROWSER SCREEN...</div>
                  <p className="text-[11px] font-sans">Capturing visual page render...</p>
                </div>
              )}
            </div>
          </div>
        )}

        {/* Perception Nodes Tree (Shown in SPLIT or NODES mode) */}
        {(viewMode === 'SPLIT' || viewMode === 'NODES') && (
          <div className={`flex flex-col min-h-0 overflow-hidden bg-bg ${
            viewMode === 'SPLIT' ? 'flex-1 md:w-1/2' : 'flex-1'
          }`}>
            {/* Filter and Search Bar */}
            <div className="px-3 py-1.5 border-b border-border bg-surface/70 flex flex-wrap items-center justify-between gap-2 text-[10px] shrink-0">
              <div className="flex items-center gap-1">
                {(['ALL', 'LINKS', 'INPUTS', 'BUTTONS', 'OTHER'] as const).map((cat) => (
                  <button
                    key={cat}
                    onClick={() => setFilter(cat)}
                    className={`px-2 py-0.5 rounded-xs transition-colors ${
                      filter === cat
                        ? 'bg-accent text-white font-bold'
                        : 'bg-card text-text-dim hover:text-text border border-border'
                    }`}
                  >
                    {cat}
                  </button>
                ))}
              </div>

              <div className="flex items-center gap-1">
                <input
                  type="text"
                  value={searchQuery}
                  onChange={(e) => setSearchQuery(e.target.value)}
                  placeholder="Filter nodes..."
                  className="input-sm py-0.5 text-[11px] w-32"
                />
              </div>
            </div>

            {/* Elements list - Fully scrollable */}
            <div className="flex-1 min-h-0 overflow-y-auto p-2 space-y-1">
              {filteredElements.length === 0 ? (
                <div className="p-6 text-center text-text-dim">
                  <p className="text-xs">No elements matching filter.</p>
                </div>
              ) : (
                filteredElements.map((el) => {
                  const isSelected = el.ref === selectedRef;
                  return (
                    <div
                      key={el.ref}
                      onClick={() => onElementClick(el)}
                      className={`p-1.5 px-2 rounded-xs border cursor-pointer transition-all flex items-center gap-2.5 ${
                        isSelected
                          ? 'bg-accent/15 border-accent shadow-xs'
                          : 'bg-card border-border hover:border-border-strong hover:bg-surface/60'
                      }`}
                    >
                      <span className={`w-11 text-center py-0.5 rounded-xs text-[11px] font-bold font-mono ${
                        isSelected ? 'bg-accent text-white' : 'bg-surface border border-border text-accent'
                      }`}>
                        {el.ref}
                      </span>

                      <span className="text-[9px] uppercase font-bold text-text-muted px-1 py-0.5 bg-bg border border-border rounded-xs w-14 text-center truncate">
                        {el.role}
                      </span>

                      <div className="flex-1 min-w-0">
                        <span className="text-bone font-sans font-medium text-xs truncate block">
                          {el.name || <span className="text-text-muted italic">(unnamed)</span>}
                        </span>
                        {el.value && (
                          <span className="text-[10px] font-mono text-yellow truncate block">
                            val: "{el.value}"
                          </span>
                        )}
                      </div>

                      <span className="text-[10px] text-accent font-mono font-bold opacity-0 group-hover:opacity-100">
                        SELECT ›
                      </span>
                    </div>
                  );
                })
              )}
            </div>
          </div>
        )}

        {/* Live Activity Logs (Shown in LOGS mode) */}
        {viewMode === 'LOGS' && (
          <div className="flex-1 min-h-0 flex flex-col overflow-hidden bg-[#0d0d10] p-4">
            <div className="text-xs font-bold text-bone mb-3 flex items-center justify-between pb-2 border-b border-border shrink-0">
              <span>LIVE AUTOMATION EVENT STREAM</span>
              <span className="text-[10px] text-text-muted font-normal">FIFO QUEUE (LAST 50 EVENTS)</span>
            </div>
            <div className="flex-1 min-h-0 overflow-y-auto space-y-1.5 font-mono text-xs">
              {activityLogs.length === 0 ? (
                <div className="text-text-muted p-4">No events logged yet.</div>
              ) : (
                activityLogs.map((log) => (
                  <div key={log.id} className="p-2 bg-surface/50 border border-border rounded-xs flex items-start gap-3">
                    <span className="text-text-muted text-[10px] whitespace-nowrap mt-0.5">{log.time}</span>
                    <span className={`px-1.5 py-0.5 rounded-xs text-[9px] font-bold uppercase ${
                      log.type === 'NAVIGATE' ? 'bg-blue-950/50 text-blue-400 border border-blue-800' :
                      log.type === 'CLICK' ? 'bg-amber-950/50 text-amber-400 border border-amber-800' :
                      log.type === 'EXTRACT' ? 'bg-emerald-950/50 text-emerald-400 border border-emerald-800' :
                      log.type === 'ERROR' ? 'bg-rose-950/50 text-rose-400 border border-rose-800' :
                      'bg-purple-950/50 text-purple-400 border border-purple-800'
                    }`}>
                      {log.type}
                    </span>
                    <div className="flex-1 min-w-0">
                      <div className="text-bone">{log.summary}</div>
                      {log.details && <div className="text-text-dim text-[11px] mt-0.5 break-all">{log.details}</div>}
                    </div>
                  </div>
                ))
              )}
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
