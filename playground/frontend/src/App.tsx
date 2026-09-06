import React, { useState, useCallback } from 'react';
import { useSessions } from './hooks/useSessions';
import { useSession } from './hooks/useSession';
import { SessionManager } from './components/SessionManager';
import { SnapshotView } from './components/SnapshotView';
import { ActionToolbar, ToolbarCallbacks } from './components/ActionToolbar';
import { ResultPane } from './components/ResultPane';
import { TourGuide } from './components/TourGuide';
import type { SessionInfo } from './hooks/useSessions';
import type { SnapshotNode } from './lib/types';

function App() {
  const [showTour, setShowTour] = useState(false);
  const { sessions, loading: sessionsLoading, error: sessionsError, open, close, reload } = useSessions();
  const [activeSession, setActiveSession] = useState<string | null>(null);
  const session = useSession(activeSession);

  // Handle session open from SessionManager
  const handleOpen = useCallback(async (opts: Record<string, unknown>) => {
    try {
      const result = await open(opts);
      if (result && typeof result === 'object' && 'id' in result) {
        setActiveSession((result as any).id);
      }
    } catch (e) {
      // Error already surfaced via hooks
    }
  }, [open]);

  const handleClose = useCallback(async (sid: string) => {
    await close(sid);
    if (activeSession === sid) {
      setActiveSession(null);
      session.setSelectedRef(null);
    }
  }, [activeSession, close, session]);

  // Build toolbar callbacks from the session hook
  const toolbarCallbacks: ToolbarCallbacks = {
    click: () => session.actions.click(session.selectedRef || '!') as any,
    fill: (text: string) => session.actions.fill(session.selectedRef || '!', text) as any,
    selectOption: (value: string) => session.actions.selectOption(session.selectedRef || '!', value) as any,
    pressKey: (key: string) => session.actions.pressKey(key) as any,
    findCss: (selector: string) => session.actions.findByCss(selector) as any,
    evalText: (expr: string) => session.actions.evalText(expr) as any,
    extract: () => session.actions.extract() as any,
    source: () => session.actions.source() as any,
    screenshot: () => session.actions.screenshot() as any,
    navigate: (url: string) => session.actions.navigate(url) as any,
    setFileChooser: (paths: string[]) => session.actions.setFileChooser(session.selectedRef || '!', paths) as any,
    cookies: () => session.actions.getCookies() as any,
    downloads: () => session.actions.getDownloads() as any,
    batch: (actions: unknown[]) => session.actions.batch(actions) as any,
  };

  return (
    <div className="flex h-screen bg-bg text-text font-sans">
      {/* Session Manager sidebar */}
      <SessionManager
        sessions={sessions}
        activeSession={activeSession}
        onSelect={setActiveSession}
        onOpen={handleOpen}
        onClose={handleClose}
        onList={reload}
      />

      {/* Main content area */}
      <div className="flex-1 flex flex-col overflow-hidden">
        {/* Action Toolbar */}
        <ActionToolbar
          selectedRef={session.selectedRef}
          selectedNode={session.selectedNode ? {
            role: session.selectedNode.role,
            name: session.selectedNode.name,
            value: session.selectedNode.value,
          } : null}
          cb={toolbarCallbacks}
          loading={session.loading}
          lastResult={session.lastResult}
          onClear={() => session.setSelectedRef(null)}
        />

        {/* Snapshot View */}
        <div className="flex-1 overflow-y-auto">
          <SnapshotView
            snapshot={session.snapshot}
            onElementClick={(node: SnapshotNode) => {
              session.setSelectedRef(node.ref);
            }}
            selectedRef={session.selectedRef}
          />
        </div>
      </div>

      {/* Result Pane */}
      <ResultPane lastResult={session.lastResult} loading={session.loading} />

      {/* Tour overlay */}
      {showTour && <TourGuide visible={showTour} onClose={() => setShowTour(false)} />}

      {/* Error banner */}
      {session.error && (
        <div className="fixed bottom-4 right-4 bg-danger/20 border border-danger text-danger px-3 py-2 rounded text-sm max-w-sm">
          {session.error}
        </div>
      )}

      {/* Tour button */}
      <button
        onClick={() => setShowTour(true)}
        className="fixed top-3 right-3 px-3 py-1.5 bg-accent hover:bg-accent-hover text-white rounded text-xs font-medium z-10"
        title="Show guided tour"
      >
        ? Tour
      </button>
    </div>
  );
}

export default App;
