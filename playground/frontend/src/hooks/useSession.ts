import { useState, useEffect, useCallback } from 'react';
import { snapshot, click, fill, selectOption, pressKey, extract, source, screenshot,
  evalText, findByCss, navigate, waitForUrl, getCookies, setCookie, getDownloads, batch,
  setFileChooser } from '../lib/actions';
import type { SessionId, Snapshot, ElementRef, ActionResult, Cookie, SnapshotNode } from '../lib/types';

export interface ActivityLogItem {
  id: string;
  time: string;
  type: 'OPEN' | 'NAVIGATE' | 'CLICK' | 'FILL' | 'EXTRACT' | 'SNAPSHOT' | 'EVAL' | 'BATCH' | 'ERROR';
  summary: string;
  details?: string;
}

export interface BreadcrumbItem {
  id: string;
  url: string;
  title?: string;
  time: string;
}

export function useSession(sid: SessionId | null) {
  const [snapshotData, setSnapshotData] = useState<Snapshot | null>(null);
  const [liveScreenshot, setLiveScreenshot] = useState<string | null>(null);
  const [liveHtml, setLiveHtml] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [lastResult, setLastResult] = useState<ActionResult | null>(null);
  const [selectedRef, setSelectedRef] = useState<ElementRef | null>(null);
  const [selectedNode, setSelectedNode] = useState<SnapshotNode | null>(null);
  const [activityLogs, setActivityLogs] = useState<ActivityLogItem[]>([]);
  const [historyTrail, setHistoryTrail] = useState<BreadcrumbItem[]>([]);

  const addLog = useCallback((type: ActivityLogItem['type'], summary: string, details?: string) => {
    const item: ActivityLogItem = {
      id: Math.random().toString(36).substring(2, 9),
      time: new Date().toLocaleTimeString('en-US', { hour12: false }),
      type,
      summary,
      details,
    };
    setActivityLogs((prev) => [item, ...prev.slice(0, 49)]);
  }, []);

  const refresh = useCallback(async () => {
    if (!sid) return;
    setLoading(true);
    try {
      const result = await snapshot(sid);
      setSnapshotData(result.snapshot);
      setError(null);
      addLog('SNAPSHOT', `Perceived ${result.snapshot.elements.length} nodes (<${Math.round(result.snapshot.elements.length * 12 + 30)} tokens)`);

      // Track URL in breadcrumb trail
      if (result.snapshot.url) {
        setHistoryTrail((prev) => {
          if (prev.length > 0 && prev[prev.length - 1].url === result.snapshot.url) return prev;
          return [
            ...prev,
            {
              id: Math.random().toString(36).substring(2, 9),
              url: result.snapshot.url,
              title: result.snapshot.title || result.snapshot.url,
              time: new Date().toLocaleTimeString('en-US', { hour12: false }),
            },
          ].slice(-15);
        });
      }
      
      // Auto-fetch visual representation:
      // In CDP: chromium provides real live PNG screenshot
      // In DOM: pure-Rust headless engine provides real live HTML source
      try {
        const img = await screenshot(sid);
        if (img && (img as any).png_base64) {
          setLiveScreenshot((img as any).png_base64);
          setLiveHtml(null);
        } else if (typeof img === 'string') {
          setLiveScreenshot(img);
          setLiveHtml(null);
        }
      } catch (_) {
        try {
          const src = await source(sid);
          let rawHtml = (src as any)?.text || (src as any)?.html;
          if (rawHtml && typeof rawHtml === 'string') {
            const pageUrl = result.snapshot.url;
            const baseTag = pageUrl && (pageUrl.startsWith('http://') || pageUrl.startsWith('https://'))
              ? `<base href="${pageUrl}">`
              : '';
            if (baseTag) {
              if (rawHtml.includes('<head>')) {
                rawHtml = rawHtml.replace('<head>', `<head>${baseTag}`);
              } else if (rawHtml.includes('<head ')) {
                rawHtml = rawHtml.replace(/<head\b[^>]*>/, `$&${baseTag}`);
              } else {
                rawHtml = baseTag + rawHtml;
              }
            }
            setLiveHtml(rawHtml);
            setLiveScreenshot(null);
          }
        } catch (_) {}
      }
    } catch (e: any) {
      setError(e.message);
      addLog('ERROR', `Snapshot failed: ${e.message}`);
    } finally {
      setLoading(false);
    }
  }, [sid, addLog]);

  const runAction = useCallback(async (actionName: string, fn: () => Promise<any>) => {
    if (!sid) {
      addLog('ERROR', `Cannot execute ${actionName.toUpperCase()}: No active browser session`);
      setError(`Cannot execute ${actionName}: No active browser session. Launch or navigate first.`);
      return;
    }
    setLoading(true);
    setError(null);
    const start = Date.now();
    try {
      const result = await fn();
      const elapsed = Date.now() - start;
      setLastResult(result as ActionResult);

      // Log action with details
      if (result?.type === 'navigated') {
        addLog('NAVIGATE', `Navigated to ${result.url} (${elapsed}ms)`);
      } else if (result?.type === 'clicked') {
        addLog('CLICK', `Clicked element -> Navigated: ${result.navigated ? 'YES' : 'NO'} (${elapsed}ms)`, result.url || undefined);
      } else if (result?.type === 'text') {
        addLog('EXTRACT', `Content extracted (${result.text.length} chars in ${elapsed}ms)`);
      } else if (result?.type === 'image') {
        setLiveScreenshot(result.png_base64);
        addLog('SNAPSHOT', `Screenshot captured (${elapsed}ms)`);
      } else {
        addLog('NAVIGATE', `${actionName.toUpperCase()} completed in ${elapsed}ms`);
      }

      // Auto-refresh snapshot and live screenshot on navigation-causing actions
      if (
        actionName === 'navigate' ||
        actionName === 'back' ||
        actionName === 'forward' ||
        actionName === 'reload' ||
        result?.type === 'navigated' ||
        (result?.type === 'clicked' && result?.navigated)
      ) {
        // Automatically clean up stale element target refs on navigation
        setSelectedRef(null);
        setSelectedNode(null);
        await refresh();
      } else if (result?.type === 'snapshot') {
        setSnapshotData(result.snapshot);
      }

      return result;
    } catch (e: any) {
      setError(e.message);
      addLog('ERROR', `${actionName.toUpperCase()} failed: ${e.message}`);
    } finally {
      setLoading(false);
    }
  }, [sid, refresh, addLog]);

  // Auto-refresh snapshot on mount / session switch
  useEffect(() => {
    if (sid) {
      addLog('OPEN', `Session ${sid} connected`);
      setSelectedRef(null);
      setSelectedNode(null);
      setLastResult(null);
      refresh();
    } else {
      setSnapshotData(null);
      setLiveScreenshot(null);
      setSelectedRef(null);
      setSelectedNode(null);
      setLastResult(null);
      setHistoryTrail([]);
    }
  }, [sid, refresh, addLog]);

  // Action methods — each wrapped in runAction
  const actions = {
    back: () => runAction('back', () => back(sid!)),
    forward: () => runAction('forward', () => forward(sid!)),
    reload: () => runAction('reload', () => reload(sid!)),
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
    batch: (actionsList: unknown[]) => runAction('batch', () => batch(sid!, actionsList)),
  };

  return {
    sid,
    snapshot: snapshotData,
    liveScreenshot,
    liveHtml,
    loading,
    error,
    lastResult,
    selectedRef,
    selectedNode,
    activityLogs,
    historyTrail,
    setSelectedRef,
    setSelectedNode,
    setLastResult,
    clearLogs: () => setActivityLogs([]),
    actions,
    refresh,
  };
}
