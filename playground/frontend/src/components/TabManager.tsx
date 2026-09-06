import { useState } from 'react';
import type { TabInfo } from '../lib/types';

interface Props {
  tabs: TabInfo[];
  activeTabId: string | null;
  onSwitchTab: (tabId: string) => Promise<any>;
  onNewTab: (url?: string) => Promise<any>;
  onCloseTab: (tabId: string) => Promise<any>;
  onRefreshTabs: () => Promise<any>;
  loading?: boolean;
  disabled?: boolean;
}

export function TabManager({
  tabs,
  activeTabId,
  onSwitchTab,
  onNewTab,
  onCloseTab,
  onRefreshTabs,
  loading = false,
  disabled = false,
}: Props) {
  const [newTabUrl, setNewTabUrl] = useState('');
  const [statusMsg, setStatusMsg] = useState<string | null>(null);

  const handleCreate = async (e: React.FormEvent) => {
    e.preventDefault();
    if (disabled) return;
    try {
      await onNewTab(newTabUrl.trim() || undefined);
      setNewTabUrl('');
      setStatusMsg('New tab created successfully');
      setTimeout(() => setStatusMsg(null), 3000);
    } catch (e: any) {
      setStatusMsg(`Failed to open tab: ${e.message}`);
    }
  };

  const handleClose = async (tabId: string) => {
    if (disabled || tabs.length <= 1) return;
    try {
      await onCloseTab(tabId);
      setStatusMsg(`Closed tab ${tabId}`);
      setTimeout(() => setStatusMsg(null), 3000);
    } catch (e: any) {
      setStatusMsg(`Failed to close tab: ${e.message}`);
    }
  };

  return (
    <div className="p-3 space-y-3 text-xs font-mono">
      <div className="flex items-center justify-between pb-2 border-b border-border">
        <div className="flex items-center gap-2">
          <span className="font-bold text-bone uppercase text-[11px]">TAB REGISTRY</span>
          <span className="badge-orange">{tabs.length} ACTIVE</span>
        </div>
        <button
          onClick={() => onRefreshTabs()}
          disabled={disabled || loading}
          className="text-[10px] text-accent hover:underline uppercase cursor-pointer"
        >
          REFRESH
        </button>
      </div>

      {statusMsg && (
        <div className="p-2 bg-accent/10 border border-accent/40 rounded-xs text-[10px] text-accent font-mono">
          {statusMsg}
        </div>
      )}

      {/* New Tab Form */}
      <form onSubmit={handleCreate} className="space-y-1.5 p-2 bg-[#0e0e14] border border-border rounded-xs">
        <div className="text-[10px] text-text-muted font-bold uppercase">OPEN NEW TAB</div>
        <div className="flex gap-1.5">
          <input
            type="text"
            value={newTabUrl}
            onChange={(e) => setNewTabUrl(e.target.value)}
            placeholder="URL (optional, default: about:blank)"
            disabled={disabled || loading}
            className="te-input text-[11px] py-1 px-2 flex-1 bg-bg border-border rounded-xs"
          />
          <button
            type="submit"
            disabled={disabled || loading}
            className="btn-primary py-1 px-3 text-[10px] font-bold shrink-0"
          >
            + NEW
          </button>
        </div>
      </form>

      {/* Tabs List */}
      <div className="space-y-1.5 max-h-[340px] overflow-y-auto">
        {tabs.length === 0 ? (
          <div className="p-4 text-center text-text-dim text-[11px]">
            No secondary tabs. Click "+ NEW" above to spawn an isolated browser tab.
          </div>
        ) : (
          tabs.map((tab) => {
            const isActive = activeTabId ? tab.id === activeTabId : tabs[0].id === tab.id;
            return (
              <div
                key={tab.id}
                className={`p-2 rounded-xs border transition-colors ${
                  isActive
                    ? 'bg-surface border-accent shadow-xs'
                    : 'bg-[#111116] border-border hover:border-border/80'
                }`}
              >
                <div className="flex items-center justify-between gap-1 mb-1">
                  <div className="flex items-center gap-1.5">
                    <span
                      className={`w-2 h-2 rounded-full ${
                        isActive ? 'bg-emerald-400 animate-pulse' : 'bg-text-dim/40'
                      }`}
                    />
                    <span className="font-bold text-bone text-[11px]">{tab.id}</span>
                    {isActive && (
                      <span className="text-[9px] bg-emerald-950 text-emerald-300 border border-emerald-800 px-1 py-0.2 rounded-xs font-bold">
                        ACTIVE
                      </span>
                    )}
                  </div>

                  <div className="flex items-center gap-1">
                    {!isActive && (
                      <button
                        onClick={() => onSwitchTab(tab.id)}
                        disabled={disabled || loading}
                        className="px-2 py-0.5 bg-surface hover:bg-surface-elevated text-accent border border-accent/40 rounded-xs text-[10px] font-bold cursor-pointer"
                        title="Switch active session tab"
                      >
                        SWITCH
                      </button>
                    )}
                    {tabs.length > 1 && (
                      <button
                        onClick={() => handleClose(tab.id)}
                        disabled={disabled || loading}
                        className="px-1.5 py-0.5 hover:bg-danger/20 text-danger border border-danger/30 rounded-xs text-[10px] font-bold cursor-pointer"
                        title="Close tab"
                      >
                        ✕
                      </button>
                    )}
                  </div>
                </div>

                <div className="text-[10px] text-text-dim font-mono break-all truncate">
                  {tab.url || 'about:blank'}
                </div>
              </div>
            );
          })
        )}
      </div>

      <div className="text-[10px] text-text-muted leading-relaxed">
        Tab refs (<code className="text-accent">@eN</code>) are scoped to the active tab. Switching tabs auto-refreshes perception snapshot.
      </div>
    </div>
  );
}
