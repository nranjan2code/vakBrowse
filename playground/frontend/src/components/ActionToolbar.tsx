import React, { useState } from 'react';
import type { ElementRef, ActionResult } from '../lib/types';

export interface ToolbarCallbacks {
  click: () => Promise<ActionResult | undefined>;
  fill: (text: string) => Promise<ActionResult | undefined>;
  selectOption: (value: string) => Promise<ActionResult | undefined>;
  pressKey: (key: string) => Promise<ActionResult | undefined>;
  findCss: (selector: string) => Promise<ActionResult | undefined>;
  evalText: (expr: string) => Promise<ActionResult | undefined>;
  extract: () => Promise<ActionResult | undefined>;
  source: () => Promise<ActionResult | undefined>;
  screenshot: () => Promise<ActionResult | undefined>;
  navigate: (url: string) => Promise<ActionResult | undefined>;
  setFileChooser: (paths: string[]) => Promise<ActionResult | undefined>;
  cookies: () => Promise<ActionResult | undefined>;
  downloads: () => Promise<ActionResult | undefined>;
  batch: (actions: unknown[]) => Promise<unknown[] | undefined>;
}

interface Props {
  selectedRef: ElementRef | null;
  selectedNode: { role: string; name: string; value: string | null } | null;
  cb: ToolbarCallbacks;
  loading: boolean;
  lastResult: ActionResult | null;
  onClear: () => void;
  currentUrl?: string;
}

