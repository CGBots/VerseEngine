import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { ApiError, musicApi, type AdminTrack, type MusicLibrary } from '../api';
import type { Translator } from '../i18n';
import {
  draftOf, filterSpots, formatBytes, formatDuration, isDirty, spots, titleFromFileName, toggleSpot, uploadBlocker, type Spot, type TrackDraft,
} from './musicModel';

const errKey = (e: unknown) => (e instanceof ApiError ? e.key : 'activity__internal_error');
/** Au-delà, la liste des lieux et routes demande une recherche (cartes Azgaar de centaines de lieux). */
const MAX_LISTED = 150;

interface Props {
  t: Translator;
  onClose: () => void;
}

/** Page admin « Musique » (MUSIC_SPEC.md, étape S4) : bibliothèque de l'univers et assignations. */
export function MusicAdmin({ t, onClose }: Props) {
  const [lib, setLib] = useState<MusicLibrary | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [file, setFile] = useState<File | null>(null);
  const [title, setTitle] = useState('');
  const [rights, setRights] = useState(false);
  const [uploading, setUploading] = useState(false);
  const [previewing, setPreviewing] = useState<string | null>(null);
  const preview = useRef<{ audio: HTMLAudioElement; url: string } | null>(null);
  const fileInput = useRef<HTMLInputElement>(null);
  // Nombres au format de la langue de l'activité (pas celle du navigateur).
  const locale = t('activity_ui__number_locale');
  const units = { mb: t('activity_ui__music_unit_mb'), kb: t('activity_ui__music_unit_kb') };

  const reload = useCallback(() => {
    musicApi.library().then(setLib).catch((e) => setError(errKey(e)));
  }, []);
  useEffect(reload, [reload]);

  const stopPreview = useCallback(() => {
    if (preview.current) {
      preview.current.audio.pause();
      URL.revokeObjectURL(preview.current.url);
      preview.current = null;
    }
    setPreviewing(null);
  }, []);
  useEffect(() => stopPreview, [stopPreview]);

  const listen = async (id: string) => {
    if (previewing === id) return stopPreview();
    stopPreview();
    try {
      const url = await musicApi.trackUrl(id);
      const audio = new Audio(url);
      audio.addEventListener('ended', stopPreview);
      preview.current = { audio, url };
      setPreviewing(id);
      await audio.play();
    } catch (e) {
      stopPreview();
      setError(errKey(e));
    }
  };

  const blocker = lib ? uploadBlocker(file, title, rights, lib) : 'activity_ui__working';
  const upload = async () => {
    if (!file || blocker) return;
    setUploading(true);
    setError(null);
    try {
      await musicApi.upload(await file.arrayBuffer(), title.trim(), rights);
      setFile(null);
      setTitle('');
      setRights(false);
      if (fileInput.current) fileInput.current.value = '';
      reload();
    } catch (e) {
      setError(errKey(e));
    } finally {
      setUploading(false);
    }
  };

  const allSpots = useMemo(() => (lib ? spots(lib) : []), [lib]);

  return (
    <div className="music-admin">
      <header className="import-header">
        <h2>{t('activity_ui__music_admin_title')}</h2>
        <button type="button" className="secondary small" onClick={onClose}>{t('activity_ui__close')}</button>
      </header>
      {error && <p className="import-error" role="alert">{t(error)}</p>}
      {!lib ? (
        <p className="muted">{t('activity_ui__working')}</p>
      ) : (
        <>
          <p className="muted">
            {t('activity_ui__music_usage', {
              used: formatBytes(lib.used_bytes, locale, units),
              max: formatBytes(lib.max_bytes, locale, units),
              count: lib.tracks.length,
              maxCount: lib.max_tracks,
            })}
          </p>

          <section className="music-upload">
            <h3>{t('activity_ui__music_add')}</h3>
            <label className="file-input">
              <input
                ref={fileInput}
                type="file"
                accept="audio/*,.mp3,.ogg,.opus,.flac,.wav,.m4a,.webm"
                disabled={uploading}
                onChange={(e) => {
                  const f = e.target.files?.[0] ?? null;
                  setFile(f);
                  if (f && !title.trim()) setTitle(titleFromFileName(f.name));
                }}
              />
              <span>{file ? file.name : t('activity_ui__music_choose_file')}</span>
            </label>
            <label>
              {t('activity_ui__music_track_title')}
              <input type="text" maxLength={100} value={title} onChange={(e) => setTitle(e.target.value)} />
            </label>
            <label className="music-rights">
              <input type="checkbox" checked={rights} onChange={(e) => setRights(e.target.checked)} />
              {t('activity_ui__music_rights')}
            </label>
            <p className="muted">{t('activity_ui__music_formats', { max: formatBytes(lib.max_file_bytes, locale, units) })}</p>
            <button type="button" disabled={!!blocker || uploading} title={blocker ? t(blocker) : undefined} onClick={() => void upload()}>
              {uploading ? t('activity_ui__music_checking') : t('activity_ui__music_upload')}
            </button>
          </section>

          {lib.youtube_enabled && <YoutubeImport t={t} onDone={reload} onError={setError} />}

          <section>
            <h3>{t('activity_ui__music_library')}</h3>
            {lib.tracks.length === 0 ? (
              <p className="muted">{t('activity_ui__music_empty')}</p>
            ) : (
              <ul className="music-tracks">
                {lib.tracks.map((track) => (
                  <TrackRow
                    key={track.id}
                    t={t}
                    track={track}
                    allSpots={allSpots}
                    size={formatBytes(track.bytes, locale, units)}
                    previewing={previewing === track.id}
                    onListen={() => void listen(track.id)}
                    onSaved={reload}
                    onDeleted={() => {
                      if (previewing === track.id) stopPreview();
                      reload();
                    }}
                    onError={setError}
                  />
                ))}
              </ul>
            )}
          </section>
        </>
      )}
    </div>
  );
}

