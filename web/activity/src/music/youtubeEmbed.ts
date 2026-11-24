/**
 * Lecteur officiel de YouTube (MUSIC_SPEC.md § 3.3) : rien n'est téléchargé, la vidéo est lue dans
 * le cadre de YouTube, visible (200 × 200 au moins). Il est piloté par messages (`enablejsapi`),
 * sans charger de script de YouTube dans l'activité.
 */

/** Taille minimale du lecteur exigée par YouTube. */
export const PLAYER_SIZE = 200;

/**
 * Adresse du lecteur. Dans Discord, tout passe par le proxy de l'activité : URL mapping
 * `/youtube` → `www.youtube-nocookie.com` (voir DEPLOY.md) ; ailleurs (développement), l'adresse
 * directe.
 */
export function embedBase(hostname: string): string {
  return hostname.endsWith('.discordsays.com') ? '/.proxy/youtube' : 'https://www.youtube-nocookie.com';
}

export function embedUrl(base: string, videoId: string, origin: string): string {
  const q = new URLSearchParams({ enablejsapi: '1', autoplay: '1', playsinline: '1', rel: '0', origin });
  return `${base}/embed/${encodeURIComponent(videoId)}?${q}`;
}

/**
 * Attribut `sandbox` du cadre. Par le proxy de Discord, le lecteur aurait la même origine que
 * l'activité : sans `allow-same-origin`, il reste isolé (aucun accès au jeton de session). En
 * accès direct, il garde l'origine de YouTube.
 */
export function sandboxFor(base: string): string {
  const common = 'allow-scripts allow-presentation allow-popups';
  return base.startsWith('/') ? common : `${common} allow-same-origin`;
}

/** Message envoyé au lecteur (protocole `widget` de l'API IFrame). */
export function command(func: 'playVideo' | 'pauseVideo' | 'stopVideo' | 'setVolume', args: unknown[] = []): string {
  return JSON.stringify({ event: 'command', func, args, id: 1, channel: 'widget' });
}

/** Demande au lecteur d'envoyer ses évènements. */
export const LISTENING = JSON.stringify({ event: 'listening', id: 1, channel: 'widget' });

export type PlayerEvent = { kind: 'ready' } | { kind: 'ended' } | { kind: 'playing' } | { kind: 'error'; code: number };

/** Évènement du lecteur, s'il en est un ; `null` sinon. */
export function parseEvent(data: unknown): PlayerEvent | null {
  let msg: unknown = data;
  if (typeof data === 'string') {
    try {
      msg = JSON.parse(data);
    } catch {
      return null;
    }
  }
  if (!msg || typeof msg !== 'object') return null;
  const m = msg as { event?: unknown; info?: unknown };
  const stateOf = (v: unknown) => (typeof v === 'number' ? v : null);
  switch (m.event) {
    case 'onReady':
      return { kind: 'ready' };
    case 'onError':
      return { kind: 'error', code: stateOf(m.info) ?? 0 };
    case 'onStateChange':
      return fromState(stateOf(m.info));
    case 'infoDelivery': {
      const info = m.info as { playerState?: unknown } | null;
      return info && typeof info === 'object' ? fromState(stateOf(info.playerState)) : null;
    }
    default:
      return null;
  }
}

function fromState(state: number | null): PlayerEvent | null {
  if (state === 0) return { kind: 'ended' };
  if (state === 1) return { kind: 'playing' };
  return null;
}
