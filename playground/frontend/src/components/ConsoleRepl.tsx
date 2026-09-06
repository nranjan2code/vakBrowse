import React, { useState } from 'react';

interface Props {
  onEval: (expr: string) => Promise<any>;
  loading: boolean;
  disabled: boolean;
}

interface HistoryItem {
  id: string;
  expr: string;
  result: any;
  error?: string;
  elapsedMs: number;
  time: string;
}

const COMMON_PROBES = [
  { label: 'navigator.webdriver', expr: 'navigator.webdriver' },
  { label: 'document.title', expr: 'document.title' },
  { label: 'window.location.href', expr: 'window.location.href' },
  { label: 'document.cookie', expr: 'document.cookie' },
  { label: 'navigator.userAgent', expr: 'navigator.userAgent' },
  { label: 'document.links.length', expr: 'document.links.length' },
];

export function ConsoleRepl({ onEval, loading, disabled }: Props) {
  const [inputExpr, setInputExpr] = useState('navigator.webdriver');
  const [history, setHistory] = useState<HistoryItem[]>([]);
  const [evaluating, setEvaluating] = useState(false);

  const handleSubmit = async (e?: React.FormEvent, customExpr?: string) => {
    if (e) e.preventDefault();
    const expr = (customExpr || inputExpr).trim();
    if (!expr || disabled) return;

    setEvaluating(true);
    const start = Date.now();
    try {
      const res = await onEval(expr);
      const elapsed = Date.now() - start;
      const val = typeof res === 'object' && res !== null && 'text' in res ? res.text : res;
      setHistory((prev) => [
        {
          id: Math.random().toString(36).substring(2, 9),
          expr,
          result: val,
          elapsedMs: elapsed,
          time: new Date().toLocaleTimeString('en-US', { hour12: false }),
        },
        ...prev,
      ]);
    } catch (err: any) {
      const elapsed = Date.now() - start;
      setHistory((prev) => [
        {
          id: Math.random().toString(36).substring(2, 9),
          expr,
          result: null,
          error: err.message || 'Evaluation failed',
          elapsedMs: elapsed,
          time: new Date().toLocaleTimeString('en-US', { hour12: false }),
        },
        ...prev,
      ]);
    } finally {
      setEvaluating(false);
    }
  };

  return (
    <div className="flex flex-col h-full space-y-3 font-mono text-xs">
      {/* Quick Probe Presets */}
      <div>
        <div className="text-[10px] text-text-muted uppercase tracking-wider mb-1.5 flex items-center justify-between">
          <span>QUICK PROBE EXPRESSIONS:</span>
          <span className="text-emerald-400 font-bold">REAL-TIME EVAL_TEXT</span>
        </div>
        <div className="flex flex-wrap gap-1">
          {COMMON_PROBES.map((p) => (
            <button
              key={p.label}
              onClick={() => {
                setInputExpr(p.expr);
                handleSubmit(undefined, p.expr);
              }}
              disabled={disabled || loading || evaluating}
              className="px-2 py-0.5 bg-surface hover:bg-surface-elevated border border-border hover:border-accent text-text-dim hover:text-bone rounded-xs text-[10px] transition-colors"
            >
              {p.label}
            </button>
          ))}
        </div>
      </div>

      {/* Input Bar */}
      <form onSubmit={handleSubmit} className="flex items-center gap-1.5">
        <div className="relative flex-1">
          <span className="absolute left-2.5 top-1/2 -translate-y-1/2 text-accent font-bold">&gt;</span>
          <input
            type="text"
            value={inputExpr}
            onChange={(e) => setInputExpr(e.target.value)}
            placeholder="Enter JS expression (e.g. document.title, 1+1)..."
            disabled={disabled || loading || evaluating}
            className="input-sm w-full pl-6 text-bone font-mono text-xs py-1"
          />
        </div>
        <button
          type="submit"
          disabled={disabled || loading || evaluating || !inputExpr.trim()}
          className="btn-primary py-1 px-3 text-xs shrink-0"
        >
          {evaluating ? 'RUNNING...' : 'EVAL ›'}
        </button>
      </form>

      {/* Terminal History Header & Clear */}
      <div className="flex items-center justify-between text-[10px] text-text-muted">
        <span>V8 EXECUTION CONSOLE:</span>
        {history.length > 0 && (
          <button
            onClick={() => setHistory([])}
            className="hover:text-danger text-[10px] underline cursor-pointer"
            title="Clear console output"
          >
            ✕ CLEAR CONSOLE
          </button>
        )}
      </div>

      {/* History / Output Terminal */}
      <div className="flex-1 min-h-[220px] bg-bg border border-border rounded-xs p-2 overflow-y-auto space-y-2 shadow-te-inset">
        {history.length === 0 ? (
          <div className="p-4 text-center text-text-dim text-[11px] space-y-1">
            <div className="text-bone font-bold">JS EVALUATION REPL READY</div>
            <p>Dispatches directly into the running browser's V8 execution context via CDP <code className="text-accent font-mono">Action::EvalText</code>.</p>
          </div>
        ) : (
          history.map((h) => (
            <div key={h.id} className="border-b border-border/50 pb-2 space-y-1">
              <div className="flex items-center justify-between text-[10px] text-text-muted">
                <span className="text-accent font-bold flex items-center gap-1">
                  <span>&gt;</span>
                  <span className="text-bone">{h.expr}</span>
                </span>
                <span>{h.elapsedMs}ms • {h.time}</span>
              </div>
              {h.error ? (
                <div className="text-danger text-[11px] pl-2 border-l border-danger">
                  Error: {h.error}
                </div>
              ) : (
                <pre className="text-emerald-400 text-[11px] pl-2 border-l border-emerald-500/40 whitespace-pre-wrap break-all">
                  {typeof h.result === 'string' ? h.result : JSON.stringify(h.result, null, 2)}
                </pre>
              )}
            </div>
          ))
        )}
      </div>
    </div>
  );
}
