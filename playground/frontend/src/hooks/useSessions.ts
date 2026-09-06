import { useState, useEffect, useCallback } from 'react';
import { rpc, openSession, closeSession } from '../lib/actions';
import type { SessionId } from '../lib/types';

export interface SessionInfo {
  id: string;
  url: string;
  profile: string | null;
}

export function useSessions() {
  const [sessions, setSessions] = useState<SessionInfo[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(async (isBackground = false) => {
    if (!isBackground) setLoading(true);
    try {
      const data = await rpc({ type: 'list_sessions' });
      const incoming: SessionInfo[] = data.Ok?.Sessions ?? [];
      setSessions((prev) => {
        // Deep comparison: avoid returning new reference if sessions haven't changed
        if (
          prev.length === incoming.length &&
          prev.every(
            (p, i) =>
              p.id === incoming[i].id &&
              p.url === incoming[i].url &&
              p.profile === incoming[i].profile
          )
        ) {
          return prev;
        }
        return incoming;
      });
    } catch (e: any) {
      if (!isBackground) setError(e.message);
    } finally {
      if (!isBackground) setLoading(false);
    }
  }, []);

  const open = async (opts: Record<string, unknown> = {}) => {
    setError(null);
    const result = await openSession(opts);
    await load(false);
    return result;
  };

  const close = async (sid: SessionId) => {
    await closeSession(sid);
    await load(false);
  };

  useEffect(() => {
    load(false);
    // Silent background poll: never triggers global loading overlay
    const interval = setInterval(() => {
      load(true);
    }, 3000);
    return () => clearInterval(interval);
  }, [load]);

  return { sessions, loading, error, open, close, reload: () => load(false) };
}
