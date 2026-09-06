import type { ActionResult } from '../lib/types';

interface Props {
  lastResult: ActionResult | null;
  loading: boolean;
}

export function ResultPane({ lastResult, loading }: Props) {
  if (!lastResult) {
    return (
      <div className="w-80 border-l border-border p-4 overflow-y-auto text-text-dim">
        <p>No actions performed yet.</p>
        <p className="mt-2 text-xs">
          Click an element or run an action to see results here.
        </p>
      </div>
    );
  }

  return (
    <div className="w-80 border-l border-border p-4 overflow-y-auto">
      <h3 className="text-sm font-medium text-text-dim mb-3 uppercase">
        Last Result
      </h3>
      {loading && <div className="text-xs text-text-dim mb-2">Loading...</div>}

      {lastResult.type === 'text' && (
        <pre className="whitespace-pre-wrap text-sm text-text bg-bg rounded p-3 border border-border overflow-y-auto max-h-96">
          {lastResult.text}
        </pre>
      )}

      {lastResult.type === 'navigated' && (
        <div className="space-y-1">
          <p className="text-sm"><span className="text-text-dim">Navigated to:</span> <span className="text-text break-all">{lastResult.url}</span></p>
          <p className="text-sm"><span className="text-text-dim">Title:</span> <span className="text-text">{lastResult.title}</span></p>
        </div>
      )}

      {lastResult.type === 'clicked' && (
        <div className="space-y-1">
          <p className="text-sm"><span className="text-text-dim">Navigated:</span> <span className={lastResult.navigated ? 'text-success' : 'text-danger'}>{lastResult.navigated ? 'yes' : 'no'}</span></p>
          {lastResult.url && <p className="text-sm text-text break-all">{lastResult.url}</p>}
        </div>
      )}

      {lastResult.type === 'elements' && (
        <div>
          <p className="text-sm text-text-dim mb-2">{lastResult.refs.length} element(s) found:</p>
          <div className="flex flex-wrap gap-1">
            {lastResult.refs.map((ref) => (
              <span key={ref} className="text-xs font-mono bg-accent/20 text-accent px-2 py-0.5 rounded">
                {ref}
              </span>
            ))}
          </div>
        </div>
      )}

      {lastResult.type === 'image' && (
        <img
          src={`data:image/png;base64,${lastResult.png_base64}`}
          alt="screenshot"
          className="max-w-full rounded border border-border"
        />
      )}

      {lastResult.type === 'flag' && (
        <p className="text-sm"><span className="text-text-dim">Flag:</span> <span className={lastResult.ok ? 'text-success' : 'text-danger'}>{lastResult.ok ? 'true' : 'false'}</span></p>
      )}

      {lastResult.type === 'tools' && (
        <div>
          <p className="text-sm text-text-dim mb-2">{lastResult.tools.length} WebMCP tool(s):</p>
          {lastResult.tools.map((tool) => (
            <div key={tool.name} className="mb-1">
              <p className="text-xs font-mono text-accent">{tool.name}</p>
              <p className="text-xs text-text-dim">{tool.description}</p>
            </div>
          ))}
        </div>
      )}

      {lastResult.type === 'tabs' && (
        <div>
          <p className="text-sm text-text-dim mb-2">{lastResult.tabs.length} tab(s):</p>
          {lastResult.tabs.map((tab) => (
            <p key={tab.id} className="text-xs font-mono text-text-dim">{tab.id}: {tab.url}</p>
          ))}
        </div>
      )}

      {lastResult.type === 'tab_opened' && (
        <p className="text-sm"><span className="text-text-dim">Tab opened:</span> <span className="text-text">{lastResult.tab.id}</span></p>
      )}

      {lastResult.type === 'done' && <p className="text-sm text-text-dim">Done.</p>}
    </div>
  );
}
