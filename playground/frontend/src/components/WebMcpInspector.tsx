import React, { useState, useEffect } from 'react';

interface Props {
  onGetTools: () => Promise<any>;
  onInvokeTool: (name: string, argsJson: string) => Promise<any>;
  loading: boolean;
  disabled: boolean;
  currentUrl?: string;
}

interface WebMcpToolItem {
  name: string;
  description?: string;
  parameters?: any;
}

export function WebMcpInspector({ onGetTools, onInvokeTool, loading, disabled, currentUrl }: Props) {
  const [tools, setTools] = useState<WebMcpToolItem[]>([]);
  const [selectedTool, setSelectedTool] = useState<string | null>(null);
  const [argsJson, setArgsJson] = useState('{}');
  const [invocationResult, setInvocationResult] = useState<any | null>(null);
  const [fetching, setFetching] = useState(false);
  const [invoking, setInvoking] = useState(false);
  const [statusMsg, setStatusMsg] = useState<string | null>(null);

  const loadTools = async () => {
    if (disabled) return;
    setFetching(true);
    setStatusMsg(null);
    try {
      const res = await onGetTools();
      const list = Array.isArray(res) ? res : res?.tools || [];
      setTools(list);
      if (list.length > 0) {
        setSelectedTool(list[0].name);
        setStatusMsg(`Discovered ${list.length} page WebMCP tool(s)`);
      } else {
        setSelectedTool(null);
        setStatusMsg('Zero WebMCP tools declared by this page');
      }
    } catch (err: any) {
      setStatusMsg(`Tool probe error: ${err.message}`);
    } finally {
      setFetching(false);
    }
  };

  useEffect(() => {
    loadTools();
  }, [disabled, currentUrl]);

  const handleInvoke = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!selectedTool || disabled) return;
    setInvoking(true);
    setInvocationResult(null);
    try {
      const res = await onInvokeTool(selectedTool, argsJson);
      setInvocationResult(res);
      setStatusMsg(`Tool "${selectedTool}" executed successfully`);
    } catch (err: any) {
      setInvocationResult({ error: err.message || 'Invocation failed' });
      setStatusMsg(`Invocation error: ${err.message}`);
    } finally {
      setInvoking(false);
    }
  };

  const activeToolObj = tools.find((t) => t.name === selectedTool);

  return (
    <div className="flex flex-col h-full space-y-3 font-mono text-xs">
      {/* Header */}
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-2">
          <span className="font-bold text-bone">WEBMCP TOOL DISCOVERY</span>
          <span className="badge-orange">{tools.length} TOOLS</span>
        </div>
        <button
          onClick={loadTools}
          disabled={disabled || loading || fetching}
          className="btn-primary py-0.5 px-2.5 text-[10px]"
        >
          {fetching ? 'PROBING...' : '↻ DISCOVER'}
        </button>
      </div>

      {statusMsg && (
        <div className="text-[10px] text-accent border border-border bg-surface px-2 py-1 rounded-xs">
          {statusMsg}
        </div>
      )}

      {/* Main Content */}
      <div className="flex-1 min-h-[220px] bg-bg border border-border rounded-xs overflow-y-auto p-2 space-y-2.5 shadow-te-inset">
        {tools.length === 0 ? (
          <div className="p-6 text-center text-text-dim text-[11px] space-y-2">
            <div className="text-bone font-bold uppercase">NO WEBMCP TOOLS DETECTED</div>
            <p className="leading-relaxed">
              WebMCP enables web applications to declare callable agent tools via <code className="text-accent">navigator.modelContext</code>.
            </p>
            <p className="text-[10px] text-text-muted">
              When visiting WebMCP-enabled websites, agents can list and invoke domain-specific capabilities directly without parsing HTML.
            </p>
          </div>
        ) : (
          <>
            {/* Tool Selection Tabs */}
            <div className="flex items-center gap-1 overflow-x-auto pb-1">
              {tools.map((t) => (
                <button
                  key={t.name}
                  onClick={() => {
                    setSelectedTool(t.name);
                    setInvocationResult(null);
                  }}
                  className={`px-2 py-1 rounded-xs text-[11px] font-bold tracking-wider transition-colors shrink-0 ${
                    selectedTool === t.name
                      ? 'bg-accent text-white'
                      : 'bg-surface hover:bg-surface-elevated text-bone border border-border'
                  }`}
                >
                  ⚡ {t.name}
                </button>
              ))}
            </div>

            {/* Active Tool Spec Card */}
            {activeToolObj && (
              <div className="p-2.5 bg-card border border-border-strong rounded-xs space-y-2">
                <div className="flex items-center justify-between">
                  <span className="font-bold text-accent text-[11px]">{activeToolObj.name}</span>
                  <span className="badge-dim uppercase text-[9px]">WEBMCP DISPATCH</span>
                </div>
                {activeToolObj.description && (
                  <p className="text-[11px] text-text-dim font-sans">{activeToolObj.description}</p>
                )}

                {/* Parameters Schema */}
                {activeToolObj.parameters && (
                  <div className="space-y-1">
                    <span className="text-[10px] text-text-muted uppercase font-bold">SCHEMA:</span>
                    <pre className="p-2 bg-bg border border-border rounded-xs text-[10px] text-emerald-400 overflow-x-auto max-h-32">
                      {JSON.stringify(activeToolObj.parameters, null, 2)}
                    </pre>
                  </div>
                )}

                {/* Invocation Form */}
                <form onSubmit={handleInvoke} className="space-y-2 pt-1 border-t border-border">
                  <div className="flex items-center justify-between">
                    <label className="text-[10px] text-text-muted uppercase font-bold">ARGUMENTS (JSON):</label>
                    <span className="text-[9px] text-text-muted">Pass valid JSON object</span>
                  </div>
                  <textarea
                    value={argsJson}
                    onChange={(e) => setArgsJson(e.target.value)}
                    rows={3}
                    className="w-full bg-bg border border-border rounded-xs p-2 font-mono text-[11px] text-bone focus:outline-none focus:border-accent"
                    placeholder="{}"
                  />
                  <button
                    type="submit"
                    disabled={loading || invoking || disabled}
                    className="btn-primary w-full py-1 text-xs font-bold uppercase tracking-wider"
                  >
                    {invoking ? 'INVOKING TOOL...' : `EXECUTE ${activeToolObj.name} ›`}
                  </button>
                </form>

                {/* Invocation Result */}
                {invocationResult && (
                  <div className="p-2 bg-surface border border-border rounded-xs space-y-1 mt-2">
                    <div className="flex items-center justify-between text-[10px] font-bold text-bone">
                      <span>INVOCATION RETURN:</span>
                      <button
                        onClick={() => setInvocationResult(null)}
                        className="text-text-muted hover:text-danger text-[10px] underline cursor-pointer"
                      >
                        ✕ CLEAR
                      </button>
                    </div>
                    <pre className="p-2 bg-bg border border-border rounded-xs text-[10px] text-emerald-400 overflow-x-auto max-h-40">
                      {JSON.stringify(invocationResult, null, 2)}
                    </pre>
                  </div>
                )}
              </div>
            )}
          </>
        )}
      </div>
    </div>
  );
}
