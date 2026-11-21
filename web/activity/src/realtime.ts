/** Évènements poussés par le serveur (`/api/ws`) : ils disent quoi relire. */
export type ServerEvent = 'travel_updated' | 'character_updated' | 'map_updated' | 'servers_updated' | 'resync';

const KNOWN: readonly string[] = ['travel_updated', 'character_updated', 'map_updated', 'servers_updated', 'resync'];

/** Délai avant la n-ième tentative de reconnexion : 1, 2, 4… plafonné à 30 s. */
export function backoffMs(attempt: number): number {
  return Math.min(30_000, 1000 * 2 ** Math.max(0, attempt));
}

/**
 * WebSocket authentifiée par premier message (le jeton n'apparaît pas dans l'URL). Se reconnecte
 * avec back-off et émet `resync` à chaque reconnexion, pour tout relire.
 */
export function connectRealtime(getToken: () => string | null, onEvent: (e: ServerEvent) => void): () => void {
  let socket: WebSocket | null = null;
  let attempt = 0;
  let stopped = false;
  let timer: number | undefined;

  const open = () => {
    const token = getToken();
    if (stopped || !token) return;
    const url = `${location.protocol === 'https:' ? 'wss' : 'ws'}://${location.host}/api/ws`;
    socket = new WebSocket(url);
    socket.onopen = () => socket?.send(JSON.stringify({ type: 'auth', token }));
    socket.onmessage = (msg) => {
      try {
        const data = JSON.parse(String(msg.data)) as { type?: string };
        if (data.type === 'hello') {
          if (attempt > 0) onEvent('resync');
          attempt = 0;
        } else if (data.type && KNOWN.includes(data.type)) {
          onEvent(data.type as ServerEvent);
        }
      } catch {
        /* message ignoré */
      }
    };
    socket.onclose = () => {
      if (stopped) return;
      timer = window.setTimeout(open, backoffMs(attempt));
      attempt += 1;
    };
  };

  open();
  return () => {
    stopped = true;
    window.clearTimeout(timer);
    socket?.close();
  };
}
