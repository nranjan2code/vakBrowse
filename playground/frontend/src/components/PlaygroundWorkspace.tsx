import React, { useState, useCallback, useEffect } from 'react';
import { useSessions } from '../hooks/useSessions';
import { useSession } from '../hooks/useSession';
import { PRESET_SCENARIOS, PresetScenario } from './PresetWorkflows';
import { ConsoleRepl } from './ConsoleRepl';
import { CookieManager } from './CookieManager';
import { DownloadsViewer } from './DownloadsViewer';
import { WebMcpInspector } from './WebMcpInspector';
import { BatchStudio } from './BatchStudio';
import { ExportCodeModal } from './ExportCodeModal';
import { TourGuide } from './TourGuide';
import { TokenSavingsGauge } from './TokenSavingsGauge';
import { TabBar } from './TabBar';
import { TabManager } from './TabManager';
import type { SnapshotNode } from '../lib/types';

export function PlaygroundWorkspace() {
  const { sessions, loading: sessionsLoading, error: sessionsError, open, close, reload } = useSessions();
  const [activeSession, setActiveSession] = useState<string | null>(null);
  const session = useSession(activeSession);

  // Omnibar state (empty when no session is active)
  const [urlInput, setUrlInput] = useState('');
  const [navigating, setNavigating] = useState(false);
  const [navError, setNavError] = useState<string | null>(null);

  // Target interaction state
  const [fillText, setFillText] = useState('');
  const [selectedKey, setSelectedKey] = useState('Enter');

  // View state
  const [viewMode, setViewMode] = useState<'SPLIT' | 'RENDER' | 'TREE'>('SPLIT');
  const [activeRightTab, setActiveRightTab] = useState<'OUTPUT' | 'CONSOLE' | 'TABS' | 'COOKIES' | 'DOWNLOADS' | 'WEBMCP' | 'BATCH' | 'WIRE_JSON' | 'LOGS'>('OUTPUT');
  const [showExportModal, setShowExportModal] = useState(false);
  const [showTourGuide, setShowTourGuide] = useState(false);

  // Interactive Live Canvas State
  const [visionClickMode, setVisionClickMode] = useState(true);
  const [clickRipple, setClickRipple] = useState<{ x: number; y: number; visible: boolean }>({ x: 0, y: 0, visible: false });

  // CSS Selector Resolver State
  const [cssQuery, setCssQuery] = useState('');
  const [cssMatches, setCssMatches] = useState<string[]>([]);
  const [cssSearching, setCssSearching] = useState(false);
  const [cssMessage, setCssMessage] = useState<string | null>(null);

  // Automated Preset Recipe Runner State
  const [selectedPreset, setSelectedPreset] = useState<PresetScenario | null>(null);
  const [recipeRunning, setRecipeRunning] = useState(false);
  const [recipeStepIndex, setRecipeStepIndex] = useState<number>(-1);

  // Modals
  const [showNewModal, setShowNewModal] = useState(false);
  const [newUrl, setNewUrl] = useState('https://en.wikipedia.org/wiki/Artificial_intelligence');
  const [newStealth, setNewStealth] = useState(true);

  const [modalLoading, setModalLoading] = useState(false);
  const [modalError, setModalError] = useState<string | null>(null);

  const [batchModalOpen, setBatchModalOpen] = useState(false);
  const [batchJson, setBatchJson] = useState('[\n  {"type":"extract"}\n]');

  // Auto-select first active session if none is selected
  useEffect(() => {
    if (!activeSession && sessions.length > 0) {
      setActiveSession(sessions[0].id);
    } else if (activeSession && !sessions.some((s) => s.id === activeSession)) {
      setActiveSession(sessions.length > 0 ? sessions[0].id : null);
    }
  }, [activeSession, sessions]);

  // Keep URL bar in sync with active session URL, clear if no session is open
  useEffect(() => {
    if (session.snapshot?.url) {
      setUrlInput(session.snapshot.url);
    } else if (!activeSession || sessions.length === 0) {
      setUrlInput('');
    }
  }, [session.snapshot?.url, activeSession, sessions.length]);

  // Universal Navigate or Open handler:
  const handleNavigate = useCallback(async (targetUrl?: string) => {
    let raw = (targetUrl || urlInput || '').trim();
    if (!raw) return;

    if (!raw.startsWith('http://') && !raw.startsWith('https://') && !raw.startsWith('file://') && !raw.startsWith('about:') && !raw.startsWith('data:')) {
      raw = `https://${raw}`;
    }

    setUrlInput(raw);
    setNavigating(true);
    setNavError(null);

    try {
      const isAlive = activeSession && sessions.some((s) => s.id === activeSession);
      if (!isAlive) {
        // Automatically spawn a new session with stealth
        const res = await open({
          url: raw,
          stealth: true,
        });
        if (res && typeof res === 'object' && 'id' in res) {
          setActiveSession((res as any).id);
        }
      } else {
        // Navigate existing session, with transparent failover if session was closed
        try {
          await session.actions.navigate(raw);
        } catch (_) {
          const res = await open({
            url: raw,
            stealth: true,
          });
          if (res && typeof res === 'object' && 'id' in res) {
            setActiveSession((res as any).id);
          }
        }
      }
    } catch (err: any) {
      setNavError(err.message || 'Failed to navigate to target URL');
    } finally {
      setNavigating(false);
    }
  }, [urlInput, activeSession, sessions, open, session.actions]);

  // Interactive Live Canvas Click-at coordinates dispatch
  const handleCanvasClick = async (e: React.MouseEvent<HTMLImageElement>) => {
    if (!activeSession || !session.liveScreenshot || navigating || session.loading) return;
    const img = e.currentTarget;
    const rect = img.getBoundingClientRect();
    const scaleX = img.naturalWidth / rect.width;
    const scaleY = img.naturalHeight / rect.height;
    const clientX = e.clientX - rect.left;
    const clientY = e.clientY - rect.top;
    const x = Math.round(clientX * scaleX);
    const y = Math.round(clientY * scaleY);

    setClickRipple({ x: clientX, y: clientY, visible: true });
    setTimeout(() => setClickRipple((prev) => ({ ...prev, visible: false })), 800);

    await session.actions.clickAt(x, y);
  };

  // Viewport scroll dispatcher
  const handleScroll = async (dy: number) => {
    if (!activeSession || navigating || session.loading) return;
    await session.actions.scroll(0, dy);
  };

  // CSS Selector resolver
  const handleFindCss = async (e?: React.FormEvent) => {
    if (e) e.preventDefault();
    if (!cssQuery.trim() || !activeSession || session.loading) return;
    setCssSearching(true);
    setCssMessage(null);
    try {
      const res = await session.actions.findByCss(cssQuery.trim());
      const refs = (res as any)?.refs?.map((r: any) => (typeof r === 'string' ? r : r[0] || r)) || [];
      setCssMatches(refs);
      if (refs.length > 0) {
        setCssMessage(`Found ${refs.length} element(s): ${refs.join(', ')}`);
        session.setSelectedRef(refs[0]);
        const node = session.snapshot?.elements.find((el) => el.ref === refs[0]);
        if (node) session.setSelectedNode(node);
      } else {
        setCssMessage(`No interactive elements match '${cssQuery.trim()}'`);
      }
    } catch (err: any) {
      setCssMessage(`CSS match error: ${err.message}`);
    } finally {
      setCssSearching(false);
    }
  };

  // Launch preset scenario
  const handleLaunchPreset = useCallback(async (scenario: PresetScenario) => {
    setSelectedPreset(scenario);
    setUrlInput(scenario.initialUrl);
    setNavigating(true);
    setNavError(null);
    try {
      const res = await open({
        url: scenario.initialUrl,
        stealth: scenario.stealth,
      });
      if (res && typeof res === 'object' && 'id' in res) {
        setActiveSession((res as any).id);
      }
    } catch (err: any) {
      setNavError(err.message || 'Failed to launch preset');
    } finally {
      setNavigating(false);
    }
  }, [open]);

  // Automated step-by-step recipe runner
  const handleExecuteRecipe = async () => {
    if (!selectedPreset || recipeRunning) return;
    setRecipeRunning(true);
    setRecipeStepIndex(0);
    setNavError(null);

    try {
      // Step 1: Open / Navigate
      await handleNavigate(selectedPreset.initialUrl);
      setRecipeStepIndex(1);
      await new Promise((r) => setTimeout(r, 600));

      if (selectedPreset.id === 'wikipedia-research' || selectedPreset.id === 'hn-intelligence') {
        setRecipeStepIndex(2);
        await session.actions.extract();
        setActiveRightTab('OUTPUT');
      } else if (selectedPreset.id === 'example-link-proof') {
        setRecipeStepIndex(2);
        session.setSelectedRef('@e1');
        await session.actions.click('@e1');
        setActiveRightTab('OUTPUT');
      } else if (selectedPreset.id === 'wikipedia-form-search') {
        setRecipeStepIndex(2);
        const searchRef = session.snapshot?.elements.find((e) => e.role === 'textbox')?.ref || '@e1';
        session.setSelectedRef(searchRef);
        await session.actions.fill(searchRef, 'Rust programming language');
        setRecipeStepIndex(3);
        await session.actions.pressKey('Enter');
      } else if (selectedPreset.id === 'stealth-probe') {
        setRecipeStepIndex(2);
        await session.actions.evalText('navigator.webdriver');
        setRecipeStepIndex(3);
        await session.actions.evalText('navigator.userAgent');
        setActiveRightTab('CONSOLE');
      } else if (selectedPreset.id === 'batch-pipeline') {
        setRecipeStepIndex(2);
        await session.actions.batch([{ type: 'extract' }, { type: 'screenshot', full_page: false }]);
        setActiveRightTab('OUTPUT');
      }
    } catch (err: any) {
      setNavError(`Recipe execution failed: ${err.message}`);
    } finally {
      setRecipeRunning(false);
      setRecipeStepIndex(-1);
    }
  };

  // Open custom modal session
  const handleCreateSession = async (e: React.FormEvent) => {
    e.preventDefault();
    setModalLoading(true);
    setModalError(null);
    setNavError(null);
    try {
      const res = await open({
        url: newUrl,
        stealth: newStealth,
      });
      if (res && typeof res === 'object' && 'id' in res) {
        setActiveSession((res as any).id);
      }
      setShowNewModal(false);
    } catch (err: any) {
      setModalError(err.message || 'Failed to create session');
      setNavError(err.message || 'Failed to create session');
    } finally {
      setModalLoading(false);
    }
  };

  // Close session tab
  const handleCloseSession = async (sid: string, e: React.MouseEvent) => {
    e.stopPropagation();
    await close(sid);
    const remaining = sessions.filter((s) => s.id !== sid);
    if (activeSession === sid) {
      const nextSid = remaining.length > 0 ? remaining[0].id : null;
      setActiveSession(nextSid);
      if (!nextSid) {
        setUrlInput('');
        session.setSelectedRef(null);
        session.setSelectedNode(null);
        session.setLastResult(null);
        setNavError(null);
      }
    }
  };

  // Close all open sessions cleanly
  const handleCloseAllSessions = async () => {
    for (const s of sessions) {
      await close(s.id);
    }
    setActiveSession(null);
    setUrlInput('');
    session.setSelectedRef(null);
    session.setSelectedNode(null);
    session.setLastResult(null);
    session.clearLogs();
    setNavError(null);
  };

  // Direct element action
  const handleElementClick = async (el: SnapshotNode) => {
    session.setSelectedRef(el.ref);
    session.setSelectedNode(el);
    const res = await session.actions.click(el.ref);
    if (res?.navigated) {
      session.setSelectedRef(null);
      session.setSelectedNode(null);
    }
  };

  // Batch action submit
  const handleBatchSubmit = async () => {
    try {
      const parsed = JSON.parse(batchJson);
      await session.actions.batch(parsed);
      setBatchModalOpen(false);
      setActiveRightTab('OUTPUT');
    } catch (_) {
      alert('Invalid JSON in batch request');
    }
  };

  const isLoading = navigating || session.loading;
  const estimatedTokens = session.snapshot?.elements.length
    ? Math.round(session.snapshot.elements.length * 12 + 30)
    : 0;

  return (
    <div className="flex-1 min-h-0 flex flex-col bg-bg text-text font-mono text-xs overflow-hidden select-none">
      {/* 1. TOP BROWSER TAB STRIP + PRESET LAUNCHER */}
      <div className="h-10 border-b border-border bg-card flex items-center justify-between px-3 shrink-0 gap-2 overflow-x-auto">
        {/* Real browser tab strip */}
        <div className="flex items-center gap-1.5 overflow-x-auto py-1">
          {sessions.length === 0 ? (
            <span className="text-[11px] text-text-muted italic px-2">No active browser tabs</span>
          ) : (
            sessions.map((s) => {
              const isActive = s.id === activeSession;
              return (
                <div
                  key={s.id}
                  onClick={() => setActiveSession(s.id)}
                  className={`group flex items-center gap-2 px-3 py-1 rounded-t-xs border-t border-x cursor-pointer transition-all max-w-[200px] text-xs ${
                    isActive
                      ? 'bg-surface border-border text-bone font-bold shadow-xs'
                      : 'bg-card border-transparent text-text-dim hover:text-text hover:bg-surface/50'
                  }`}
                >
                  <span className={`w-1.5 h-1.5 rounded-full shrink-0 ${isActive ? 'bg-accent' : 'bg-emerald-400'}`} />
                  <span className="truncate text-[11px]">{s.url.replace(/^https?:\/\//, '') || s.id}</span>
                  <button
                    onClick={(e) => handleCloseSession(s.id, e)}
                    className="text-text-muted hover:text-danger text-[10px] ml-1 opacity-60 group-hover:opacity-100"
                    title="Close tab"
                  >
                    ✕
                  </button>
                </div>
              );
            })
          )}

          {/* + New Tab Button */}
          <button
            onClick={() => setShowNewModal(true)}
            className="px-2.5 py-1 bg-surface hover:bg-surface-elevated border border-border rounded-xs text-[11px] text-bone font-bold flex items-center gap-1 hover:border-accent transition-colors"
            title="Open new browser tab"
          >
            <span>+</span>
            <span className="hidden sm:inline">NEW TAB</span>
          </button>

          {/* Close All Tabs Button */}
          {sessions.length > 1 && (
            <button
              onClick={handleCloseAllSessions}
              className="px-2 py-1 bg-surface hover:bg-danger/20 border border-border hover:border-danger text-[10px] text-text-muted hover:text-danger rounded-xs font-bold transition-colors"
              title="Close all browser sessions and clear workspace"
            >
              ✕ CLOSE ALL ({sessions.length})
            </button>
          )}
          {/* Guided Tour Launcher */}
          <button
            onClick={() => setShowTourGuide(true)}
            className="px-2.5 py-1 bg-surface hover:bg-surface-elevated border border-accent/50 hover:border-accent text-accent rounded-xs text-[11px] font-bold flex items-center gap-1 transition-colors"
            title="Open guided capability tour"
          >
            <span>?</span>
            <span className="hidden sm:inline">TOUR</span>
          </button>
        </div>

        {/* Quick Presets Dropdown & Recipe Runner */}
        <div className="flex items-center gap-1.5 shrink-0">
          <span className="text-[10px] text-text-muted uppercase hidden md:inline">PRESET:</span>
          <select
            onChange={(e) => {
              const found = PRESET_SCENARIOS.find((p) => p.id === e.target.value);
              if (found) handleLaunchPreset(found);
            }}
            value={selectedPreset?.id || ''}
            className="input-sm py-0.5 text-[11px] bg-surface text-bone border-border cursor-pointer max-w-[170px]"
          >
            <option value="" disabled>⚡ Quick Load Recipe...</option>
            {PRESET_SCENARIOS.map((p) => (
              <option key={p.id} value={p.id}>{p.name}</option>
            ))}
          </select>
          {selectedPreset && (
            <button
              onClick={handleExecuteRecipe}
              disabled={recipeRunning || isLoading}
              className="btn-primary py-0.5 px-2 text-[10px] uppercase font-bold tracking-wider"
              title={`Execute ${selectedPreset.name} workflow sequence`}
            >
              {recipeRunning ? `STEP ${recipeStepIndex + 1}...` : '⚡ RUN RECIPE'}
            </button>
          )}
        </div>
      </div>

      {/* MULTI-TAB BROWSER STRIP */}
      {activeSession && (
        <TabBar
          tabs={session.tabs}
          activeTabId={session.activeTabId}
          onSwitchTab={(tabId) => session.actions.switchTab(tabId)}
          onNewTab={(url) => session.actions.newTab(url)}
          onCloseTab={(tabId) => session.actions.closeTab(tabId)}
          disabled={isLoading}
        />
      )}

      {/* 2. BROWSER OMNIBAR & DIRECT ACTIONS */}
      <div className="p-2 border-b border-border bg-surface/90 flex flex-wrap items-center justify-between gap-2 shrink-0">
        {/* Navigation controls cluster (Back, Forward, Reload) & URL input */}
        <div className="flex items-center gap-1 flex-1 min-w-[340px]">
          <div className="flex items-center gap-0.5 bg-card border border-border rounded-xs p-0.5 shrink-0">
            <button
              onClick={() => session.actions.back()}
              disabled={isLoading || !activeSession}
              className="p-1 px-2 text-bone hover:text-accent disabled:opacity-40 text-xs transition-colors rounded-xs hover:bg-surface"
              title="History Back"
            >
              ◀
            </button>
            <button
              onClick={() => session.actions.forward()}
              disabled={isLoading || !activeSession}
              className="p-1 px-2 text-bone hover:text-accent disabled:opacity-40 text-xs transition-colors rounded-xs hover:bg-surface"
              title="History Forward"
            >
              ▶
            </button>
            <button
              onClick={() => session.actions.reload()}
              disabled={isLoading || !activeSession}
              className="p-1 px-2 text-bone hover:text-accent disabled:opacity-40 text-xs transition-colors rounded-xs hover:bg-surface"
              title="Reload Page"
            >
              ↻
            </button>
          </div>

          <form
            onSubmit={(e) => {
              e.preventDefault();
              handleNavigate(urlInput);
            }}
            className="flex items-center gap-1 flex-1"
          >
            <input
              type="text"
              value={urlInput}
              onChange={(e) => setUrlInput(e.target.value)}
              placeholder="Enter any URL (e.g. https://news.google.com, https://en.wikipedia.org)"
              disabled={isLoading}
              className="input-sm flex-1 font-mono text-xs py-1 text-bone"
            />
            <button
              type="submit"
              disabled={isLoading || !urlInput.trim()}
              className="btn-primary py-1 px-4"
            >
              {isLoading ? 'LOADING...' : 'NAVIGATE ›'}
            </button>
          </form>
        </div>

        {/* Primary Agent Tools */}
        <div className="flex items-center gap-1.5 shrink-0">
          <button
            onClick={async () => {
              if (!activeSession) await handleNavigate(urlInput);
              session.actions.extract();
              setActiveRightTab('OUTPUT');
            }}
            disabled={isLoading}
            className="btn-success py-1 px-2.5"
            title="Extract readable clean markdown"
          >
            📄 EXTRACT TEXT
          </button>
          <button
            onClick={async () => {
              if (!activeSession) await handleNavigate(urlInput);
              session.actions.screenshot();
              setActiveRightTab('OUTPUT');
            }}
            disabled={isLoading}
            className="btn-secondary py-1 px-2.5"
            title="Capture full page screenshot"
          >
            📸 SCREENSHOT
          </button>
          <button
            onClick={async () => {
              if (!activeSession) await handleNavigate(urlInput);
              session.actions.source();
              setActiveRightTab('OUTPUT');
            }}
            disabled={isLoading}
            className="btn-ghost py-1 px-2"
            title="View HTML source"
          >
            &lt;/&gt; SOURCE
          </button>
          <button
            onClick={() => session.actions.rotateProxy()}
            disabled={isLoading || !activeSession}
            className="btn-ghost py-1 px-2 text-[11px]"
            title="Cycle to next proxy endpoint in pool and restore URL"
          >
            🔄 ROTATE PROXY
          </button>
          <button
            onClick={() => setActiveRightTab('BATCH')}
            className={`py-1 px-2.5 rounded-xs text-[11px] font-bold transition-colors ${
              activeRightTab === 'BATCH'
                ? 'bg-accent text-white'
                : 'btn-orange-outline'
            }`}
            title="Open visual batch recipe studio"
          >
            ⚡ BATCH
          </button>
          <button
            onClick={() => setShowExportModal(true)}
            className="px-2.5 py-1 bg-surface hover:bg-surface-elevated border border-accent/60 hover:border-accent text-accent hover:text-white rounded-xs text-[11px] font-bold flex items-center gap-1 transition-colors"
            title="Export session workflow to Python, CLI, MCP or REST"
          >
            <span>EXPORT</span>
            <span>📋</span>
          </button>
        </div>
      </div>

      {/* 2b. NAVIGATION JOURNEY TRAIL & GENERATION FRESHNESS */}
      <div className="px-3 py-1 bg-[#121217] border-b border-border flex items-center justify-between text-[11px] font-mono shrink-0 overflow-x-auto gap-2">
        <div className="flex items-center gap-2 min-w-0">
          <span className="text-[10px] uppercase text-text-muted font-bold shrink-0 flex items-center gap-1">
            <span className="w-1.5 h-1.5 rounded-full bg-accent" />
            JOURNEY TRAIL:
          </span>
          {session.historyTrail && session.historyTrail.length > 0 ? (
            <div className="flex items-center gap-1.5 overflow-x-auto py-0.5">
              {session.historyTrail.map((item, idx) => (
                <div key={item.id} className="flex items-center gap-1 shrink-0">
                  {idx > 0 && <span className="text-text-muted">➔</span>}
                  <button
                    onClick={() => handleNavigate(item.url)}
                    className="hover:text-accent underline text-text-dim text-[11px] truncate max-w-[170px]"
                    title={`Jump back to ${item.url} (${item.time})`}
                  >
                    {item.url.replace(/^https?:\/\//, '').replace(/\/$/, '')}
                  </button>
                </div>
              ))}
            </div>
          ) : (
            <span className="text-text-muted italic text-[11px]">
              {sessions.length > 0 ? 'Single page • Breadcrumbs record automatically on navigation' : 'Idle • Enter any URL above or select a preset recipe'}
            </span>
          )}
        </div>

        <div className="flex items-center gap-2 shrink-0">
          {activeSession && sessions.length > 0 ? (
            <span className="px-1.5 py-0.5 bg-surface border border-emerald-500/40 rounded-xs text-[10px] text-emerald-400 font-bold flex items-center gap-1.5" title="Element refs @eN are bound to the current snapshot generation and auto-indexed on every navigation">
              <span className="w-1.5 h-1.5 rounded-full bg-emerald-400 animate-pulse" />
              REFS: GENERATION LIVE ({session.snapshot?.elements.length || 0} NODES)
            </span>
          ) : (
            <span className="px-1.5 py-0.5 bg-card border border-border text-text-muted rounded-xs text-[10px] flex items-center gap-1.5">
              <span className="w-1.5 h-1.5 rounded-full bg-text-muted" />
              ENGINE IDLE (0 NODES)
            </span>
          )}
        </div>
      </div>

      {/* Error alert banner */}
      {(navError || session.error || sessionsError) && (
        <div className="px-3 py-1.5 bg-danger/20 border-b border-danger text-danger text-xs font-mono flex items-center justify-between shrink-0">
          <span>{navError || session.error || sessionsError}</span>
          <button
            onClick={() => setNavError(null)}
            className="text-text-muted hover:text-white font-bold ml-2"
          >
            ✕
          </button>
        </div>
      )}

      {/* 3. STAGED ELEMENT TARGET & INTERACTION BAR */}
      <div className="px-3 py-1.5 border-b border-border bg-card flex flex-wrap items-center justify-between gap-2 shrink-0 text-xs">
        <div className="flex items-center gap-2 min-w-0">
          <span className="text-[10px] text-text-muted uppercase tracking-wider">TARGET:</span>
          {session.selectedRef ? (
            <div className="flex items-center gap-2 min-w-0">
              <span className="badge-orange font-bold text-xs">{session.selectedRef}</span>
              <span className="text-text-dim text-[11px]">[{session.selectedNode?.role}]</span>
              {session.selectedNode?.name && (
                <span className="text-bone font-medium truncate max-w-[200px]" title={session.selectedNode.name}>
                  "{session.selectedNode.name}"
                </span>
              )}
              <button
                onClick={() => {
                  session.setSelectedRef(null);
                  session.setSelectedNode(null);
                }}
                className="text-text-muted hover:text-danger text-[10px] ml-1"
                title="Clear target"
              >
                ✕
              </button>
            </div>
          ) : (
            <span className="text-text-muted text-[11px] italic">
              Click any element in the perception tree to target it
            </span>
          )}
        </div>

        {/* Target Action Controls */}
        <div className="flex items-center gap-1.5">
          <button
            onClick={() => session.actions.click(session.selectedRef || '!')}
            disabled={isLoading || !session.selectedRef}
            className="btn-primary py-0.5 px-3"
            title="Dispatch trusted mouse click at center"
          >
            CLICK {session.selectedRef || ''}
          </button>

          <form
            onSubmit={(e) => {
              e.preventDefault();
              if (fillText && session.selectedRef) session.actions.fill(session.selectedRef, fillText);
            }}
            className="flex items-center gap-1"
          >
            <input
              type="text"
              value={fillText}
              onChange={(e) => setFillText(e.target.value)}
              placeholder="Value to fill..."
              disabled={isLoading || !session.selectedRef}
              className="input-sm py-0.5 w-28 text-[11px]"
            />
            <button
              type="submit"
              disabled={isLoading || !session.selectedRef || !fillText}
              className="btn-secondary py-0.5 px-2 text-[11px]"
            >
              FILL
            </button>
          </form>

          <div className="flex items-center gap-1">
            <select
              value={selectedKey}
              onChange={(e) => setSelectedKey(e.target.value)}
              disabled={isLoading || !activeSession}
              className="input-sm py-0.5 bg-bg text-bone text-[11px]"
            >
              <option value="Enter">Enter</option>
              <option value="Tab">Tab</option>
              <option value="Escape">Escape</option>
            </select>
            <button
              onClick={() => session.actions.pressKey(selectedKey)}
              disabled={isLoading || !activeSession}
              className="btn-secondary py-0.5 px-2 text-[11px]"
            >
              KEY
            </button>
          </div>
        </div>
      </div>

      {/* 4. MAIN WORKSPACE */}
      <div className="flex-1 min-h-0 flex overflow-hidden relative">
        {/* Action progress bar */}
        {session.loading && !navigating && (
          <div className="absolute top-0 left-0 right-0 h-0.5 bg-gradient-to-r from-accent via-amber-400 to-accent z-40 animate-pulse" />
        )}

        {/* Navigation Loading Overlay - ONLY shown when navigating to a new destination */}
        {navigating && (
          <div className="absolute inset-0 bg-black/70 backdrop-blur-xs z-30 flex flex-col items-center justify-center p-6 text-center">
            <div className="te-panel rounded-xs border-accent p-6 max-w-sm space-y-3 shadow-2xl">
              <div className="w-10 h-10 mx-auto rounded-full border-2 border-accent border-t-transparent animate-spin" />
              <div className="font-bold text-bone text-xs uppercase tracking-wider">
                COMMUNICATING WITH ENGINE
              </div>
              <p className="text-[11px] text-text-dim font-sans">
                Launching instance, dispatching navigation, and indexing accessibility perception tree...
              </p>
            </div>
          </div>
        )}

        {/* LEFT / CENTER: THE REAL BROWSER (TAKES 65% OF SCREEN WIDTH) */}
        <div className="flex-1 min-h-0 flex flex-col border-r border-border overflow-hidden">
          {/* View mode toggle header */}
          <div className="px-3 py-1.5 border-b border-border bg-surface flex items-center justify-between shrink-0">
            <div className="flex items-center gap-2 min-w-0">
              <span className="font-bold text-bone text-xs flex items-center gap-1.5 truncate">
                <span className={`w-2 h-2 rounded-full ${isLoading ? 'bg-accent animate-ping' : 'bg-emerald-400'}`} />
                {session.snapshot?.title || 'Active Viewport'}
              </span>
              <span className="badge-dim hidden sm:inline">
                {session.snapshot?.elements.length || 0} NODES
              </span>
              <span className="badge-yellow">
                ~{estimatedTokens} TOKENS
              </span>
            </div>

            <div className="flex items-center gap-1 bg-bg border border-border p-0.5 rounded-xs text-[10px] shrink-0">
              <button
                onClick={() => setViewMode('SPLIT')}
                className={`px-2 py-0.5 rounded-xs transition-colors ${
                  viewMode === 'SPLIT' ? 'bg-accent text-white font-bold' : 'text-text-dim hover:text-text'
                }`}
              >
                SPLIT VIEW
              </button>
              <button
                onClick={() => setViewMode('RENDER')}
                className={`px-2 py-0.5 rounded-xs transition-colors ${
                  viewMode === 'RENDER' ? 'bg-accent text-white font-bold' : 'text-text-dim hover:text-text'
                }`}
              >
                LIVE RENDER
              </button>
              <button
                onClick={() => setViewMode('TREE')}
                className={`px-2 py-0.5 rounded-xs transition-colors ${
                  viewMode === 'TREE' ? 'bg-accent text-white font-bold' : 'text-text-dim hover:text-text'
                }`}
              >
                PERCEPTION TREE
              </button>
            </div>
          </div>

          {/* Body: Live Render Canvas & Perception Elements Tree */}
          <div className="flex-1 min-h-0 flex flex-col md:flex-row overflow-hidden bg-[#0d0d10]">
            {/* Live Render Canvas (Shown in SPLIT or RENDER mode) */}
            {(viewMode === 'SPLIT' || viewMode === 'RENDER') && (
              <div className={`flex flex-col min-h-0 border-b md:border-b-0 md:border-r border-border overflow-hidden ${
                viewMode === 'SPLIT' ? 'flex-1 md:w-1/2' : 'flex-1'
              }`}>
                <div className="px-3 py-1 bg-surface/80 border-b border-border/80 flex items-center justify-between text-[10px] text-text-muted shrink-0">
                  <div className="flex items-center gap-2">
                    <span className="text-bone font-bold uppercase">LIVE SCREEN CANVAS</span>
                    <span className={session.liveScreenshot ? 'text-emerald-400 font-bold' : 'text-text-muted'}>
                      {session.liveScreenshot ? 'REAL-TIME CHROMIUM' : 'AWAITING RENDER'}
                    </span>
                  </div>
                  {/* Canvas Controls */}
                  <div className="flex items-center gap-1">
                    <button
                      onClick={() => setVisionClickMode(!visionClickMode)}
                      className={`px-1.5 py-0.5 rounded-xs text-[9px] font-bold transition-colors ${
                        visionClickMode ? 'bg-accent text-white' : 'bg-surface text-text-dim hover:text-text'
                      }`}
                      title="Toggle coordinate vision click"
                    >
                      {visionClickMode ? '🎯 VISION CLICK: ON' : '👁️ VISION CLICK: OFF'}
                    </button>
                    <button
                      onClick={() => handleScroll(-400)}
                      disabled={isLoading || !activeSession}
                      className="px-1.5 py-0.5 bg-surface hover:bg-surface-elevated text-bone border border-border rounded-xs text-[9px] font-bold"
                      title="Scroll up 400px"
                    >
                      ▲ UP
                    </button>
                    <button
                      onClick={() => handleScroll(400)}
                      disabled={isLoading || !activeSession}
                      className="px-1.5 py-0.5 bg-surface hover:bg-surface-elevated text-bone border border-border rounded-xs text-[9px] font-bold"
                      title="Scroll down 400px"
                    >
                      ▼ DOWN
                    </button>
                    <button
                      onClick={() => session.actions.screenshot()}
                      disabled={isLoading || !activeSession}
                      className="px-1.5 py-0.5 bg-surface hover:bg-surface-elevated text-accent border border-border rounded-xs text-[9px] font-bold"
                      title="Force refresh screenshot"
                    >
                      ↻
                    </button>
                  </div>
                </div>
                <div
                  onWheel={(e) => {
                    if (activeSession && !isLoading) {
                      e.preventDefault();
                      handleScroll(e.deltaY > 0 ? 300 : -300);
                    }
                  }}
                  className="flex-1 min-h-0 overflow-y-auto p-3 flex items-start justify-center bg-[radial-gradient(#1f1f26_1px,transparent_1px)] bg-[size:16px_16px]"
                >
                  {session.liveScreenshot ? (
                    <div className="border border-border-strong rounded-xs shadow-2xl overflow-hidden max-w-full relative select-none">
                      <img
                        src={`data:image/png;base64,${session.liveScreenshot}`}
                        alt="Live Web Page Render"
                        onClick={visionClickMode ? handleCanvasClick : undefined}
                        className={`max-w-full h-auto object-contain block ${visionClickMode ? 'cursor-crosshair' : ''}`}
                      />
                      {clickRipple.visible && (
                        <div
                          className="absolute pointer-events-none -translate-x-1/2 -translate-y-1/2 rounded-full border-2 border-accent bg-accent/40 animate-ping z-20"
                          style={{
                            left: clickRipple.x,
                            top: clickRipple.y,
                            width: '28px',
                            height: '28px',
                          }}
                        />
                      )}
                    </div>
                  ) : activeSession && session.snapshot ? (
                    <div className="p-6 text-center text-text-dim my-auto space-y-3 max-w-md">
                      <div className="inline-flex items-center gap-2 px-2.5 py-1 bg-accent/15 border border-accent/40 rounded-xs text-bone text-[11px] font-bold uppercase tracking-wider font-mono">
                        <span className="w-2 h-2 rounded-xs bg-accent animate-pulse" />
                        CHROMIUM ENGINE ACTIVE
                      </div>
                      <div className="text-xs font-bold text-bone">SCREENSHOT LOADING</div>
                      <p className="text-[11px] font-sans text-text-dim leading-relaxed">
                        The browser runtime is running and the perception tree is available. The live screenshot will appear shortly.
                      </p>
                      <div className="text-[10px] text-accent font-mono border border-accent/20 bg-accent/5 p-2 rounded-xs truncate">
                        ACTIVE: {session.snapshot.url || activeSession} ({session.snapshot.elements.length} nodes)
                      </div>
                    </div>
                  ) : (
                    <div className="p-8 text-center text-text-dim my-auto space-y-3 max-w-md">
                      <div className="text-sm font-bold text-bone">READY TO BROWSE</div>
                      <p className="text-xs font-sans text-text-dim">
                        Type any URL into the bar above and click <span className="text-accent font-bold">NAVIGATE ›</span>, or pick a verified preset recipe to stream visual page renders.
                      </p>
                      <div className="flex flex-wrap items-center justify-center gap-2 pt-2">
                        <button
                          onClick={() => handleNavigate('https://en.wikipedia.org/wiki/Artificial_intelligence')}
                          className="btn-secondary text-[11px]"
                        >
                          WIKIPEDIA AI ›
                        </button>
                        <button
                          onClick={() => handleNavigate('https://news.ycombinator.com')}
                          className="btn-secondary text-[11px]"
                        >
                          HACKER NEWS ›
                        </button>
                        <button
                          onClick={() => handleNavigate('https://example.com')}
                          className="btn-secondary text-[11px]"
                        >
                          EXAMPLE DOMAIN ›
                        </button>
                      </div>
                    </div>
                  )}
                </div>
              </div>
            )}

            {/* Perception Tree Table (Shown in SPLIT or TREE mode) */}
            {(viewMode === 'SPLIT' || viewMode === 'TREE') && (
              <div className={`flex flex-col min-h-0 overflow-hidden bg-bg ${
                viewMode === 'SPLIT' ? 'flex-1 md:w-1/2' : 'flex-1'
              }`}>
                <div className="px-3 py-1.5 bg-surface/90 border-b border-border/80 flex flex-col gap-1.5 shrink-0">
                  <div className="flex items-center justify-between text-[10px] text-text-muted">
                    <span className="text-bone font-bold uppercase">ACCESSIBILITY TREE (&lt;500 TOKENS)</span>
                    <span>{session.snapshot?.elements.length || 0} NODES</span>
                  </div>
                  {/* CSS Selector Resolver */}
                  <form onSubmit={handleFindCss} className="flex items-center gap-1">
                    <input
                      type="text"
                      value={cssQuery}
                      onChange={(e) => setCssQuery(e.target.value)}
                      placeholder="Find by CSS (e.g. a[href*='wiki'], button)..."
                      disabled={isLoading || !activeSession}
                      className="input-sm flex-1 text-[11px] py-0.5"
                    />
                    <button
                      type="submit"
                      disabled={isLoading || !activeSession || !cssQuery.trim()}
                      className="btn-secondary py-0.5 px-2 text-[10px] font-bold shrink-0"
                    >
                      {cssSearching ? 'FINDING...' : 'FIND CSS ›'}
                    </button>
                    {cssMatches.length > 0 && (
                      <button
                        type="button"
                        onClick={() => {
                          setCssMatches([]);
                          setCssMessage(null);
                          setCssQuery('');
                        }}
                        className="text-text-muted hover:text-danger text-[10px] px-1"
                        title="Clear matches"
                      >
                        ✕
                      </button>
                    )}
                  </form>
                  {cssMessage && (
                    <div className="text-[10px] text-accent truncate">
                      {cssMessage}
                    </div>
                  )}
                </div>
                <div className="flex-1 min-h-0 overflow-y-auto p-2 space-y-1">
                  {!session.snapshot || session.snapshot.elements.length === 0 ? (
                    <div className="p-6 text-center text-text-dim text-xs">
                      No interactive elements loaded. Enter a URL above and click NAVIGATE.
                    </div>
                  ) : (
                    session.snapshot.elements.map((el) => {
                      const isSelected = session.selectedRef === el.ref;
                      const isCssMatch = cssMatches.includes(el.ref);
                      return (
                        <div
                          key={el.ref}
                          onClick={() => {
                            session.setSelectedRef(el.ref);
                            session.setSelectedNode(el);
                          }}
                          className={`p-2 rounded-xs border cursor-pointer transition-all flex items-center justify-between gap-2 ${
                            isSelected
                              ? 'bg-accent/15 border-accent shadow-xs'
                              : isCssMatch
                              ? 'bg-accent/10 border-accent'
                              : 'bg-card border-border hover:border-border-strong hover:bg-surface/60'
                          }`}
                        >
                          <div className="flex items-center gap-2 min-w-0">
                            <span className={`w-11 text-center py-0.5 rounded-xs text-[11px] font-bold font-mono ${
                              isSelected ? 'bg-accent text-white' : isCssMatch ? 'bg-accent text-white' : 'bg-surface border border-border text-accent'
                            }`}>
                              {el.ref}
                            </span>
                            <span className="text-[9px] uppercase font-bold text-text-muted px-1 py-0.5 bg-bg border border-border rounded-xs w-14 text-center truncate">
                              {el.role}
                            </span>
                            <span className="text-bone text-xs truncate max-w-[160px] lg:max-w-[240px]">
                              {el.name || <span className="text-text-muted italic">(unnamed)</span>}
                            </span>
                          </div>

                          <button
                            onClick={(e) => {
                              e.stopPropagation();
                              handleElementClick(el);
                            }}
                            className="px-2 py-0.5 bg-accent/20 hover:bg-accent text-accent hover:text-white border border-accent/40 rounded-xs text-[10px] font-bold uppercase transition-colors shrink-0"
                          >
                            CLICK ›
                          </button>
                        </div>
                      );
                    })
                  )}
                </div>
              </div>
            )}
          </div>
        </div>

        {/* RIGHT DRAWER: TELEMETRY, EXTRACTED CONTENT & PROTOCOL INSPECTOR (35% WIDTH) */}
        <div className="w-80 lg:w-96 min-h-0 flex flex-col bg-card shrink-0 overflow-hidden">
          {/* Drawer Header Tabs */}
          <div className="flex items-center border-b border-border bg-surface text-[10px] shrink-0 overflow-x-auto">
            <button
              onClick={() => setActiveRightTab('OUTPUT')}
              className={`px-3 py-2 text-center font-bold tracking-wider transition-colors whitespace-nowrap ${
                activeRightTab === 'OUTPUT' ? 'bg-card text-bone border-b-2 border-accent' : 'text-text-dim hover:text-text'
              }`}
            >
              EXTRACT
            </button>
            <button
              onClick={() => setActiveRightTab('CONSOLE')}
              className={`px-3 py-2 text-center font-bold tracking-wider transition-colors whitespace-nowrap ${
                activeRightTab === 'CONSOLE' ? 'bg-card text-bone border-b-2 border-accent' : 'text-text-dim hover:text-text'
              }`}
            >
              CONSOLE
            </button>
            <button
              onClick={() => setActiveRightTab('TABS')}
              className={`px-3 py-2 text-center font-bold tracking-wider transition-colors whitespace-nowrap ${
                activeRightTab === 'TABS' ? 'bg-card text-bone border-b-2 border-accent' : 'text-text-dim hover:text-text'
              }`}
            >
              TABS
            </button>
            <button
              onClick={() => setActiveRightTab('COOKIES')}
              className={`px-3 py-2 text-center font-bold tracking-wider transition-colors whitespace-nowrap ${
                activeRightTab === 'COOKIES' ? 'bg-card text-bone border-b-2 border-accent' : 'text-text-dim hover:text-text'
              }`}
            >
              COOKIES
            </button>
            <button
              onClick={() => setActiveRightTab('DOWNLOADS')}
              className={`px-3 py-2 text-center font-bold tracking-wider transition-colors whitespace-nowrap ${
                activeRightTab === 'DOWNLOADS' ? 'bg-card text-bone border-b-2 border-accent' : 'text-text-dim hover:text-text'
              }`}
            >
              DOWNLOADS
            </button>
            <button
              onClick={() => setActiveRightTab('WEBMCP')}
              className={`px-3 py-2 text-center font-bold tracking-wider transition-colors whitespace-nowrap ${
                activeRightTab === 'WEBMCP' ? 'bg-card text-bone border-b-2 border-accent' : 'text-text-dim hover:text-text'
              }`}
            >
              WEBMCP
            </button>
            <button
              onClick={() => setActiveRightTab('BATCH')}
              className={`px-3 py-2 text-center font-bold tracking-wider transition-colors whitespace-nowrap ${
                activeRightTab === 'BATCH' ? 'bg-card text-bone border-b-2 border-accent' : 'text-text-dim hover:text-text'
              }`}
            >
              BATCH
            </button>
            <button
              onClick={() => setActiveRightTab('WIRE_JSON')}
              className={`px-3 py-2 text-center font-bold tracking-wider transition-colors whitespace-nowrap ${
                activeRightTab === 'WIRE_JSON' ? 'bg-card text-bone border-b-2 border-accent' : 'text-text-dim hover:text-text'
              }`}
            >
              WIRE JSON
            </button>
            <button
              onClick={() => setActiveRightTab('LOGS')}
              className={`px-3 py-2 text-center font-bold tracking-wider transition-colors whitespace-nowrap ${
                activeRightTab === 'LOGS' ? 'bg-card text-bone border-b-2 border-accent' : 'text-text-dim hover:text-text'
              }`}
            >
              LOGS
            </button>
          </div>

          {/* Context Efficiency Telemetry Gauge */}
          <div className="p-2 border-b border-border bg-[#101014] shrink-0">
            <TokenSavingsGauge elementsCount={session.snapshot?.elements.length || 0} rawSourceBytes={session.rawSourceBytes} />
          </div>

          {/* Drawer Body */}
          <div className="flex-1 min-h-0 overflow-y-auto p-3">
            {activeRightTab === 'WIRE_JSON' && (
              <div className="space-y-2">
                <div className="flex items-center justify-between text-[10px] text-text-muted">
                  <span>LAST ACTION RESULT FRAME:</span>
                  {session.lastResult && (
                    <div className="flex items-center gap-2">
                      <button
                        onClick={() => navigator.clipboard.writeText(JSON.stringify(session.lastResult, null, 2))}
                        className="text-accent hover:underline cursor-pointer"
                      >
                        COPY
                      </button>
                      <button
                        onClick={() => session.setLastResult(null)}
                        className="text-text-muted hover:text-danger underline cursor-pointer"
                        title="Clear wire frame"
                      >
                        ✕ CLEAR
                      </button>
                    </div>
                  )}
                </div>
                <pre className="p-3 bg-bg border border-border rounded-xs text-[11px] font-mono text-emerald-400 overflow-x-auto leading-relaxed shadow-te-inset max-h-[600px]">
                  {session.lastResult ? JSON.stringify(session.lastResult, null, 2) : '// No action dispatched yet'}
                </pre>
              </div>
            )}

            {activeRightTab === 'LOGS' && (
              <div className="space-y-2">
                <div className="flex items-center justify-between text-[10px] text-text-muted uppercase">
                  <span>REAL-TIME EXECUTION LOG:</span>
                  {session.activityLogs.length > 0 && (
                    <button
                      onClick={session.clearLogs}
                      className="text-text-muted hover:text-danger underline cursor-pointer"
                      title="Clear execution logs"
                    >
                      ✕ CLEAR LOGS
                    </button>
                  )}
                </div>
                <div className="space-y-1.5 font-mono text-[11px]">
                  {session.activityLogs.length === 0 ? (
                    <div className="text-text-muted text-xs p-3">No activity logged yet.</div>
                  ) : (
                    session.activityLogs.map((log) => (
                      <div key={log.id} className="p-2 bg-surface border border-border rounded-xs">
                        <div className="flex items-center justify-between text-[10px] text-text-muted">
                          <span>{log.time}</span>
                          <span className="text-accent font-bold">{log.type}</span>
                        </div>
                        <div className="text-bone mt-0.5">{log.summary}</div>
                        {log.details && <div className="text-text-dim text-[10px] mt-0.5 break-all">{log.details}</div>}
                      </div>
                    ))
                  )}
                </div>
              </div>
            )}

            {activeRightTab === 'OUTPUT' && (
              <div className="space-y-3">
                {!session.lastResult ? (
                  <div className="p-6 text-center text-text-dim space-y-2">
                    <div className="text-xs font-bold text-bone">NO EXTRACTED OUTPUT</div>
                    <p className="text-[11px] font-sans">
                      Click "EXTRACT TEXT" above or click any link in the tree to view markdown or result data.
                    </p>
                  </div>
                ) : session.lastResult.type === 'text' ? (
                  <div className="space-y-2">
                    <div className="flex items-center justify-between text-[10px]">
                      <span className="text-text-muted font-bold uppercase">
                        READABILITY OUTPUT ({session.lastResult.text.length} BYTES):
                      </span>
                      <div className="flex items-center gap-2">
                        <button
                          onClick={() => navigator.clipboard.writeText((session.lastResult as any).text)}
                          className="text-accent hover:underline uppercase cursor-pointer"
                        >
                          COPY
                        </button>
                        <button
                          onClick={() => session.setLastResult(null)}
                          className="text-text-muted hover:text-danger uppercase underline cursor-pointer"
                          title="Dismiss output"
                        >
                          ✕ CLEAR
                        </button>
                      </div>
                    </div>
                    <pre className="p-3 bg-bg border border-border rounded-xs text-[11px] font-sans text-bone whitespace-pre-wrap leading-relaxed max-h-[600px] overflow-y-auto shadow-te-inset">
                      {session.lastResult.text}
                    </pre>
                  </div>
                ) : session.lastResult.type === 'clicked' ? (
                  <div className="te-panel rounded-xs border-border p-4 space-y-2">
                    <div className="badge-green">CLICK COMPLETED</div>
                    <div className="text-xs text-bone">
                      Navigated: <span className="font-bold">{session.lastResult.navigated ? 'YES' : 'NO'}</span>
                    </div>
                    {session.lastResult.url && (
                      <div className="text-[11px] text-text-dim break-all mt-1">
                        URL: {session.lastResult.url}
                      </div>
                    )}
                  </div>
                ) : (
                  <pre className="p-3 bg-bg border border-border rounded-xs text-xs text-bone whitespace-pre-wrap">
                    {JSON.stringify(session.lastResult, null, 2)}
                  </pre>
                )}
              </div>
            )}

            {activeRightTab === 'CONSOLE' && (
              <ConsoleRepl
                onEval={(expr) => session.actions.evalText(expr)}
                loading={session.loading}
                disabled={!activeSession}
              />
            )}

            {activeRightTab === 'TABS' && (
              <TabManager
                tabs={session.tabs}
                activeTabId={session.activeTabId}
                onSwitchTab={(tabId) => session.actions.switchTab(tabId)}
                onNewTab={(url) => session.actions.newTab(url)}
                onCloseTab={(tabId) => session.actions.closeTab(tabId)}
                onRefreshTabs={() => session.actions.listTabs()}
                loading={session.loading}
                disabled={!activeSession}
              />
            )}

            {activeRightTab === 'COOKIES' && (
              <CookieManager
                onGetCookies={() => session.actions.getCookies()}
                onSetCookie={(c) => session.actions.setCookie(c)}
                onClearCookies={() => session.actions.clearCookies()}
                loading={session.loading}
                disabled={!activeSession}
                currentUrl={session.snapshot?.url}
              />
            )}

            {activeRightTab === 'DOWNLOADS' && (
              <DownloadsViewer
                onGetDownloads={() => session.actions.getDownloads()}
                onSetDownloadDir={(d) => session.actions.setDownloadDir(d)}
                loading={session.loading}
                disabled={!activeSession}
              />
            )}

            {activeRightTab === 'WEBMCP' && (
              <WebMcpInspector
                onGetTools={() => session.actions.webmcpTools()}
                onInvokeTool={(name, argsJson) => session.actions.webmcpInvoke(name, argsJson)}
                loading={session.loading}
                disabled={!activeSession}
                currentUrl={session.snapshot?.url}
              />
            )}

            {activeRightTab === 'BATCH' && (
              <BatchStudio
                onRunBatch={(actions) => session.actions.batch(actions)}
                loading={session.loading}
                disabled={!activeSession}
                activeRef={session.selectedRef}
              />
            )}
          </div>
        </div>
      </div>

      {/* NEW BROWSER NODE MODAL */}
      {showNewModal && (
        <div className="fixed inset-0 bg-black/80 backdrop-blur-xs flex items-center justify-center p-4 z-50">
          <div className="te-panel rounded-xs border-accent p-5 w-full max-w-md space-y-4 shadow-2xl">
            <div className="flex items-center justify-between pb-2 border-b border-border">
              <span className="font-mono text-xs font-bold text-bone uppercase flex items-center gap-1.5">
                <span className="w-2 h-2 rounded-xs bg-accent" />
                INITIALIZE BROWSER INSTANCE
              </span>
              <button
                onClick={() => setShowNewModal(false)}
                className="text-text-dim hover:text-text text-xs font-bold"
              >
                ✕
              </button>
            </div>

            <form onSubmit={handleCreateSession} className="space-y-3.5 font-mono text-xs">
              {modalError && (
                <div className="p-2.5 bg-danger/15 border border-danger/40 text-danger text-[11px] rounded-xs space-y-1">
                  <div className="font-bold uppercase tracking-wider flex items-center gap-1.5">
                    <span className="w-1.5 h-1.5 rounded-full bg-danger" />
                    SPAWN ENGINE ERROR
                  </div>
                  <div className="text-[10px] opacity-90 leading-tight">{modalError}</div>
                </div>
              )}

              <div className="space-y-1">
                <label className="text-[11px] text-text-dim uppercase tracking-wider block">TARGET URL:</label>
                <input
                  type="text"
                  value={newUrl}
                  onChange={(e) => setNewUrl(e.target.value)}
                  placeholder="https://..."
                  className="input-sm w-full font-mono text-xs"
                  required
                />
              </div>

              <div className="p-2.5 bg-surface border border-border rounded-xs text-[11px] text-text-dim">
                <div className="flex items-center gap-1.5 mb-1">
                  <span className="w-1.5 h-1.5 rounded-full bg-emerald-400" />
                  <span className="text-bone font-bold uppercase tracking-wider">ENGINE: CDP (CHROMIUM)</span>
                </div>
                <p className="text-[10px] leading-relaxed">
                  Full Chromium browser via Chrome DevTools Protocol. Real rendering, screenshots, trusted mouse events, and JavaScript evaluation.
                </p>
              </div>

              <label className="flex items-center gap-2 cursor-pointer text-text-dim hover:text-text pt-1">
                <input
                  type="checkbox"
                  checked={newStealth}
                  onChange={(e) => setNewStealth(e.target.checked)}
                  className="accent-accent"
                />
                <span>STEALTH EVASIONS (MASK WEBDRIVER)</span>
              </label>

              <div className="flex items-center justify-end gap-2 pt-3 border-t border-border">
                <button
                  type="button"
                  onClick={() => setShowNewModal(false)}
                  className="btn-ghost"
                >
                  CANCEL
                </button>
                <button
                  type="submit"
                  disabled={modalLoading}
                  className="btn-primary"
                >
                  {modalLoading ? 'LAUNCHING...' : 'SPAWN BROWSER INSTANCE ›'}
                </button>
              </div>
            </form>
          </div>
        </div>
      )}

      {/* BATCH ACTION MODAL */}
      {batchModalOpen && (
        <div className="fixed inset-0 bg-black/80 backdrop-blur-xs flex items-center justify-center p-4 z-50">
          <div className="te-panel rounded-xs border-accent p-5 w-full max-w-lg space-y-4 shadow-2xl font-mono text-xs">
            <div className="flex items-center justify-between pb-2 border-b border-border">
              <span className="font-bold text-bone uppercase flex items-center gap-1.5">
                <span className="w-2 h-2 rounded-xs bg-accent" />
                DISPATCH BATCH ACTIONS (FAIL-FAST)
              </span>
              <button
                onClick={() => setBatchModalOpen(false)}
                className="text-text-dim hover:text-text font-bold"
              >
                ✕
              </button>
            </div>
            <textarea
              value={batchJson}
              onChange={(e) => setBatchJson(e.target.value)}
              rows={6}
              className="w-full bg-bg border border-border rounded-xs p-3 font-mono text-xs text-emerald-400 focus:outline-none focus:border-accent shadow-te-inset"
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
                disabled={isLoading}
                className="btn-primary"
              >
                DISPATCH BATCH ›
              </button>
              </div>
            </div>
          </div>
        )}

      {/* EXPORT TO CODE MODAL */}
      <ExportCodeModal
        isOpen={showExportModal}
        onClose={() => setShowExportModal(false)}
        url={session.snapshot?.url || urlInput}
        selectedRef={session.selectedRef}
      />

      {/* GUIDED CAPABILITY TOUR MODAL */}
      <TourGuide
        visible={showTourGuide}
        onClose={() => setShowTourGuide(false)}
      />
    </div>
  );
}
