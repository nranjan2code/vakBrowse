import { useState, useEffect, useCallback } from 'react';
import {
  snapshot, click, fill, selectOption, pressKey, extract, source, screenshot,
  evalText, findByCss, navigate, back, forward, reload, waitForUrl, waitForTruthy,
  getCookies, setCookie, clearCookies, getDownloads, setDownloadDir, batch,
  setFileChooser, scroll, clickAt, rotateProxy, webmcpTools, webmcpInvoke,
  listTabs, newTab, switchTab, closeTab,
} from '../lib/actions';
import type { SessionId, Snapshot, ElementRef, ActionResult, Cookie, SnapshotNode, TabInfo } from '../lib/types';

export interface ActivityLogItem {
  id: string;
  time: string;
  type: 'OPEN' | 'NAVIGATE' | 'CLICK' | 'FILL' | 'EXTRACT' | 'SNAPSHOT' | 'EVAL' | 'BATCH' | 'SCROLL' | 'CLICK_AT' | 'PROXY' | 'WEBMCP' | 'TABS' | 'ERROR';
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
  const [rawSourceBytes, setRawSourceBytes] = useState<number>(0);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [lastResult, setLastResult] = useState<ActionResult | null>(null);
  const [selectedRef, setSelectedRef] = useState<ElementRef | null>(null);
  const [selectedNode, setSelectedNode] = useState<SnapshotNode | null>(null);
  const [activityLogs, setActivityLogs] = useState<ActivityLogItem[]>([]);
  const [historyTrail, setHistoryTrail] = useState<BreadcrumbItem[]>([]);
  const [tabs, setTabs] = useState<TabInfo[]>([]);
  const [activeTabId, setActiveTabId] = useState<string | null>(null);

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
      
      // Auto-fetch live screenshot and raw source size in parallel
      // (both are non-critical — perception tree is the primary data)
      const [imgRes, srcRes] = await Promise.allSettled([
        screenshot(sid),
        source(sid),
      ]);

      if (imgRes.status === 'fulfilled') {
        const img = imgRes.value;
        if (img && (img as any).png_base64) {
          setLiveScreenshot((img as any).png_base64);
        } else if (typeof img === 'string') {
          setLiveScreenshot(img);
        }
      }

      // Measure real raw HTML source bytes for live token efficiency gauge
      if (srcRes.status === 'fulfilled') {
        const srcText = (srcRes.value as any)?.text;
        if (typeof srcText === 'string') {
          setRawSourceBytes(new Blob([srcText]).size);
        }
      }

      // Auto-sync session tabs
      try {
        const tabsRes = await listTabs(sid);
        if (tabsRes && (tabsRes as any).type === 'tabs') {
          const tList = (tabsRes as any).tabs || [];
          setTabs(tList);
          setActiveTabId((curr) => {
            if (curr && tList.some((t: TabInfo) => t.id === curr)) return curr;
            return tList.length > 0 ? tList[0].id : null;
          });
        }
      } catch (_) {}
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
      } else if (actionName === 'scroll') {
        addLog('SCROLL', `Scrolled viewport (${elapsed}ms)`);
      } else if (actionName === 'click_at') {
        addLog('CLICK_AT', `Vision click dispatched at coords (${elapsed}ms)`);
      } else if (actionName === 'rotate_proxy') {
        addLog('PROXY', `Rotated proxy endpoint (${elapsed}ms)`);
      } else {
        addLog('NAVIGATE', `${actionName.toUpperCase()} completed in ${elapsed}ms`);
      }

      // Determine if this action is purely read-only (no DOM mutation possible)
      const readOnlyActions = new Set([
        'extract', 'source', 'screenshot', 'cookies', 'set_cookie', 'clear_cookies',
        'downloads', 'set_download_dir', 'wait_for_url', 'wait_for_truthy',
        'web_mcp_tools', 'set_file_chooser',
      ]);

      // Navigation-causing actions: clear stale element refs
      const isNavigation =
        actionName === 'navigate' ||
        actionName === 'back' ||
        actionName === 'forward' ||
        actionName === 'reload' ||
        actionName === 'rotate_proxy' ||
        actionName === 'switch_tab' ||
        actionName === 'new_tab' ||
        actionName === 'close_tab' ||
        result?.type === 'navigated' ||
        (result?.type === 'clicked' && result?.navigated);

      if (isNavigation) {
        // Clear stale element target refs on navigation
        setSelectedRef(null);
        setSelectedNode(null);
        await refresh();
      } else if (!readOnlyActions.has(actionName)) {
        // Any mutation-causing action (click, fill, select, press_key, eval_text,
        // batch, scroll, click_at, find_by_css, web_mcp_invoke, etc.)
        // always refresh so the playground stays reactive to page changes
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
    waitForUrl: (pattern: string, timeoutMs?: number) => runAction('wait_for_url', () => waitForUrl(sid!, pattern, timeoutMs)),
    waitForTruthy: (expr: string, timeoutMs?: number) => runAction('wait_for_truthy', () => waitForTruthy(sid!, expr, timeoutMs)),
    setFileChooser: (ref: ElementRef, paths: string[]) => runAction('set_file_chooser', () => setFileChooser(sid!, ref, paths)),
    getCookies: () => runAction('cookies', () => getCookies(sid!) as any),
    setCookie: (cookie: Cookie) => runAction('set_cookie', () => setCookie(sid!, cookie)),
    clearCookies: () => runAction('clear_cookies', () => clearCookies(sid!)),
    getDownloads: () => runAction('downloads', () => getDownloads(sid!)),
    setDownloadDir: (dir: string) => runAction('set_download_dir', () => setDownloadDir(sid!, dir)),
    scroll: (dx: number, dy: number) => runAction('scroll', () => scroll(sid!, dx, dy)),
    clickAt: (x: number, y: number) => runAction('click_at', () => clickAt(sid!, x, y)),
    rotateProxy: () => runAction('rotate_proxy', () => rotateProxy(sid!)),
    webmcpTools: () => runAction('web_mcp_tools', () => webmcpTools(sid!)),
    webmcpInvoke: (name: string, argsJson: string) => runAction('web_mcp_invoke', () => webmcpInvoke(sid!, name, argsJson)),
    listTabs: () =>
      runAction('tabs', async () => {
        const res = await listTabs(sid!);
        if (res && (res as any).type === 'tabs') {
          setTabs((res as any).tabs || []);
        }
        return res;
      }),
    newTab: (url?: string) =>
      runAction('new_tab', async () => {
        const res = await newTab(sid!, url);
        if (res && (res as any).type === 'tab_opened') {
          setActiveTabId((res as any).tab.id);
        }
        return res;
      }),
    switchTab: (tab: string) =>
      runAction('switch_tab', async () => {
        setActiveTabId(tab);
        return await switchTab(sid!, tab);
      }),
    closeTab: (tab: string) =>
      runAction('close_tab', async () => {
        const res = await closeTab(sid!, tab);
        return res;
      }),
    batch: (actionsList: unknown[]) => runAction('batch', () => batch(sid!, actionsList)),
  };

  return {
    sid,
    snapshot: snapshotData,
    liveScreenshot,
    rawSourceBytes,
    loading,
    error,
    lastResult,
    selectedRef,
    selectedNode,
    activityLogs,
    historyTrail,
    tabs,
    activeTabId,
    setActiveTabId,
    setSelectedRef,
    setSelectedNode,
    setLastResult,
    clearLogs: () => setActivityLogs([]),
    actions,
    refresh,
  };
}