function TrackRow({ t, track, allSpots, size, previewing, onListen, onSaved, onDeleted, onError }: {
  t: Translator;
  track: AdminTrack;
  allSpots: Spot[];
  size: string;
  previewing: boolean;
  onListen: () => void;
  onSaved: () => void;
  onDeleted: () => void;
  onError: (key: string) => void;
}) {
  const [draft, setDraft] = useState<TrackDraft>(() => draftOf(track));
  const [picking, setPicking] = useState(false);
  const [query, setQuery] = useState('');
  const [saving, setSaving] = useState(false);
  const [confirmDelete, setConfirmDelete] = useState(false);
  useEffect(() => setDraft(draftOf(track)), [track]);

  const assigned = allSpots.filter((s) => (s.kind === 'place' ? draft.places : draft.roads).includes(s.id));
  const found = filterSpots(allSpots, query);
  const dirty = isDirty(track, draft);

  const save = async () => {
    setSaving(true);
    try {
      await musicApi.update(track.id, { title: draft.title.trim(), places: draft.places, roads: draft.roads });
      onSaved();
    } catch (e) {
      onError(errKey(e));
    } finally {
      setSaving(false);
    }
  };
  const remove = async () => {
    try {
      await musicApi.remove(track.id);
      onDeleted();
    } catch (e) {
      onError(errKey(e));
    }
  };

  return (
    <li className="music-track">
      <div className="music-track-head">
        <input type="text" aria-label={t('activity_ui__music_track_title')} maxLength={100} value={draft.title} onChange={(e) => setDraft({ ...draft, title: e.target.value })} />
        <span className="muted">
          {formatDuration(track.duration_ms)} · {size}
          {track.origin === 'youtube' && ` · ${t('activity_ui__music_from_youtube')}`}
        </span>
        <button type="button" className="secondary small" aria-pressed={previewing} onClick={onListen}>
          {previewing ? t('activity_ui__music_stop') : t('activity_ui__music_listen')}
        </button>
      </div>
      <div className="music-spots">
        {assigned.length === 0 ? (
          <span className="muted">{t('activity_ui__music_nowhere')}</span>
        ) : (
          assigned.map((s) => (
            <button key={`${s.kind}${s.id}`} type="button" className={`chip ${s.kind}`} title={t('activity_ui__music_unassign')} onClick={() => setDraft(toggleSpot(draft, s))}>
              {s.kind === 'road' ? '⇄ ' : ''}{s.label} ×
            </button>
          ))
        )}
        <button type="button" className="secondary small" aria-expanded={picking} onClick={() => setPicking((p) => !p)}>
          {t('activity_ui__music_assign')}
        </button>
      </div>
      {picking && (
        <div className="music-picker">
          <input type="search" placeholder={t('activity_ui__music_search')} value={query} onChange={(e) => setQuery(e.target.value)} />
          <ul>
            {found.slice(0, MAX_LISTED).map((s) => (
              <li key={`${s.kind}${s.id}`}>
                <label>
                  <input type="checkbox" checked={(s.kind === 'place' ? draft.places : draft.roads).includes(s.id)} onChange={() => setDraft(toggleSpot(draft, s))} />
                  {s.kind === 'road' ? '⇄ ' : ''}{s.label}
                </label>
              </li>
            ))}
          </ul>
          {found.length > MAX_LISTED && <p className="muted">{t('activity_ui__music_more', { count: found.length - MAX_LISTED })}</p>}
        </div>
      )}
      <div className="import-nav">
        {confirmDelete ? (
          <>
            <button type="button" className="secondary small" onClick={() => setConfirmDelete(false)}>{t('activity_ui__back')}</button>
            <button type="button" className="danger small" onClick={() => void remove()}>{t('activity_ui__music_confirm_delete')}</button>
          </>
        ) : (
          <button type="button" className="secondary small" onClick={() => setConfirmDelete(true)}>{t('activity_ui__music_delete')}</button>
        )}
        <button type="button" className="small" disabled={!dirty || saving || !draft.title.trim()} onClick={() => void save()}>
          {t('activity_ui__music_save')}
        </button>
      </div>
    </li>
  );
}

