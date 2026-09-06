import React, { useState } from 'react';

interface Props {
  onRunBatch: (actions: any[]) => Promise<any>;
  loading: boolean;
  disabled: boolean;
  activeRef: string | null;
}

interface BatchStep {
  id: string;
  type: 'navigate' | 'click' | 'fill' | 'press_key' | 'extract' | 'eval_text';
  url?: string;
  ref?: string;
  text?: string;
  key?: string;
  expression?: string;
}

const PRESET_BATCHES: { name: string; description: string; steps: BatchStep[] }[] = [
  {
    name: 'Wikipedia AI Extract',
    description: 'Navigate to Wikipedia AI article and extract formatted readability text in 1 round-trip.',
    steps: [
      { id: '1', type: 'navigate', url: 'https://en.wikipedia.org/wiki/Artificial_intelligence' },
      { id: '2', type: 'extract' },
    ],
  },
  {
    name: 'Stealth Security Probe',
    description: 'Probe webdriver flag and user agent in a single atomic verification.',
    steps: [
      { id: '1', type: 'eval_text', expression: 'navigator.webdriver' },
      { id: '2', type: 'eval_text', expression: 'navigator.userAgent' },
    ],
  },
  {
    name: 'HN Frontpage Digest',
    description: 'Navigate to Hacker News and read top stories in 1 round-trip.',
    steps: [
      { id: '1', type: 'navigate', url: 'https://news.ycombinator.com' },
      { id: '2', type: 'extract' },
    ],
  },
];

