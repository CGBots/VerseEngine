import { useCallback, useEffect, useState } from 'react';
import { createPortal } from 'react-dom';
import { ApiError, placeApi, type PlaceSheetView, type PublishReport, type PublishStatus } from '../api';
import { openExternal } from '../discord';
import type { Translator } from '../i18n';
import { charCount, formatPopulation } from '../placeSheet';
import { CloseIcon } from './icons/Icons';
import { LootTableEditor } from './LootTableEditor';
import { burgSummary } from './PlaceLore';

// Clés écrites en entier : le test des traductions les cherche telles quelles dans le code.
const STATUS_KEY: Record<PublishStatus, string> = {
  created: 'activity_ui__wiki_created',
  updated: 'activity_ui__wiki_updated',
  recreated: 'activity_ui__wiki_recreated',
  no_forum: 'activity_ui__wiki_no_forum',
  failed: 'activity_ui__wiki_failed',
};
const errorKey = (e: unknown) => (e instanceof ApiError ? e.key : 'activity__internal_error');

interface Props {
  placeId: string;
  t: Translator;
  locale: string;
  /** Incrémenté à chaque `map_updated` : la fiche se relit (lieu modifié ailleurs). */
  version: number;
  onClose: () => void;
}

/** Fiche d'un lieu en mode admin : informations, texte de la fiche (enregistrer, publier dans le
 *  forum wiki de chaque serveur) et accès à l'éditeur de sa table de butin. */