/** Import du son d'une vidéo YouTube (MUSIC_SPEC.md § 3.3) : la tâche tourne sur le serveur. */
function YoutubeImport({ t, onDone, onError }: { t: Translator; onDone: () => void; onError: (key: string) => void }) {
  const [url, setUrl] = useState('');
  const [title, setTitle] = useState('');
  const [rights, setRights] = useState(false);
  const [job, setJob] = useState<string | null>(null);

  useEffect(() => {
    if (!job) return;
    const id = window.setInterval(async () => {
      try {
        const state = await musicApi.youtubeJob(job);
        if (state.status === 'running') return;
        setJob(null);
        if (state.status === 'failed') onError(state.error);
        else {
          setUrl('');
          setTitle('');
          setRights(false);
          onDone();
        }
      } catch (e) {
        setJob(null);
        onError(errKey(e));
      }
    }, 2000);
    return () => window.clearInterval(id);
  }, [job, onDone, onError]);

  const blocker = !url.trim() ? 'activity_ui__music_youtube_url' : !rights ? 'activity__music_rights_required' : null;
  const start = async () => {
    try {
      const res = await musicApi.youtube(url.trim(), title, rights);
      setJob(res.job_id);
    } catch (e) {
      onError(errKey(e));
    }
  };

  return (
    <section className="music-upload">
      <h3>{t('activity_ui__music_youtube_title')}</h3>
      <label>
        {t('activity_ui__music_youtube_url')}
        <input type="url" inputMode="url" placeholder="https://youtu.be/…" value={url} disabled={!!job} onChange={(e) => setUrl(e.target.value)} />
      </label>
      <label>
        {t('activity_ui__music_youtube_track_title')}
        <input type="text" maxLength={100} value={title} disabled={!!job} onChange={(e) => setTitle(e.target.value)} />
      </label>
      <label className="music-rights">
        <input type="checkbox" checked={rights} disabled={!!job} onChange={(e) => setRights(e.target.checked)} />
        {t('activity_ui__music_rights')}
      </label>
      <p className="muted">{t('activity_ui__music_youtube_help')}</p>
      <button type="button" disabled={!!blocker || !!job} title={blocker ? t(blocker) : undefined} onClick={() => void start()}>
        {job ? t('activity_ui__music_youtube_running') : t('activity_ui__music_youtube_import')}
      </button>
    </section>
  );
}
