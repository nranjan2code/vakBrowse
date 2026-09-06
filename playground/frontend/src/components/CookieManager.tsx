import React, { useState, useEffect } from 'react';
import type { Cookie } from '../lib/types';

interface Props {
  onGetCookies: () => Promise<any>;
  onSetCookie: (cookie: Cookie) => Promise<any>;
  loading: boolean;
  disabled: boolean;
  currentUrl?: string;
}

export function CookieManager({ onGetCookies, onSetCookie, loading, disabled, currentUrl }: Props) {
  const [cookies, setCookies] = useState<Cookie[]>([]);
  const [fetching, setFetching] = useState(false);
  const [statusMsg, setStatusMsg] = useState<string | null>(null);

  // New Cookie Form State
  const [showAddForm, setShowAddForm] = useState(false);
  const [name, setName] = useState('');
  const [value, setValue] = useState('');
  const [domain, setDomain] = useState('');
  const [path, setPath] = useState('/');
  const [secure, setSecure] = useState(true);
  const [httpOnly, setHttpOnly] = useState(false);

  // Set domain default from current URL
  useEffect(() => {
    if (currentUrl) {
      try {
        const u = new URL(currentUrl);
        setDomain(u.hostname);
      } catch (_) {}
    }
  }, [currentUrl]);

  const loadCookies = async () => {
    if (disabled) return;
    setFetching(true);
    setStatusMsg(null);
    try {
      const res = await onGetCookies();
      const list = Array.isArray(res) ? res : res?.cookies || [];
      setCookies(list);
      setStatusMsg(`Loaded ${list.length} active cookies`);
    } catch (err: any) {
      setStatusMsg(`Failed to load cookies: ${err.message}`);
    } finally {
      setFetching(false);
    }
  };

  useEffect(() => {
    loadCookies();
  }, [disabled]);

  const handleAddCookie = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!name || disabled) return;

    setFetching(true);
    setStatusMsg(null);
    try {
      const newCookie: Cookie = {
        name,
        value,
        domain: domain || (currentUrl ? new URL(currentUrl).hostname : 'localhost'),
        path: path || '/',
        secure,
        http_only: httpOnly,
        same_site: 'Lax',
      };
      await onSetCookie(newCookie);
      setName('');
      setValue('');
      setShowAddForm(false);
      setStatusMsg(`Cookie "${name}" set successfully`);
      await loadCookies();
    } catch (err: any) {
      setStatusMsg(`Failed to set cookie: ${err.message}`);
    } finally {
      setFetching(false);
    }
  };

  return (
    <div className="flex flex-col h-full space-y-3 font-mono text-xs">
      {/* Top Header & Actions */}
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-2">
          <span className="font-bold text-bone">BROWSER COOKIE JAR</span>
          <span className="badge-dim">{cookies.length} COOKIES</span>
        </div>
        <div className="flex items-center gap-1.5">
          <button
            onClick={() => setShowAddForm(!showAddForm)}
            disabled={disabled || loading || fetching}
            className="btn-secondary py-0.5 px-2 text-[10px]"
          >
            {showAddForm ? 'CANCEL' : '+ ADD COOKIE'}
          </button>
          <button
            onClick={loadCookies}
            disabled={disabled || loading || fetching}
            className="btn-primary py-0.5 px-2.5 text-[10px]"
          >
            {fetching ? 'SYNCING...' : '↻ REFRESH'}
          </button>
        </div>
      </div>

      {statusMsg && (
        <div className="text-[10px] text-accent border border-border bg-surface px-2 py-1 rounded-xs">
          {statusMsg}
        </div>
      )}

      {/* Add Cookie Form */}
      {showAddForm && (
        <form onSubmit={handleAddCookie} className="p-3 bg-surface border border-border rounded-xs space-y-2">
          <div className="font-bold text-bone text-[11px]">INJECT SESSION COOKIE</div>
          <div className="grid grid-cols-2 gap-2">
            <div>
              <label className="text-[9px] text-text-muted uppercase">NAME:</label>
              <input
                type="text"
                required
                value={name}
                onChange={(e) => setName(e.target.value)}
                placeholder="auth_token"
                className="input-sm w-full text-[11px]"
              />
            </div>
            <div>
              <label className="text-[9px] text-text-muted uppercase">VALUE:</label>
              <input
                type="text"
                required
                value={value}
                onChange={(e) => setValue(e.target.value)}
                placeholder="secret_value"
                className="input-sm w-full text-[11px]"
              />
            </div>
          </div>

          <div className="grid grid-cols-2 gap-2">
            <div>
              <label className="text-[9px] text-text-muted uppercase">DOMAIN:</label>
              <input
                type="text"
                value={domain}
                onChange={(e) => setDomain(e.target.value)}
                placeholder=".example.com"
                className="input-sm w-full text-[11px]"
              />
            </div>
            <div>
              <label className="text-[9px] text-text-muted uppercase">PATH:</label>
              <input
                type="text"
                value={path}
                onChange={(e) => setPath(e.target.value)}
                placeholder="/"
                className="input-sm w-full text-[11px]"
              />
            </div>
          </div>

          <div className="flex items-center gap-4 pt-1">
            <label className="flex items-center gap-1.5 text-[10px] text-text-dim cursor-pointer">
              <input
                type="checkbox"
                checked={secure}
                onChange={(e) => setSecure(e.target.checked)}
                className="accent-accent"
              />
              <span>Secure</span>
            </label>
            <label className="flex items-center gap-1.5 text-[10px] text-text-dim cursor-pointer">
              <input
                type="checkbox"
                checked={httpOnly}
                onChange={(e) => setHttpOnly(e.target.checked)}
                className="accent-accent"
              />
              <span>HttpOnly</span>
            </label>
            <button
              type="submit"
              disabled={loading || fetching}
              className="btn-primary py-0.5 px-3 text-[11px] ml-auto"
            >
              SAVE COOKIE
            </button>
          </div>
        </form>
      )}

      {/* Cookies Table */}
      <div className="flex-1 min-h-[220px] bg-bg border border-border rounded-xs overflow-y-auto shadow-te-inset">
        {cookies.length === 0 ? (
          <div className="p-6 text-center text-text-dim text-[11px] space-y-1">
            <div className="text-bone font-bold">NO COOKIES FOUND</div>
            <p>The browser has no cookies stored for this domain yet. Cookies set by the server or injected manually will appear here.</p>
          </div>
        ) : (
          <div className="divide-y divide-border/60">
            {cookies.map((c, i) => (
              <div key={`${c.name}-${i}`} className="p-2.5 hover:bg-surface/40 transition-colors space-y-1">
                <div className="flex items-center justify-between">
                  <span className="font-bold text-accent">{c.name}</span>
                  <div className="flex items-center gap-1 text-[9px]">
                    {c.secure && <span className="badge-dim">SECURE</span>}
                    {c.http_only && <span className="badge-dim">HTTPONLY</span>}
                  </div>
                </div>
                <div className="text-[11px] text-emerald-400 truncate max-w-full font-mono bg-card px-1.5 py-0.5 rounded-xs">
                  {c.value}
                </div>
                <div className="flex items-center justify-between text-[10px] text-text-muted pt-0.5">
                  <span>Domain: {c.domain}</span>
                  <span>Path: {c.path}</span>
                </div>
              </div>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}
