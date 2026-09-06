import { useState, useEffect } from 'react';
import { rpc, openSession, closeSession } from '../lib/actions';
import type { SessionId, Snapshot, ResponsePayload } from '../lib/types';

export interface SessionInfo {
  id: string;
  url: string;
  profile: string | null;
}

export function useSessions() {
  const [sessions, setSessions] = useState<SessionInfo[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const load = async () => {
    setLoading(true);
    try {
      const data = await rpc({ type: 'list_sessions' });
      setSessions(data.Ok?.Sessions ?? []);
    } catch (e: any) {
      setError(e.message);
    } finally {
      setLoading(false);
    }
  };

  const open = async (opts: Record<string, unknown> = {}) => {
    setError(null);
    const result = await openSession(opts);
    await load();
    return result;
  };

  const close = async (sid: SessionId) => {
    await closeSession(sid);
    await load();
  };

  useEffect(() => {
    load();
  }, []);

  return { sessions, loading, error, open, close, reload: load };
}
