import React, { useState } from 'react';
import type { SessionId } from '../lib/types';

interface SessionInfo {
  id: string;
  url: string;
  profile: string | null;
}

interface Props {
  sessions: SessionInfo[];
  activeSession: string | null;
  onSelect: (sid: string) => void;
  onOpen: (opts: Record<string, unknown>) => Promise<void>;
  onClose: (sid: string) => Promise<void>;
  onList: () => void;
}

export function SessionManager({ sessions, activeSession, onSelect, onOpen, onClose, onList }: Props) {
  return (
    <div className="w-64 border-r border-border p-3 overflow-y-auto bg-card">
      <div className="flex items-center justify-between mb-3">
        <h2 className="text-sm font-medium text-text-dim uppercase">Sessions</h2>
        <button
          onClick={onList}
          className="text-xs text-accent hover:text-accent-hover"
          title="Refresh session list"
        >
          ↻
        </button>
      </div>

      <OpenSessionForm onSubmit={onOpen} />

      <div className="mt-3 space-y-1">
        {sessions.map((s) => (
          <SessionRow
            key={s.id}
            session={s}
            active={s.id === activeSession}
            onSelect={() => onSelect(s.id)}
            onClose={() => onClose(s.id)}
          />
        ))}
        {sessions.length === 0 && (
          <p className="text-xs text-text-dim">No sessions. Open one above.</p>
        )}
      </div>
    </div>
  );
}

function SessionRow({
  session, active, onSelect, onClose
}: { session: SessionInfo; active: boolean; onSelect: () => void; onClose: () => void }) {
  return (
    <div
      className={`
        flex items-center justify-between p-2 rounded text-sm cursor-pointer
        ${active ? 'bg-accent/20 border border-accent' : 'bg-bg border border-border hover:bg-card/70'}
      `}
      onClick={onSelect}
    >
      <div className="flex-1 min-w-0">
        <span className="font-mono text-accent">{session.id}</span>
        {session.profile && <span className="text-xs text-text-dim"> 📋</span>}
        <p className="text-xs text-text-dim truncate">{session.url}</p>
      </div>
      <button
        onClick={(e) => { e.stopPropagation(); onClose(); }}
        className="ml-1 text-xs text-danger hover:text-red-400"
        title="Close session"
      >
        ×
      </button>
    </div>
  );
}

function OpenSessionForm({ onSubmit }: { onSubmit: (opts: Record<string, unknown>) => Promise<void> }) {
  const [url, setUrl] = useState('https://example.com');
  const [stealth, setStealth] = useState(false);
  const [headed, setHeaded] = useState(false);
  const [showAdvanced, setShowAdvanced] = useState(false);
  const [proxy, setProxy] = useState('');
  const [backend, setBackend] = useState<'cdp' | 'dom'>('cdp');

  const handleSubmit = () => {
    const opts: Record<string, unknown> = {
      url,
      stealth,
      headed,
      backend,
    };
    if (proxy) opts.proxy = proxy;
    onSubmit(opts);
    // Reset
    setUrl('https://example.com');
  };

  return (
    <div className="space-y-2">
      <input
        type="url"
        placeholder="URL to open..."
        value={url}
        onChange={(e) => setUrl(e.target.value)}
        className="w-full px-2 py-1 bg-bg border border-border rounded text-sm text-text"
      />
      <div className="flex gap-2">
        <button onClick={handleSubmit} className="btn-primary flex-1">
          Open
        </button>
        <button
          onClick={() => setShowAdvanced(!showAdvanced)}
          className="px-2 py-1 bg-border rounded text-xs"
          title="Advanced options"
        >
          ⚙
        </button>
      </div>

      {showAdvanced && (
        <div className="space-y-1 text-xs">
          <label className="flex items-center gap-1">
            <input type="checkbox" checked={stealth} onChange={(e) => setStealth(e.target.checked)} />
            <span>Stealth</span>
          </label>
          <label className="flex items-center gap-1">
            <input type="checkbox" checked={headed} onChange={(e) => setHeaded(e.target.checked)} />
            <span>Headed (visible window)</span>
          </label>
          <label className="flex items-center gap-2">
            <span>Backend:</span>
            <select
              value={backend}
              onChange={(e) => setBackend(e.target.value as 'cdp' | 'dom')}
              className="bg-bg border border-border rounded px-1 py-0.5 text-text"
            >
              <option value="cdp">CDP (Chrome)</option>
              <option value="dom">DOM (pure Rust, no Chrome)</option>
            </select>
          </label>
          <label className="block">
            <span className="text-text-dim">Proxy:</span>
            <input
              type="text"
              placeholder="socks5://host:port or http://host:port"
              value={proxy}
              onChange={(e) => setProxy(e.target.value)}
              className="w-full px-1 py-0.5 bg-bg border border-border rounded text-text"
            />
          </label>
        </div>
      )}
    </div>
  );
}
