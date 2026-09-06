import React, { useState } from 'react';
import type { ElementRef, ActionResult } from '../lib/types';

// Callback bridge — the parent SessionView wires these to the useSession hook.
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
}

export function ActionToolbar({ selectedRef, selectedNode, cb, loading, lastResult, onClear }: Props) {
  return (
    <div className="border-b border-border p-3 bg-card space-y-2">
      {/* Selection row */}
      {selectedRef && (
        <div className="flex items-center gap-2 text-sm">
          <span className="text-accent font-mono">Selected: {selectedRef}</span>
          <span className="text-text-dim">role={selectedNode?.role}</span>
          {selectedNode?.name && <span className="text-text-dim">name="{selectedNode.name}"</span>}
          <button
            onClick={onClear}
            className="px-2 py-0.5 bg-border hover:bg-accent/20 rounded text-xs"
          >
            Clear
          </button>
        </div>
      )}

      {/* Primary actions row */}
      <div className="flex flex-wrap gap-2 items-center">
        <button
          onClick={cb.click}
          disabled={loading || !selectedRef}
          className="btn-primary"
          title="Click the selected element"
        >
          Click
        </button>

        <FillField disabled={loading || !selectedRef} onSubmit={cb.fill} />
        <SelectField disabled={loading || !selectedRef} onSubmit={cb.selectOption} />
        <PressKeyField disabled={loading} onSubmit={cb.pressKey} />
        <CssField disabled={loading} onSubmit={cb.findCss} />
        <EvalField disabled={loading} onSubmit={cb.evalText} />

        <div className="border-l border-border h-6 mx-2" />

        <button onClick={cb.extract} disabled={loading} className="btn-success" title="Extract readable content">
          Extract
        </button>
        <button onClick={cb.source} disabled={loading} className="btn-ghost" title="View page HTML source">
          Source
        </button>
        <button onClick={cb.screenshot} disabled={loading} className="btn-ghost" title="Take screenshot">
          Screenshot
        </button>
        <FileInputField disabled={loading || !selectedRef} onSubmit={cb.setFileChooser} />

        <div className="border-l border-border h-6 mx-2" />

        <NavigateField disabled={loading} onSubmit={cb.navigate} />
        <button onClick={cb.cookies} disabled={loading} className="btn-ghost" title="List cookies">
          Cookies
        </button>
        <button onClick={cb.downloads} disabled={loading} className="btn-ghost" title="List downloads">
          Downloads
        </button>
        <BatchButton disabled={loading} onSubmit={cb.batch} />

        {lastResult && (
          <div className="ml-auto text-xs text-text-dim max-w-xs truncate">
            last: {formatResult(lastResult)}
          </div>
        )}
      </div>
    </div>
  );
}

function formatResult(r: ActionResult): string {
  switch (r.type) {
    case 'navigated': return `→ ${r.url?.slice(0, 40) ?? ''}`;
    case 'clicked': return `clicked nav=${r.navigated}`;
    case 'elements': return `${r.refs.length} found`;
    case 'flag': return `ok=${r.ok}`;
    case 'done': return 'done';
    case 'text': return `"${r.text.slice(0, 40)}"`;
    case 'image': return 'png';
    case 'tabs': return `${r.tabs.length} tabs`;
    case 'tools': return `${r.tools.length} tools`;
    default: return r.type;
  }
}

type SubmitFn<T> = (value: T) => Promise<unknown>;

function FillField({ disabled, onSubmit }: { disabled: boolean; onSubmit: SubmitFn<string> }) {
  const [text, setText] = useState('');
  return (
    <div className="flex items-center gap-1">
      <input type="text" placeholder="fill text..." value={text}
        onChange={(e) => setText(e.target.value)}
        className="input-sm" disabled={disabled} />
      <button onClick={() => { onSubmit(text); setText(''); }}
        disabled={disabled || !text} className="btn-primary text-xs">Fill
      </button>
    </div>
  );
}

function SelectField({ disabled, onSubmit }: { disabled: boolean; onSubmit: SubmitFn<string> }) {
  const [value, setValue] = useState('');
  return (
    <div className="flex items-center gap-1">
      <input type="text" placeholder="option value..." value={value}
        onChange={(e) => setValue(e.target.value)}
        className="input-sm w-32" disabled={disabled} />
      <button onClick={() => { onSubmit(value); setValue(''); }}
        disabled={disabled || !value} className="btn-primary text-xs">Select
      </button>
    </div>
  );
}

