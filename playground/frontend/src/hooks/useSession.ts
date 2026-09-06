import { useState, useEffect, useCallback } from 'react';
import { snapshot, click, fill, selectOption, pressKey, extract, source, screenshot,
  evalText, findByCss, navigate, waitForUrl, getCookies, setCookie, getDownloads, batch,
  setFileChooser } from '../lib/actions';
import type { SessionId, Snapshot, ElementRef, ActionResult, Cookie, SnapshotNode } from '../lib/types';

export function useSession(sid: SessionId | null) {
  const [snapshotData, setSnapshotData] = useState<Snapshot | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [lastResult, setLastResult] = useState<ActionResult | null>(null);
  const [selectedRef, setSelectedRef] = useState<ElementRef | null>(null);
  const [selectedNode, setSelectedNode] = useState<SnapshotNode | null>(null);

  const refresh = useCallback(async () => {
    if (!sid) return;
    setLoading(true);
    try {
      const result = await snapshot(sid);
      setSnapshotData(result.snapshot);
      setError(null);
    } catch (e: any) {
      setError(e.message);
    } finally {
      setLoading(false);
    }
  }, [sid]);

  const runAction = useCallback(async (actionName: string, fn: () => Promise<any>) => {
    if (!sid) return;
    setLoading(true);
    setError(null);
    try {
      const result = await fn();
      setLastResult(result as ActionResult);
      // Auto-refresh on navigation-causing actions
      if (result?.type === 'navigated' || result?.type === 'clicked') {
        setTimeout(refresh, 300);
      } else if (result?.type === 'snapshot') {
        setSnapshotData(result.snapshot);
      }
      return result;
    } catch (e: any) {
      setError(e.message);
    } finally {
      setLoading(false);
    }
  }, [sid, refresh]);

  // Auto-refresh snapshot on mount
  useEffect(() => {
    if (sid) refresh();
  }, [sid, refresh]);

  // Action methods — each wrapped in runAction
  const actions = {
    click: (ref: ElementRef) => runAction('click', () => click(sid!, ref)),
    fill: (ref: ElementRef, text: string) => runAction('fill', () => fill(sid!, ref, text)),
    selectOption: (ref: ElementRef, value: string) => runAction('select_option', () => selectOption(sid!, ref, value)),
    pressKey: (key: string) => runAction('press_key', () => pressKey(sid!, key)),
    findByCss: (selector: string) => runAction('find_by_css', () => findByCss(sid!, selector)),
    evalText: (expr: string) => runAction('eval_text', () => evalText(sid!, expr)),
    extract: () => runAction('extract', () => extract(sid!)),
    source: () => runAction('source', () => source(sid!)),
    screenshot: () => runAction('screenshot', () => screenshot(sid!)),
    navigate: (url: string) => runAction('navigate', () => navigate(sid!, url)),
    waitForUrl: (pattern: string) => runAction('wait_for_url', () => waitForUrl(sid!, pattern)),
    setFileChooser: (ref: ElementRef, paths: string[]) => runAction('set_file_chooser', () => setFileChooser(sid!, ref, paths)),
    getCookies: () => runAction('cookies', () => getCookies(sid!) as any),
    setCookie: (cookie: Cookie) => runAction('set_cookie', () => setCookie(sid!, cookie)),
    getDownloads: () => runAction('downloads', () => getDownloads(sid!)),
    batch: (actions: unknown[]) => runAction('batch', () => batch(sid!, actions)),
  };

  return {
    sid,
    snapshot: snapshotData,
    loading,
    error,
    lastResult,
    selectedRef,
    selectedNode: selectedNode ? {
      role: selectedNode.role,
      name: selectedNode.name,
      value: selectedNode.value,
    } : null,
    setSelectedRef,
    actions,
    refresh,
  };
}
