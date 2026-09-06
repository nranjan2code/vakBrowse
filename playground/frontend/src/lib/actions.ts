import type { Snapshot, ElementRef, SessionId, ActionResult } from './types';

// The RPC endpoint that proxies the vakd-rest Request model.
const BASE = import.meta.env.DEV ? 'http://localhost:7788' : '';

export async function rpc(req: Record<string, unknown>): Promise<any> {
  const r = await fetch(`${BASE}/playground/rpc`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(req),
  });
  const json = await r.json();
  if (json.Ok && json.Ok.Error) {
    const kind = Object.keys(json.Ok.Error)[0];
    throw new Error(`${kind}: ${json.Ok.Error[kind]}`);
  }
  return json;
}

// --- typed convenience wrappers ---

export async function openSession(opts: Record<string, unknown>): Promise<{ id: string; url: string }> {
  const resp = await rpc({ type: 'open', options: { headless: true, ...opts } });
  return resp.Ok.Opened;
}

export async function closeSession(sid: string): Promise<boolean> {
  const resp = await rpc({ type: 'close', session: sid });
  return resp.Ok.Closed;
}

export async function listSessions(): Promise<any[]> {
  const resp = await rpc({ type: 'list_sessions' });
  return resp.Ok.Sessions;
}

export async function snapshot(sid: string): Promise<{ snapshot: Snapshot }> {
  const resp = await rpc({ type: 'act', session: sid, action: { type: 'snapshot' } });
  return resp.Ok.Result;
}

export async function click(sid: string, ref: ElementRef): Promise<ActionResult> {
  const resp = await rpc({ type: 'act', session: sid, action: { type: 'click', ref } });
  return resp.Ok.Result;
}

export async function fill(sid: string, ref: ElementRef, text: string): Promise<ActionResult> {
  const resp = await rpc({ type: 'act', session: sid, action: { type: 'fill', ref, text } });
  return resp.Ok.Result;
}

export async function selectOption(sid: string, ref: ElementRef, value: string): Promise<ActionResult> {
  const resp = await rpc({ type: 'act', session: sid, action: { type: 'select_option', ref, value } });
  return resp.Ok.Result;
}

export async function pressKey(sid: string, keyParam: string): Promise<ActionResult> {
  const resp = await rpc({ type: 'act', session: sid, action: { type: 'press_key', key: keyParam } });
  return resp.Ok.Result;
}

export async function extract(sid: string): Promise<ActionResult> {
  const resp = await rpc({ type: 'act', session: sid, action: { type: 'extract' } });
  return resp.Ok.Result;
}

export async function source(sid: string): Promise<ActionResult> {
  const resp = await rpc({ type: 'act', session: sid, action: { type: 'source' } });
  return resp.Ok.Result;
}

export async function screenshot(sid: string): Promise<ActionResult> {
  const resp = await rpc({ type: 'act', session: sid, action: { type: 'screenshot', full_page: false } });
  return resp.Ok.Result;
}

export async function evalText(sid: string, expr: string): Promise<ActionResult> {
  const resp = await rpc({ type: 'act', session: sid, action: { type: 'eval_text', expression: expr } });
  return resp.Ok.Result;
}

export async function findByCss(sid: string, selector: string): Promise<ActionResult> {
  const resp = await rpc({ type: 'act', session: sid, action: { type: 'find_by_css', selector } });
  return resp.Ok.Result;
}

export async function navigate(sid: string, url: string): Promise<ActionResult> {
  const resp = await rpc({ type: 'act', session: sid, action: { type: 'navigate', url } });
  return resp.Ok.Result;
}

export async function back(sid: string): Promise<ActionResult> {
  const resp = await rpc({ type: 'act', session: sid, action: { type: 'back' } });
  return resp.Ok.Result;
}

export async function forward(sid: string): Promise<ActionResult> {
  const resp = await rpc({ type: 'act', session: sid, action: { type: 'forward' } });
  return resp.Ok.Result;
}

export async function reload(sid: string): Promise<ActionResult> {
  const resp = await rpc({ type: 'act', session: sid, action: { type: 'reload' } });
  return resp.Ok.Result;
}

export async function waitForUrl(sid: string, pattern: string, timeoutMs = 5000): Promise<ActionResult> {
  const resp = await rpc({ type: 'act', session: sid, action: { type: 'wait_for_url', pattern, timeout_ms: timeoutMs } });
  return resp.Ok.Result;
}

export async function setFileChooser(sid: string, ref: ElementRef, paths: string[]): Promise<ActionResult> {
  const resp = await rpc({ type: 'act', session: sid, action: { type: 'set_file_chooser', ref, paths } });
  return resp.Ok.Result;
}

export async function getCookies(sid: string): Promise<any> {
  const resp = await rpc({ type: 'act', session: sid, action: { type: 'cookies' } });
  return resp.Ok.Result.cookies;
}

export async function setCookie(sid: string, cookie: any): Promise<ActionResult> {
  const resp = await rpc({ type: 'act', session: sid, action: { type: 'set_cookie', cookie } });
  return resp.Ok.Result;
}

export async function getDownloads(sid: string): Promise<any[]> {
  const resp = await rpc({ type: 'act', session: sid, action: { type: 'downloads' } });
  const text = resp.Ok.Result.text;
  return JSON.parse(text);
}

export async function batch(sid: string, actions: any[]): Promise<any[]> {
  const resp = await rpc({ type: 'batch', session: sid, actions });
  return resp.Ok.Results;
}