export function ActionToolbar({
  selectedRef,
  selectedNode,
  cb,
  loading,
  lastResult,
  onClear,
  currentUrl,
}: Props) {
  const [navUrl, setNavUrl] = useState(currentUrl || '');
  const [fillText, setFillText] = useState('');
  const [keyInput, setKeyInput] = useState('Enter');
  const [evalExpr, setEvalExpr] = useState('navigator.webdriver');
  const [cssSelector, setCssSelector] = useState('button, a');
  const [batchModalOpen, setBatchModalOpen] = useState(false);
  const [batchJson, setBatchJson] = useState('[\n  {"type":"extract"}\n]');

  const handleNavSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    if (navUrl.trim()) cb.navigate(navUrl.trim());
  };

  const handleFillSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    if (fillText) cb.fill(fillText);
  };

  const handleBatchSubmit = async () => {
    try {
      const parsed = JSON.parse(batchJson);
      await cb.batch(parsed);
      setBatchModalOpen(false);
    } catch (err) {
      alert('Invalid JSON in batch request');
    }
  };

  return (
    <div className="border-b border-border bg-card p-3 space-y-3 font-mono text-xs select-none">
      {/* Row 1: Hardware Navigation Bar */}
      <div className="flex flex-wrap items-center gap-2">
        <form onSubmit={handleNavSubmit} className="flex-1 min-w-[280px] flex items-center gap-1.5">
          <span className="text-[11px] text-text-muted">URL:</span>
          <input
            type="text"
            value={navUrl}
            onChange={(e) => setNavUrl(e.target.value)}
            placeholder="https://..."
            className="input-sm flex-1 font-mono text-xs text-bone"
            disabled={loading}
          />
          <button
            type="submit"
            disabled={loading || !navUrl}
            className="btn-primary"
            title="Navigate to URL"
          >
            NAVIGATE ›
          </button>
        </form>

        {/* Core Perception Actions */}
        <div className="flex items-center gap-1.5 border-l border-border pl-2">
          <button
            onClick={() => cb.extract()}
            disabled={loading}
            className="btn-success"
            title="Extract readability main-content text"
          >
            EXTRACT TEXT
          </button>
          <button
            onClick={() => cb.screenshot()}
            disabled={loading}
            className="btn-secondary"
            title="Capture PNG screenshot"
          >
            SCREENSHOT
          </button>
          <button
            onClick={() => cb.source()}
            disabled={loading}
            className="btn-ghost"
            title="Inspect raw HTML source"
          >
            SOURCE
          </button>
        </div>
      </div>

      {/* Row 2: Selected Element Staged Target & Interaction Deck */}
      <div className="flex flex-wrap items-center justify-between gap-2 p-2 bg-surface/70 border border-border rounded-xs">
        <div className="flex items-center gap-2">
          <span className="text-[10px] text-text-muted uppercase tracking-wider">
            STAGED TARGET:
          </span>
          {selectedRef ? (
            <div className="flex items-center gap-2">
              <span className="badge-orange font-bold">{selectedRef}</span>
              <span className="text-text-dim text-[11px]">[{selectedNode?.role}]</span>
              {selectedNode?.name && (
                <span className="text-bone font-sans font-medium text-xs max-w-[180px] truncate">
                  "{selectedNode.name}"
                </span>
              )}
              <button
                onClick={onClear}
                className="text-text-muted hover:text-danger text-[10px] uppercase font-bold ml-1"
                title="Deselect target"
              >
                ✕
              </button>
            </div>
          ) : (
            <span className="text-text-muted text-[11px] italic font-sans">
              Click any element in snapshot below to stage for action
            </span>
          )}
        </div>

        {/* Action triggers */}
        <div className="flex items-center gap-1.5 flex-wrap">
          <button
            onClick={() => cb.click()}
            disabled={loading || !selectedRef}
            className="btn-primary"
            title="Dispatch trusted mouse click at box-model center"
          >
            CLICK {selectedRef || ''}
          </button>

          {/* Fill Input field */}
          <form onSubmit={handleFillSubmit} className="flex items-center gap-1">
            <input
              type="text"
              value={fillText}
              onChange={(e) => setFillText(e.target.value)}
              placeholder="Fill value..."
              disabled={loading || !selectedRef}
              className="input-sm w-32"
            />
            <button
              type="submit"
              disabled={loading || !selectedRef || !fillText}
              className="btn-secondary"
            >
              FILL
            </button>
          </form>

          {/* Key Press */}
          <div className="flex items-center gap-1">
            <select
              value={keyInput}
              onChange={(e) => setKeyInput(e.target.value)}
              disabled={loading}
              className="input-sm bg-bg text-bone py-1"
            >
              <option value="Enter">Enter</option>
              <option value="Tab">Tab</option>
              <option value="Escape">Escape</option>
              <option value="ArrowDown">Down</option>
              <option value="ArrowUp">Up</option>
            </select>
            <button
              onClick={() => cb.pressKey(keyInput)}
              disabled={loading}
              className="btn-secondary"
              title="Dispatch keyboard key"
            >
              KEY
            </button>
          </div>
        </div>
      </div>

      {/* Row 3: Extended Tools (CSS Search, Eval, Batch, State) */}
      <div className="flex flex-wrap items-center justify-between gap-2 text-[11px] pt-1">
        <div className="flex items-center gap-2 flex-wrap">
          {/* CSS Resolver */}
          <div className="flex items-center gap-1">
            <span className="text-text-muted text-[10px]">CSS:</span>
            <input
              type="text"
              value={cssSelector}
              onChange={(e) => setCssSelector(e.target.value)}
              placeholder="selector"
              className="input-sm w-28 py-0.5"
            />
            <button
              onClick={() => cb.findCss(cssSelector)}
              disabled={loading || !cssSelector}
              className="btn-ghost py-0.5"
            >
              FIND
            </button>
          </div>

          {/* Eval */}
          <div className="flex items-center gap-1">
            <span className="text-text-muted text-[10px]">JS:</span>
            <input
              type="text"
              value={evalExpr}
              onChange={(e) => setEvalExpr(e.target.value)}
              placeholder="expression"
              className="input-sm w-36 py-0.5"
            />
            <button
              onClick={() => cb.evalText(evalExpr)}
              disabled={loading || !evalExpr}
              className="btn-ghost py-0.5"
            >
              EVAL
            </button>
          </div>
        </div>

        <div className="flex items-center gap-1">
          <button
            onClick={() => cb.cookies()}
            disabled={loading}
            className="btn-ghost py-0.5"
          >
            COOKIES
          </button>
          <button
            onClick={() => cb.downloads()}
            disabled={loading}
            className="btn-ghost py-0.5"
          >
            DOWNLOADS
          </button>
          <button
            onClick={() => setBatchModalOpen(true)}
            disabled={loading}
            className="btn-orange-outline py-0.5"
          >
            BATCH JSON ⚡
          </button>
        </div>
      </div>

      {/* Batch Modal */}
      {batchModalOpen && (
        <div className="fixed inset-0 bg-black/75 backdrop-blur-xs flex items-center justify-center p-4 z-50">
          <div className="te-panel rounded-xs border-accent p-5 w-full max-w-lg space-y-4 shadow-2xl">
            <div className="flex items-center justify-between pb-2 border-b border-border">
              <span className="font-mono text-xs font-bold text-bone uppercase flex items-center gap-1.5">
                <span className="w-2 h-2 rounded-xs bg-accent" />
                DISPATCH BATCH ACTIONS (FAIL-FAST)
              </span>
              <button
                onClick={() => setBatchModalOpen(false)}
                className="text-text-dim hover:text-text"
              >
                ✕
              </button>
            </div>
            <p className="text-[11px] text-text-dim font-sans">
              Send an array of action payloads executed sequentially in a single round-trip:
            </p>
            <textarea
              value={batchJson}
              onChange={(e) => setBatchJson(e.target.value)}
              rows={6}
              className="w-full bg-bg border border-border rounded-xs p-3 font-mono text-xs text-emerald-400 focus:outline-none focus:border-accent"
            />
            <div className="flex justify-end gap-2">
              <button
                onClick={() => setBatchModalOpen(false)}
                className="btn-ghost"
              >
                CANCEL
              </button>
              <button
                onClick={handleBatchSubmit}
                disabled={loading}
                className="btn-primary"
              >
                DISPATCH BATCH ›
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
