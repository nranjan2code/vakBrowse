import { useState } from 'react';
import type { TabInfo } from '../lib/types';

interface Props {
  tabs: TabInfo[];
  activeTabId: string | null;
  onSwitchTab: (tabId: string) => void;
  onNewTab: (url?: string) => void;
  onCloseTab: (tabId: string) => void;
  disabled?: boolean;
}

export function TabBar({
  tabs,
  activeTabId,
  onSwitchTab,
  onNewTab,
  onCloseTab,
  disabled = false,
}: Props) {
  const [showNewTabInput, setShowNewTabInput] = useState(false);
  const [newTabUrl, setNewTabUrl] = useState('');

  const handleCreateTab = (e: React.FormEvent) => {
    e.preventDefault();
    onNewTab(newTabUrl.trim() || undefined);
    setNewTabUrl('');
    setShowNewTabInput(false);
  };

  const getDomainFromUrl = (url: string) => {
    try {
      if (!url || url === 'about:blank') return 'about:blank';
      const parsed = new URL(url);
      return parsed.hostname + (parsed.pathname !== '/' ? parsed.pathname.slice(0, 15) : '');
    } catch {
      return url.slice(0, 20);
    }
  };

  return (
    <div className="flex items-center bg-[#0d0d12] border-b border-border px-2 pt-1 gap-1 text-[11px] select-none overflow-x-auto">
      <div className="flex items-center gap-1 overflow-x-auto py-0.5">
        {tabs.length === 0 ? (
          <div className="px-3 py-1 bg-surface text-text-dim rounded-t-xs text-[10px] font-mono border-t-2 border-transparent">
            t0: main
          </div>
        ) : (
          tabs.map((tab) => {
            const isActive = activeTabId ? tab.id === activeTabId : tabs[0].id === tab.id;
            return (
              <div
                key={tab.id}
                onClick={() => !disabled && onSwitchTab(tab.id)}
                className={`group flex items-center gap-2 px-3 py-1 rounded-t-xs cursor-pointer transition-all border-t-2 ${
                  isActive
                    ? 'bg-surface text-bone border-accent font-semibold shadow-xs'
                    : 'bg-[#121218] hover:bg-surface/60 text-text-dim hover:text-bone border-transparent'
                }`}
                title={`${tab.id}: ${tab.url}`}
              >
                <span
                  className={`w-1.5 h-1.5 rounded-full shrink-0 ${
                    isActive ? 'bg-emerald-400 animate-pulse' : 'bg-text-dim/50'
                  }`}
                />
                <span className="font-mono text-[10px] text-accent shrink-0">{tab.id}</span>
                <span className="truncate max-w-[130px] text-[10px] font-mono">
                  {getDomainFromUrl(tab.url)}
                </span>
                {tabs.length > 1 && (
                  <button
                    onClick={(e) => {
                      e.stopPropagation();
                      if (!disabled) onCloseTab(tab.id);
                    }}
                    className="text-text-dim hover:text-danger rounded-xs px-1 text-[10px] opacity-70 group-hover:opacity-100 transition-opacity ml-1 cursor-pointer"
                    title={`Close tab ${tab.id}`}
                  >
                    ✕
                  </button>
                )}
              </div>
            );
          })
        )}
      </div>

      {/* New Tab Button / Input */}
      {showNewTabInput ? (
        <form onSubmit={handleCreateTab} className="flex items-center gap-1 shrink-0 ml-1 py-0.5">
          <input
            type="text"
            value={newTabUrl}
            onChange={(e) => setNewTabUrl(e.target.value)}
            placeholder="https://... (or blank)"
            autoFocus
            className="te-input text-[10px] py-0.5 px-2 w-48 font-mono bg-bg border-border rounded-xs"
          />
          <button
            type="submit"
            disabled={disabled}
            className="px-2 py-0.5 bg-accent hover:bg-accent/90 text-white rounded-xs text-[10px] font-bold cursor-pointer"
          >
            OPEN
          </button>
          <button
            type="button"
            onClick={() => setShowNewTabInput(false)}
            className="px-1 py-0.5 text-text-dim hover:text-bone text-[10px] cursor-pointer"
          >
            ✕
          </button>
        </form>
      ) : (
        <button
          onClick={() => setShowNewTabInput(true)}
          disabled={disabled}
          className="px-2 py-1 bg-surface/40 hover:bg-surface text-text-dim hover:text-bone rounded-t-xs text-[11px] font-mono transition-colors shrink-0 ml-1 cursor-pointer"
          title="Open new tab (Action::NewTab)"
        >
          +
        </button>
      )}

      <div className="ml-auto text-[9px] font-mono text-text-dim/60 uppercase shrink-0 px-2">
        TABS: {tabs.length || 1} ACTIVE
      </div>
    </div>
  );
}