export function PlaceSheet({ placeId, t, locale, version, onClose }: Props) {
  const [sheet, setSheet] = useState<PlaceSheetView | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [draft, setDraft] = useState('');
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState<string | null>(null);
  const [report, setReport] = useState<PublishReport[] | null>(null);
  const [lootOpen, setLootOpen] = useState(false);

  /** Applique l'état relu sur le serveur ; le texte saisi est remplacé seulement si demandé. */
  const apply = useCallback((s: PlaceSheetView, resetDraft: boolean) => {
    setSheet(s);
    if (resetDraft) setDraft(s.description ?? '');
  }, []);

  useEffect(() => {
    setReport(null);
    setMessage(null);
    setLootOpen(false);
  }, [placeId]);

  useEffect(() => {
    let alive = true;
    placeApi
      .sheet(placeId)
      .then((s) => {
        if (!alive) return;
        setLoadError(null);
        // Une relecture (temps réel) ne doit pas écraser un texte en cours de saisie.
        setSheet((prev) => {
          if (!prev || prev.id !== s.id) setDraft(s.description ?? '');
          return s;
        });
      })
      .catch((e) => alive && setLoadError(errorKey(e)));
    return () => {
      alive = false;
    };
  }, [placeId, version]);

  const dirty = sheet !== null && draft.trim() !== (sheet.description ?? '').trim();
  const tooLong = sheet !== null && charCount(draft.trim()) > sheet.max_description;

  const save = async () => {
    if (!sheet) return null;
    const s = await placeApi.saveDescription(sheet.id, draft);
    apply(s, true);
    return s;
  };
  const run = async (action: () => Promise<void>) => {
    setBusy(true);
    setMessage(null);
    try {
      await action();
    } catch (e) {
      setMessage(t(errorKey(e)));
    } finally {
      setBusy(false);
    }
  };
  const onSave = () => void run(async () => {
    await save();
    setMessage(t('activity_ui__place_saved'));
  });
  // Publier = publier le texte enregistré : un texte modifié est d'abord enregistré.
  const onPublish = () => void run(async () => {
    if (!sheet) return;
    if (dirty) await save();
    const r = await placeApi.publish(sheet.id);
    apply(r.sheet, true);
    setReport(r.report);
  });

  const header = (
    <header className="window-header">
      <h2>{sheet?.name ?? t('activity_ui__place_sheet')}</h2>
      <button type="button" className="icon-button" aria-label={t('activity_ui__close')} title={t('activity_ui__close')} onClick={onClose}>
        <CloseIcon size={18} />
      </button>
    </header>
  );
  if (!sheet)
    return (
      <div className="place-sheet floating-window" role="dialog" aria-label={t('activity_ui__place_sheet')}>
        {header}
        <p className="muted">{loadError ? t(loadError) : t('activity_ui__loading')}</p>
      </div>
    );

  const burg = sheet.burg;
  const channel = (id: string) => `https://discord.com/channels/${sheet.server_id}/${id}`;
  const link = (url: string, label: string) => (
    <a href={url} onClick={(e) => { e.preventDefault(); void openExternal(url); }}>{label}</a>
  );
  return (
    <div className="place-sheet floating-window" role="dialog" aria-label={t('activity_ui__place_sheet')}>
      {header}
      <dl className="place-facts">
        <dt>{t('activity_ui__place_channels')}</dt>
        <dd>
          {link(channel(sheet.id), t('activity_ui__place_category'))}
          {sheet.central_channel_id && <> · {link(channel(sheet.central_channel_id), t('activity_ui__place_central'))}</>}
        </dd>
        {burg && (
          <>
            <dt>{t('activity_ui__place_burg')}</dt>
            <dd>
              {burgSummary(burg, t)}
              {burg.name && burg.name !== sheet.name && <span className="muted"> · {burg.name}</span>}
            </dd>
            {burg.population != null && (
              <>
                <dt>{t('place_wiki__population')}</dt>
                <dd>{formatPopulation(burg.population, locale)}</dd>
              </>
            )}
            {burg.state && (
              <>
                <dt>{t('place_wiki__state')}</dt>
                <dd>{burg.state}</dd>
              </>
            )}
            {burg.culture && (
              <>
                <dt>{t('place_wiki__culture')}</dt>
                <dd>{burg.culture}</dd>
              </>
            )}
            {(burg.kind || burg.group) && (
              <>
                <dt>{t('activity_ui__place_azgaar_type')}</dt>
                <dd>{[burg.group, burg.kind].filter(Boolean).join(' · ')}</dd>
              </>
            )}
          </>
        )}
        <dt>{t('activity_ui__place_roads')}</dt>
        <dd>
          {sheet.roads.length === 0
            ? t('activity_ui__place_no_road')
            : sheet.roads.map((r) => `${r.to_name ?? r.name} (${r.distance_km} km${r.secret ? `, ${t('activity_ui__place_secret_road')}` : ''})`).join(' · ')}
        </dd>
        <dt>{t('activity_ui__place_players', { count: sheet.players_total })}</dt>
        <dd>{sheet.players.length === 0 ? t('activity_ui__place_nobody') : sheet.players.map((p) => p.name ?? p.user_id).join(', ')}</dd>
      </dl>

      <label className="place-description">
        <span className="label">{t('activity_ui__place_description')}</span>
        <textarea
          value={draft}
          rows={7}
          readOnly={!sheet.can_edit}
          maxLength={sheet.max_description * 2}
          placeholder={sheet.can_edit ? t('activity_ui__place_description_placeholder') : ''}
          onChange={(e) => setDraft(e.target.value)}
        />
        <span className={`muted counter${tooLong ? ' over' : ''}`}>
          {charCount(draft.trim())} / {sheet.max_description}
          {dirty && ` · ${t('activity_ui__place_unsaved')}`}
        </span>
      </label>
      {sheet.published ? (
        <p className="muted">{t('activity_ui__place_published_at', { date: new Date(sheet.published.at * 1000).toLocaleString(locale) })}</p>
      ) : (
        <p className="muted">{t('activity_ui__place_never_published')}</p>
      )}
      <div className="row">
        {sheet.can_edit && (
          <>
            <button type="button" className="secondary" disabled={busy || !dirty || tooLong} onClick={onSave}>{t('activity_ui__place_save')}</button>
            <button type="button" disabled={busy || tooLong || !draft.trim()} onClick={onPublish}>{t('activity_ui__place_publish')}</button>
          </>
        )}
        <button type="button" className="secondary" disabled={busy} onClick={() => setLootOpen(true)}>
          {sheet.can_edit ? t('activity_ui__place_edit_loot') : t('activity_ui__place_view_loot')}
        </button>
      </div>
      {!sheet.can_edit && <p className="muted">{t('activity_ui__place_read_only')}</p>}
      {busy && <p className="muted" role="status">{t('activity_ui__loading')}</p>}
      {message && <p className="travel-message" role="status">{message}</p>}
      {report && (
        <ul className="publish-report" aria-label={t('activity_ui__place_publish_report')}>
          {report.length === 0 && <li className="muted">{t('activity_ui__wiki_no_server')}</li>}
          {report.map((r) => (
            <li key={r.server_id} className={`status-${r.status}`}>
              <span className="name">{r.server_name ?? r.server_id}</span> · {t(STATUS_KEY[r.status])}
              {r.thread_id && <> · {link(`https://discord.com/channels/${r.server_id}/${r.thread_id}`, t('activity_ui__wiki_open_post'))}</>}
            </li>
          ))}
        </ul>
      )}
      {/* Hors de la fenêtre (qui crée son propre empilement) : la modale couvre tout l'écran. */}
      {lootOpen && createPortal(
        <LootTableEditor
          t={t}
          placeId={sheet.id}
          placeName={sheet.name}
          hasCentral={sheet.central_channel_id !== null}
          onClose={() => setLootOpen(false)}
          onSaved={() => void placeApi.sheet(sheet.id).then((s) => apply(s, false)).catch(() => undefined)}
        />,
        document.body,
      )}
    </div>
  );
}
