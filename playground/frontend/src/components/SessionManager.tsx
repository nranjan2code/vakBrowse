import React, { useState } from 'react';
import type { SessionInfo } from '../hooks/useSessions';

interface Props {
  sessions: SessionInfo[];
  activeSession: string | null;
  onSelect: (sid: string) => void;
  onOpen: (options: Record<string, unknown>) => Promise<void>;
  onClose: (sid: string) => Promise<void>;
  onList: () => void;
}

export function SessionManager({
  sessions,
  activeSession,
  onSelect,
  onOpen,
  onClose,
  onList,
}: Props) {
  const [showModal, setShowModal] = useState(false);
  const [url, setUrl] = useState('https://example.com');
  const [stealth, setStealth] = useState(true);
  const [proxy, setProxy] = useState('');
  const [headed, setHeaded] = useState(false);
  const [loading, setLoading] = useState(false);

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    setLoading(true);
    try {
      const opts: Record<string, unknown> = {
        url,
        stealth,
        headed,
      };
      if (proxy.trim()) {
        opts.proxy = proxy.trim();
      }
      await onOpen(opts);
      setShowModal(false);
    } finally {
      setLoading(false);
    }
  };

  return (
    <div className="w-72 border-r border-border bg-card flex flex-col h-full font-mono text-xs select-none">
      {/* Session Manager Header */}
      <div className="p-3 border-b border-border bg-surface flex items-center justify-between">
        <div className="flex items-center gap-2">
          <div className="w-2 h-2 rounded-full bg-accent" />
          <span className="font-bold text-bone uppercase tracking-wider text-xs">
            SESSION DECK
          </span>
        </div>
        <button
          onClick={onList}
          className="text-text-dim hover:text-text text-[11px] uppercase transition-colors"
          title="Refresh active session list"
        >
          [SYNC]
        </button>
      </div>

      {/* Action button: Open Session */}
      <div className="p-3 border-b border-border">
        <button
          onClick={() => setShowModal(true)}
          className="btn-primary w-full py-2 flex items-center justify-center gap-1.5"
        >
          <span>+ NEW BROWSER NODE</span>
        </button>
      </div>

      {/* Session list */}
      <div className="flex-1 overflow-y-auto p-2 space-y-1.5">
        {sessions.length === 0 ? (
          <div className="p-4 text-center text-text-dim text-xs">
            <div className="text-text-muted mb-1">NO ACTIVE SESSIONS</div>
            <p className="text-[11px] leading-relaxed">
              Launch a session or select a preset workflow to initialize an automation node.
            </p>
          </div>
        ) : (
          sessions.map((s) => {
            const isSelected = s.id === activeSession;
            return (
              <div
                key={s.id}
                onClick={() => onSelect(s.id)}
                className={`p-2.5 rounded-xs border cursor-pointer transition-all ${
                  isSelected
                    ? 'bg-surface-elevated border-accent shadow-xs'
                    : 'bg-surface/50 border-border hover:border-border-strong hover:bg-surface'
                }`}
              >
                <div className="flex items-center justify-between mb-1">
                  <span className={`font-bold flex items-center gap-1.5 ${isSelected ? 'text-accent' : 'text-bone'}`}>
                    <span className={`w-1.5 h-1.5 rounded-full ${isSelected ? 'bg-accent' : 'bg-emerald-400'}`} />
                    {s.id}
                  </span>
                  <button
                    onClick={(e) => {
                      e.stopPropagation();
                      onClose(s.id);
                    }}
                    className="text-text-muted hover:text-danger text-[10px] uppercase font-bold px-1 transition-colors"
                    title="Terminate session"
                  >
                    KILL ✕
                  </button>
                </div>
                <div className="text-[11px] text-text-dim truncate font-sans">
                  {s.url || '(empty)'}
                </div>
              </div>
            );
          })
        )}
      </div>

      {/* Session Deck Status Footer */}
      <div className="p-2.5 border-t border-border bg-surface text-[10px] text-text-muted flex justify-between">
        <span>ENGINE: RUST CORE</span>
        <span>ACTIVE: {sessions.length}</span>
      </div>

      {/* Open Session Modal */}
      {showModal && (
        <div className="fixed inset-0 bg-black/75 backdrop-blur-xs flex items-center justify-center p-4 z-50">
          <div className="te-panel rounded-xs border-accent p-5 w-full max-w-md space-y-4 shadow-2xl">
            <div className="flex items-center justify-between pb-2 border-b border-border">
              <span className="font-mono text-xs font-bold text-bone uppercase flex items-center gap-1.5">
                <span className="w-2 h-2 rounded-xs bg-accent" />
                INITIALIZE BROWSER INSTANCE
              </span>
              <button
                onClick={() => setShowModal(false)}
                className="text-text-dim hover:text-text text-xs"
              >
                ✕
              </button>
            </div>

            <form onSubmit={handleSubmit} className="space-y-3.5">
              {/* URL input */}
              <div className="space-y-1">
                <label className="text-[11px] text-text-dim uppercase tracking-wider block">
                  START TARGET URL:
                </label>
                <input
                  type="text"
                  value={url}
                  onChange={(e) => setUrl(e.target.value)}
                  placeholder="https://example.com"
                  className="input-sm w-full font-mono"
                  required
                />
              </div>


              {/* Toggles */}
              <div className="grid grid-cols-2 gap-3 pt-1 font-mono text-xs">
                <label className="flex items-center gap-2 cursor-pointer text-text-dim hover:text-text">
                  <input
                    type="checkbox"
                    checked={stealth}
                    onChange={(e) => setStealth(e.target.checked)}
                    className="accent-accent"
                  />
                  <span>STEALTH EVASIONS</span>
                </label>

                <label className="flex items-center gap-2 cursor-pointer text-text-dim hover:text-text">
                  <input
                    type="checkbox"
                    checked={headed}
                    onChange={(e) => setHeaded(e.target.checked)}
                    className="accent-accent"
                  />
                  <span>HEADED VIEWPORT</span>
                </label>
              </div>

              {/* Proxy Input */}
              <div className="space-y-1 pt-1">
                <label className="text-[11px] text-text-dim uppercase tracking-wider block">
                  OPTIONAL PROXY (HTTP / SOCKS5):
                </label>
                <input
                  type="text"
                  value={proxy}
                  onChange={(e) => setProxy(e.target.value)}
                  placeholder="http://proxy.example.com:8080"
                  className="input-sm w-full font-mono text-xs"
                />
              </div>

              {/* Buttons */}
              <div className="flex items-center justify-end gap-2 pt-3 border-t border-border">
                <button
                  type="button"
                  onClick={() => setShowModal(false)}
                  className="btn-ghost"
                >
                  CANCEL
                </button>
                <button
                  type="submit"
                  disabled={loading}
                  className="btn-primary"
                >
                  {loading ? 'LAUNCHING...' : 'SPAWN BROWSER NODE ›'}
                </button>
              </div>
            </form>
          </div>
        </div>
      )}
    </div>
  );
}
