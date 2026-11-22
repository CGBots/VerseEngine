import { useEffect, useRef } from 'react';
import type { Translator } from '../i18n';
import { command, embedBase, embedUrl, LISTENING, parseEvent, PLAYER_SIZE, sandboxFor } from '../music/youtubeEmbed';

/** Sans signe du lecteur après ce délai, il est considéré comme indisponible ici. */
const READY_TIMEOUT_MS = 15_000;
/** Erreurs propres à une vidéo (retirée, intégration interdite…) : on passe à la suivante. */
const VIDEO_ERRORS = new Set([2, 5, 100, 101, 150, 153]);

/** Lien entre le moteur (volume) et le cadre affiché. */
export interface VideoController {
  volume: number;
  post: ((msg: string) => void) | null;
}

interface Props {
  t: Translator;
  videoId: string;
  controller: VideoController;
  onEnded: () => void;
  onFailed: () => void;
}

/** Lecteur officiel de YouTube, visible (200 × 200), piloté par messages. */
export function YouTubeFrame({ t, videoId, controller, onEnded, onFailed }: Props) {
  const frame = useRef<HTMLIFrameElement>(null);
  const callbacks = useRef({ onEnded, onFailed });
  callbacks.current = { onEnded, onFailed };
  const base = embedBase(location.hostname);

  useEffect(() => {
    let ready = false;
    const win = () => frame.current?.contentWindow ?? null;
    const post = (msg: string) => win()?.postMessage(msg, '*');
    // Le lecteur n'envoie ses évènements qu'après « listening » : on le répète jusqu'à sa réponse.
    const hello = window.setInterval(() => post(LISTENING), 500);
    const timeout = window.setTimeout(() => {
      if (!ready) callbacks.current.onFailed();
    }, READY_TIMEOUT_MS);
    const onMessage = (event: MessageEvent) => {
      if (!win() || event.source !== win()) return;
      const e = parseEvent(event.data);
      if (!e) return;
      if (!ready && (e.kind === 'ready' || e.kind === 'playing')) {
        ready = true;
        window.clearInterval(hello);
        controller.post = post;
        post(command('setVolume', [Math.round(controller.volume * 100)]));
        post(command('playVideo'));
      }
      if (e.kind === 'ended') callbacks.current.onEnded();
      if (e.kind === 'error') {
        if (VIDEO_ERRORS.has(e.code)) callbacks.current.onEnded();
        else callbacks.current.onFailed();
      }
    };
    window.addEventListener('message', onMessage);
    return () => {
      window.clearInterval(hello);
      window.clearTimeout(timeout);
      window.removeEventListener('message', onMessage);
      controller.post = null;
    };
  }, [videoId, controller]);

  return (
    <iframe
      ref={frame}
      key={videoId}
      className="music-video"
      width={PLAYER_SIZE}
      height={PLAYER_SIZE}
      src={embedUrl(base, videoId, location.origin)}
      title={t('activity_ui__music_video_player')}
      sandbox={sandboxFor(base)}
      allow="autoplay; encrypted-media"
      referrerPolicy="strict-origin-when-cross-origin"
    />
  );
}