function PressKeyField({ disabled, onSubmit }: { disabled: boolean; onSubmit: SubmitFn<string> }) {
  const [key, setKey] = useState('Enter');
  return (
    <div className="flex items-center gap-1">
      <input type="text" value={key} onChange={(e) => setKey(e.target.value)}
        className="input-sm w-20" disabled={disabled} />
      <button onClick={() => onSubmit(key)} disabled={disabled} className="btn-primary text-xs">Key</button>
    </div>
  );
}

function CssField({ disabled, onSubmit }: { disabled: boolean; onSubmit: SubmitFn<string> }) {
  const [selector, setSelector] = useState('a');
  return (
    <div className="flex items-center gap-1">
      <input type="text" value={selector} onChange={(e) => setSelector(e.target.value)}
        className="input-sm w-32" disabled={disabled} />
      <button onClick={() => { onSubmit(selector); setSelector('a'); }}
        disabled={disabled} className="btn-primary text-xs">Find</button>
    </div>
  );
}

function EvalField({ disabled, onSubmit }: { disabled: boolean; onSubmit: SubmitFn<string> }) {
  const [expr, setExpr] = useState('navigator.webdriver');
  return (
    <div className="flex items-center gap-1">
      <input type="text" value={expr} onChange={(e) => setExpr(e.target.value)}
        className="input-sm w-48" disabled={disabled} />
      <button onClick={() => { onSubmit(expr); }}
        disabled={disabled} className="btn-primary text-xs">Eval</button>
    </div>
  );
}

function FileInputField({ disabled, onSubmit }: { disabled: boolean; onSubmit: SubmitFn<string[]> }) {
  const [paths, setPaths] = useState('');
  return (
    <div className="flex items-center gap-1">
      <input type="text" placeholder="/path/to/file,s2" value={paths}
        onChange={(e) => setPaths(e.target.value)}
        className="input-sm w-32" disabled={disabled} />
      <button onClick={() => { onSubmit(paths.split(',')); setPaths(''); }}
        disabled={disabled || !paths} className="btn-primary text-xs">Upload</button>
    </div>
  );
}

function NavigateField({ disabled, onSubmit }: { disabled: boolean; onSubmit: SubmitFn<string> }) {
  const [url, setUrl] = useState('https://example.com');
  return (
    <div className="flex items-center gap-1">
      <input type="url" placeholder="https://..." value={url}
        onChange={(e) => setUrl(e.target.value)}
        className="input-sm w-48" disabled={disabled} />
      <button onClick={() => onSubmit(url)} disabled={disabled || !url}
        className="btn-primary text-xs">Go</button>
    </div>
  );
}

function BatchButton({ disabled, onSubmit }: { disabled: boolean; onSubmit: SubmitFn<unknown[]> }) {
  const [json, setJson] = useState('');
  const [show, setShow] = useState(false);
  return (
    <>
      <button onClick={() => setShow(true)} disabled={disabled} className="btn-ghost">
        Batch
      </button>
      {show && (
        <div className="fixed inset-0 bg-black/50 flex items-center justify-center z-50">
          <div className="bg-card border border-border rounded p-4 w-96">
            <h3 className="text-text font-medium mb-2">Batch Actions (JSON array)</h3>
            <textarea
              value={json}
              onChange={(e) => setJson(e.target.value)}
              placeholder='[{"type":"navigate","url":"https://example.com"},{"type":"extract"}]'
              className="w-full h-24 px-2 py-1 bg-bg border border-border rounded text-sm text-text font-mono resize-none"
            />
            <div className="flex gap-2 mt-2 justify-end">
              <button onClick={() => setShow(false)} className="px-3 py-1 bg-border rounded text-sm">
                Cancel
              </button>
              <button
                onClick={() => {
                  try {
                    const actions = JSON.parse(json);
                    onSubmit(actions);
                    setShow(false);
                  } catch {
                    alert('Invalid JSON');
                  }
                }}
                disabled={disabled || !json}
                className="px-3 py-1 btn-primary text-sm"
              >
                Run
              </button>
            </div>
          </div>
        </div>
      )}
    </>
  );
}
