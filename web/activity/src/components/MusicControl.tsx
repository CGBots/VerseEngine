import { useEffect, useRef, useState } from 'react';
import { musicApi } from '../api';
import type { Translator } from '../i18n';
import { MusicPlayer, type PlayerState } from '../music/player';
import { loadSettings, saveSettings, type MusicSettings } from '../music/settings';
import { MusicIcon, MutedIcon, SpeakerIcon } from './icons/Icons';
import { YouTubeFrame, type VideoController } from './YouTubeFrame';
import { command } from '../music/youtubeEmbed';

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
    <div className="music-dock">
    <div className="music-control" role="group" aria-label={t('activity_ui__music')}>
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
