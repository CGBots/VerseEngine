import { useEffect, useLayoutEffect, useRef, useState, type PointerEvent as ReactPointerEvent } from 'react';
import { clampDock, loadDock, saveDock, snapDock, type DockPos } from '../music/dock';
import { musicApi } from '../api';
import type { Translator } from '../i18n';
import { MusicPlayer, type PlayerState } from '../music/player';
import { loadSettings, saveSettings, type MusicSettings } from '../music/settings';
import { MusicIcon, MutedIcon, SpeakerIcon } from './icons/Icons';
import { YouTubeFrame, type VideoController } from './YouTubeFrame';
import { command, insideDiscord } from '../music/youtubeEmbed';

interface Props {
  t: Translator;
  /** Incrémenté quand la position du joueur ou la bibliothèque a pu changer : on relit la playlist. */
  version: number;
}

/**
 * Musique de l'endroit où se trouve le joueur (MUSIC_SPEC.md § 4) : lecteur invisible et petite
 * commande (titre, coupure, volume). Rien ne s'affiche tant qu'aucune musique n'est prévue ici.
 */
export function MusicControl({ t, version }: Props) {
  const player = useRef<MusicPlayer | null>(null);
  const [state, setState] = useState<PlayerState>({ title: null, blocked: false, video: null, hasVideos: false });
  // Volume du lecteur YouTube, transmis par messages une fois le lecteur prêt.
  const video = useRef<VideoController>({ volume: 0, post: null });
  const [settings, setSettings] = useState<MusicSettings>(loadSettings);
  // Lecteur flottant : déplacé par sa poignée, aimanté aux bords de la carte.
  const dock = useRef<HTMLDivElement>(null);
  const [pos, setPos] = useState<DockPos | null>(loadDock);
  const drag = useRef<{ dx: number; dy: number; id: number } | null>(null);
  const bounds = () => {
    const el = dock.current;
    const parent = el?.offsetParent as HTMLElement | null;
    return el && parent ? { w: el.offsetWidth, h: el.offsetHeight, W: parent.clientWidth, H: parent.clientHeight } : null;
  };
  // Carte redimensionnée (rotation, volet) : le lecteur reste dedans.
  useLayoutEffect(() => {
    const parent = dock.current?.offsetParent as HTMLElement | null;
    if (!parent || !pos) return;
    const ro = new ResizeObserver(() => {
      const b = bounds();
      if (b) setPos((p) => (p ? clampDock(p, b.w, b.h, b.W, b.H) : p));
    });
    ro.observe(parent);
    return () => ro.disconnect();
  }, [pos === null]);
  const onGripDown = (e: ReactPointerEvent<HTMLButtonElement>) => {
    const el = dock.current;
    if (!el) return;
    e.preventDefault();
    e.currentTarget.setPointerCapture(e.pointerId);
    drag.current = { dx: e.clientX - el.offsetLeft, dy: e.clientY - el.offsetTop, id: e.pointerId };
  };
  const onGripMove = (e: ReactPointerEvent<HTMLButtonElement>) => {
    const d = drag.current;
    const b = bounds();
    if (!d || d.id !== e.pointerId || !b) return;
    setPos(clampDock({ x: e.clientX - d.dx, y: e.clientY - d.dy }, b.w, b.h, b.W, b.H));
  };
  const onGripUp = (e: ReactPointerEvent<HTMLButtonElement>) => {
    const d = drag.current;
    drag.current = null;
    const b = bounds();
    if (!d || d.id !== e.pointerId || !b || !pos) return;
    const snapped = snapDock(pos, b.w, b.h, b.W, b.H);
    setPos(snapped);
    saveDock(snapped);
  };

  useEffect(() => {
    const p = new MusicPlayer(
      () => {
        const audio = new Audio();
        audio.preload = 'auto';
        return audio;
      },
      musicApi.trackUrl,
      setState,
      undefined,
      {
        setVolume: (v) => {
          // Seulement quand le volume change vraiment (le fondu en demande 20 par seconde).
          const changed = Math.round(v * 100) !== Math.round(video.current.volume * 100);
          video.current.volume = v;
          if (changed) video.current.post?.(command('setVolume', [Math.round(v * 100)]));
        },
      },
    );
    // Dans Discord, le lecteur YouTube ne peut pas jouer (la sécurité de Discord bloque les
    // serveurs de YouTube) : les vidéos sont sautées d'emblée, seuls les fichiers audio jouent.
    if (insideDiscord(window.location.hostname)) p.videoFailed();
    player.current = p;
    return () => {
      p.dispose();
      player.current = null;
    };
  }, []);

  useEffect(() => {
    player.current?.setSettings(settings);
    saveSettings(settings);
  }, [settings]);

  useEffect(() => {
    let cancelled = false;
    musicApi
      .here()
      .then((here) => {
        if (cancelled) return;
        player.current?.setPlaylist({ spot: here.spot ? `${here.spot.kind}:${here.spot.id}` : null, tracks: here.tracks });
      })
      .catch(() => {
        // Musique indisponible : l'activité continue sans son.
      });
    return () => {
      cancelled = true;
    };
  }, [version]);

  if (!state.title && !state.hasVideos) return null;
  return (
    <div
      ref={dock}
      className={`music-dock${pos ? ' placed' : ''}`}
      style={pos ? { left: pos.x, top: pos.y } : undefined}
    >
    <div className="music-control" role="group" aria-label={t('activity_ui__music')}>
      <button
        type="button"
        className="icon-button music-grip"
        aria-label={t('activity_ui__music_move')}
        title={t('activity_ui__music_move')}
        onPointerDown={onGripDown}
        onPointerMove={onGripMove}
        onPointerUp={onGripUp}
        onPointerCancel={onGripUp}
        onDoubleClick={() => {
          // Double clic : retour à la place par défaut.
          setPos(null);
          try { localStorage.removeItem('ve.music.dock'); } catch { /* rien à oublier */ }
        }}
      >
        ⠿
      </button>
      {state.blocked ? (
        <button type="button" className="small" onClick={() => player.current?.unlock()}>
          <MusicIcon size={16} /> {t('activity_ui__music_enable')}
        </button>
      ) : (
        <>
          {state.title && (
            <span className="music-title" title={state.title}>
              <MusicIcon size={14} /> {state.title}
            </span>
          )}
          <button
            type="button"
            className="icon-button"
            aria-label={settings.muted ? t('activity_ui__music_unmute') : t('activity_ui__music_mute')}
            aria-pressed={settings.muted}
            onClick={() => setSettings((s) => ({ ...s, muted: !s.muted }))}
          >
            {settings.muted ? <MutedIcon size={16} /> : <SpeakerIcon size={16} />}
          </button>
          <input
            type="range"
            min={0}
            max={1}
            step={0.05}
            value={settings.volume}
            aria-label={t('activity_ui__music_volume')}
            onChange={(e) => setSettings((s) => ({ ...s, volume: Number(e.target.value), muted: false }))}
          />
        </>
      )}
      {state.hasVideos && (
        <label className="music-hide-video" title={t('activity_ui__music_hide_video')}>
          <input type="checkbox" checked={settings.videoHidden} onChange={(e) => setSettings((s) => ({ ...s, videoHidden: e.target.checked }))} />
          <span>{t('activity_ui__music_hide_video')}</span>
        </label>
      )}
    </div>
    {state.video && (
      <YouTubeFrame
        t={t}
        videoId={state.video}
        controller={video.current}
        onEnded={() => player.current?.videoEnded()}
        onFailed={() => player.current?.videoFailed()}
      />
    )}
    </div>
  );
}