export function BatchStudio({ onRunBatch, loading, disabled, activeRef }: Props) {
  const [steps, setSteps] = useState<BatchStep[]>([
    { id: '1', type: 'navigate', url: 'https://en.wikipedia.org/wiki/Artificial_intelligence' },
    { id: '2', type: 'extract' },
  ]);
  const [results, setResults] = useState<any[] | null>(null);
  const [running, setRunning] = useState(false);
  const [errorMsg, setErrorMsg] = useState<string | null>(null);

  const addStep = (type: BatchStep['type']) => {
    const newStep: BatchStep = {
      id: Math.random().toString(36).substring(2, 9),
      type,
      url: type === 'navigate' ? 'https://example.com' : undefined,
      ref: (type === 'click' || type === 'fill') ? (activeRef || '@e1') : undefined,
      text: type === 'fill' ? 'sample input' : undefined,
      key: type === 'press_key' ? 'Enter' : undefined,
      expression: type === 'eval_text' ? 'document.title' : undefined,
    };
    setSteps([...steps, newStep]);
  };

  const removeStep = (id: string) => {
    setSteps(steps.filter((s) => s.id !== id));
  };

  const updateStep = (id: string, updates: Partial<BatchStep>) => {
    setSteps(steps.map((s) => (s.id === id ? { ...s, ...updates } : s)));
  };

  const handleExecute = async () => {
    if (disabled || steps.length === 0) return;
    setRunning(true);
    setErrorMsg(null);
    setResults(null);

    // Format for Rust backend Request::Batch
    const formattedActions = steps.map((s) => {
      if (s.type === 'navigate') return { type: 'navigate', url: s.url };
      if (s.type === 'click') return { type: 'click', ref: s.ref };
      if (s.type === 'fill') return { type: 'fill', ref: s.ref, text: s.text };
      if (s.type === 'press_key') return { type: 'press_key', key: s.key || 'Enter' };
      if (s.type === 'extract') return { type: 'extract' };
      if (s.type === 'eval_text') return { type: 'eval_text', expression: s.expression };
      return { type: s.type };
    });

    try {
      const res = await onRunBatch(formattedActions);
      setResults(Array.isArray(res) ? res : res?.results || []);
    } catch (err: any) {
      setErrorMsg(err.message || 'Batch execution failed');
    } finally {
      setRunning(false);
    }
  };

  return (
    <div className="flex flex-col h-full space-y-3 font-mono text-xs">
      {/* Top Header & Presets */}
      <div>
        <div className="flex items-center justify-between mb-1.5">
          <span className="font-bold text-bone">AUTOMATED BATCH STUDIO</span>
          <div className="flex items-center gap-2">
            <span className="text-[10px] text-emerald-400 font-bold">ATOMIC FAIL-FAST</span>
            {steps.length > 0 && (
              <button
                onClick={() => {
                  setSteps([]);
                  setResults(null);
                  setErrorMsg(null);
                }}
                className="text-[10px] text-text-muted hover:text-danger underline cursor-pointer"
                title="Wipe all steps from this recipe"
              >
                ✕ CLEAR
              </button>
            )}
          </div>
        </div>
        <div className="flex items-center gap-1.5 flex-wrap">
          <span className="text-[10px] text-text-muted">PRESET:</span>
          {PRESET_BATCHES.map((p) => (
            <button
              key={p.name}
              onClick={() => {
                setSteps(p.steps);
                setResults(null);
                setErrorMsg(null);
              }}
              className="px-2 py-0.5 bg-surface hover:bg-surface-elevated border border-border hover:border-accent text-text-dim hover:text-bone rounded-xs text-[10px] transition-colors"
              title={p.description}
            >
              {p.name}
            </button>
          ))}
        </div>
      </div>

      {/* Steps List */}
      <div className="flex-1 min-h-[220px] bg-bg border border-border rounded-xs p-2 overflow-y-auto space-y-2 shadow-te-inset">
        {steps.length === 0 ? (
          <div className="p-8 text-center text-text-dim text-[11px] space-y-2">
            <div className="text-bone font-bold">RECIPE WORKSPACE IS CLEAN</div>
            <p className="text-text-muted">
              Choose a preset recipe above or click any action below to build a pipeline from scratch.
            </p>
          </div>
        ) : (
          steps.map((step, index) => (
            <div key={step.id} className="p-2 bg-card border border-border-strong rounded-xs space-y-1.5">
              <div className="flex items-center justify-between">
                <div className="flex items-center gap-1.5">
                  <span className="w-5 h-5 rounded-full bg-accent/20 border border-accent text-accent text-[10px] flex items-center justify-center font-bold">
                    {index + 1}
                  </span>
                  <span className="font-bold text-bone uppercase text-[11px]">{step.type}</span>
                </div>
              <button
                onClick={() => removeStep(step.id)}
                className="text-text-muted hover:text-danger text-[11px] px-1"
                title="Remove step"
              >
                ✕
              </button>
            </div>

            {/* Step Field Inputs */}
            {step.type === 'navigate' && (
              <input
                type="text"
                value={step.url || ''}
                onChange={(e) => updateStep(step.id, { url: e.target.value })}
                placeholder="https://..."
                className="input-sm w-full text-[11px]"
              />
            )}

            {step.type === 'click' && (
              <div className="flex items-center gap-1.5">
                <span className="text-[10px] text-text-muted">TARGET REF:</span>
                <input
                  type="text"
                  value={step.ref || ''}
                  onChange={(e) => updateStep(step.id, { ref: e.target.value })}
                  placeholder="@e1"
                  className="input-sm w-24 text-[11px]"
                />
              </div>
            )}

            {step.type === 'fill' && (
              <div className="flex items-center gap-1.5">
                <input
                  type="text"
                  value={step.ref || ''}
                  onChange={(e) => updateStep(step.id, { ref: e.target.value })}
                  placeholder="@e1"
                  className="input-sm w-20 text-[11px]"
                />
                <input
                  type="text"
                  value={step.text || ''}
                  onChange={(e) => updateStep(step.id, { text: e.target.value })}
                  placeholder="Text to fill..."
                  className="input-sm flex-1 text-[11px]"
                />
              </div>
            )}

            {step.type === 'press_key' && (
              <select
                value={step.key || 'Enter'}
                onChange={(e) => updateStep(step.id, { key: e.target.value })}
                className="input-sm text-[11px] bg-surface"
              >
                <option value="Enter">Enter</option>
                <option value="Tab">Tab</option>
                <option value="Escape">Escape</option>
                <option value="ArrowDown">ArrowDown</option>
              </select>
            )}

            {step.type === 'eval_text' && (
              <input
                type="text"
                value={step.expression || ''}
                onChange={(e) => updateStep(step.id, { expression: e.target.value })}
                placeholder="navigator.webdriver"
                className="input-sm w-full text-[11px]"
              />
            )}
          </div>
        )))}

        {/* Add Step Action Bar */}
        <div className="flex flex-wrap items-center gap-1 pt-1">
          <span className="text-[10px] text-text-muted">+ ADD:</span>
          <button onClick={() => addStep('navigate')} className="btn-ghost py-0.5 px-1.5 text-[10px]">+ NAVIGATE</button>
          <button onClick={() => addStep('click')} className="btn-ghost py-0.5 px-1.5 text-[10px]">+ CLICK</button>
          <button onClick={() => addStep('fill')} className="btn-ghost py-0.5 px-1.5 text-[10px]">+ FILL</button>
          <button onClick={() => addStep('press_key')} className="btn-ghost py-0.5 px-1.5 text-[10px]">+ KEY</button>
          <button onClick={() => addStep('extract')} className="btn-ghost py-0.5 px-1.5 text-[10px]">+ EXTRACT</button>
          <button onClick={() => addStep('eval_text')} className="btn-ghost py-0.5 px-1.5 text-[10px]">+ EVAL</button>
        </div>
      </div>

      {/* Error Alert */}
      {errorMsg && (
        <div className="text-[11px] text-danger border border-danger/40 bg-danger/10 p-2 rounded-xs">
          {errorMsg}
        </div>
      )}

      {/* Run Button */}
      <button
        onClick={handleExecute}
        disabled={disabled || loading || running || steps.length === 0}
        className="btn-primary py-1.5 w-full text-xs font-bold uppercase tracking-wider"
      >
        {running ? 'EXECUTING PIPELINE...' : `DISPATCH BATCH (${steps.length} STEPS) ⚡`}
      </button>

      {/* Results View */}
      {results && (
        <div className="p-2.5 bg-surface border border-border rounded-xs space-y-1.5">
          <div className="flex items-center justify-between text-[10px] font-bold text-bone">
            <span>PIPELINE RESULTS:</span>
            <div className="flex items-center gap-2">
              <span className="text-emerald-400">ALL STEPS PASSED</span>
              <button
                onClick={() => setResults(null)}
                className="text-text-muted hover:text-danger text-[10px] underline cursor-pointer"
                title="Dismiss batch results"
              >
                ✕ CLEAR
              </button>
            </div>
          </div>
          <pre className="p-2 bg-bg border border-border rounded-xs text-[10px] text-emerald-400 overflow-x-auto max-h-40">
            {JSON.stringify(results, null, 2)}
          </pre>
        </div>
      )}
    </div>
  );
}
