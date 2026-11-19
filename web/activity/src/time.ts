/** Décalage d'horloge serveur − client (secondes), mesuré à chaque réponse. */
export class ServerClock {
  private offset = 0;

  sync(serverTime: number, clientNowMs: number = Date.now()): void {
    if (Number.isFinite(serverTime)) this.offset = serverTime - clientNowMs / 1000;
  }

  /** Heure serveur estimée (secondes Unix). */
  now(clientNowMs: number = Date.now()): number {
    return clientNowMs / 1000 + this.offset;
  }

  /** Secondes restantes avant `timestamp` (≥ 0). */
  remaining(timestamp: number, clientNowMs: number = Date.now()): number {
    return Math.max(0, Math.ceil(timestamp - this.now(clientNowMs)));
  }
}

/** 3725 → "1 h 02 min 05 s" ; 65 → "1 min 05 s" ; 9 → "9 s". */
export function formatDuration(totalSeconds: number): string {
  const s = Math.max(0, Math.floor(totalSeconds));
  const d = Math.floor(s / 86400);
  const h = Math.floor((s % 86400) / 3600);
  const m = Math.floor((s % 3600) / 60);
  const sec = s % 60;
  const pad = (n: number) => n.toString().padStart(2, '0');
  if (d > 0) return `${d} j ${pad(h)} h ${pad(m)} min`;
  if (h > 0) return `${h} h ${pad(m)} min ${pad(sec)} s`;
  if (m > 0) return `${m} min ${pad(sec)} s`;
  return `${sec} s`;
}
