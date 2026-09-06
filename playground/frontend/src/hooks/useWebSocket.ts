import { useState, useEffect, useRef } from 'react';
import type { ResponsePayload } from '../lib/types';

// WebSocket hook for real-time snapshot pushes / console events.
export function useWebSocket(sid: string | null) {
  const [lastMessage, setLastMessage] = useState<ResponsePayload | null>(null);
  const wsRef = useRef<WebSocket | null>(null);

  useEffect(() => {
    if (!sid) return;

    const proto = location.protocol === 'https:' ? 'wss:' : 'ws:';
    const ws = new WebSocket(`${proto}//${location.host}/ws`);
    wsRef.current = ws;

    ws.onopen = () => {
      ws.send(JSON.stringify({ type: 'subscribe', session: sid }));
    };

    ws.onmessage = (ev) => {
      const resp = JSON.parse(ev.data);
      if ('Ok' in resp) {
        setLastMessage(resp.Ok);
      }
    };

    return () => {
      ws.close();
    };
  }, [sid]);

  return { lastMessage, ws: wsRef.current };
}
