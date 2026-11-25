import { useCallback, useEffect, useRef, useState } from 'react';
import { ApiError, importApi, uploadMapFile, type ImportPlan, type JobState, type MapSourceInfo, type PlanResponse, type RemovalInfo, type ServerList, type UploadedMap } from '../api';
import { openExternal } from '../discord';
import type { Translator } from '../i18n';
import { AzgaarStudio } from './AzgaarStudio';
import { GenerateEditor } from './GenerateEditor';
import { ImportPreview } from './ImportPreview';
import {
  blocker, estimatedMinutes, initialChoices, missingServers, nameConfirmed, optionsForSlot, percent, planRequest, removalChoices, serverColor, setSlot, slotCount, summary,
  simplifyLasso, toggleSecret, type ImportChoices,
} from './importModel';

const AZGAAR_URL = 'https://azgaar.github.io/Fantasy-Map-Generator/';
/** Taille max acceptée par le serveur. */
const MAX_MAP_BYTES = 32 * 1024 * 1024;
/** L'aperçu est recalculé après une pause dans les réglages (le serveur limite à 1 par seconde). */
const PLAN_DEBOUNCE_MS = 1200;

const MODE_LABEL = { merge: 'activity_ui__import_mode_merge', reimport: 'activity_ui__import_mode_reimport', replace: 'activity_ui__import_mode_replace' } as const;
const PHASE_LABEL = {
  Creating: 'activity_ui__import_phase_creating',
  Waiting: 'activity_ui__import_phase_waiting',
  Deleting: 'activity_ui__import_phase_deleting',
  RollingBack: 'activity_ui__import_phase_rolling_back',
  Done: 'activity_ui__import_phase_done',
  RolledBack: 'activity_ui__import_phase_rolled_back_phase',
  RollbackIncomplete: 'activity_ui__import_phase_rollback_incomplete_phase',
} as const;

type Step = 'intro' | 'upload' | 'azgaar' | 'generate' | 'options' | 'confirm' | 'running' | 'finished';

interface Props {
  t: Translator;
  /** Nom de l'univers existant ; `null` : l'import crée l'univers. */
  universeName: string | null;
  /** Incrémenté quand le bot rejoint ou quitte un serveur (WebSocket). */
  serversVersion: number;
  onClose?: () => void;
  /** Import terminé, avec le nom de l'univers (affiché par l'animation de naissance). */
  onDone: (universeName: string) => void;
}

const errKey = (e: unknown) => (e instanceof ApiError ? e.key : 'activity__internal_error');

export function ImportWizard({ t, universeName, serversVersion, onClose, onDone }: Props) {
  const [step, setStep] = useState<Step>('intro');
  const [error, setError] = useState<string | null>(null);
  // « Mettre à jour le rendu seulement » : message du résultat, et envoi en cours.
  const [renderResult, setRenderResult] = useState<string | null>(null);
  const [renderBusy, setRenderBusy] = useState(false);
  const renderOnly = async () => {
    setRenderBusy(true);
    setError(null);
    try {
      const r = await importApi.renderOnly();
      setRenderResult(r.result);
    } catch (e) {
      setError(errKey(e));
    } finally {
      setRenderBusy(false);
    }
  };
  const [map, setMap] = useState<UploadedMap | null>(null);
  const [uploading, setUploading] = useState(false);
  const [choices, setChoices] = useState<ImportChoices | null>(null);
  // Outil lasso (aperçu) : glisser trace la zone à importer.
  const [lasso, setLasso] = useState(false);
  const [plan, setPlan] = useState<PlanResponse | null>(null);
  const [planning, setPlanning] = useState(false);
  const [servers, setServers] = useState<ServerList | null>(null);
  const [newName, setNewName] = useState('');
  const [timeModifier, setTimeModifier] = useState('');
  const [fog, setFog] = useState(false);
  const [typedName, setTypedName] = useState('');
  const [job, setJob] = useState<JobState | null>(null);
  // Azgaar reste chargé une fois ouvert : revenir en arrière ne fait pas perdre la carte en cours.
  const [azgaarOpened, setAzgaarOpened] = useState(false);
  const [source, setSource] = useState<'upload' | 'azgaar'>('upload');

  // Liste des serveurs : au départ, et quand le bot rejoint ou quitte un serveur.
  const loadServers = useCallback(() => {
    importApi.servers().then(setServers).catch((e) => setError(errKey(e)));
  }, []);
  useEffect(() => {
    if (step === 'options' || step === 'confirm') loadServers();
  }, [step, serversVersion, loadServers]);

  // Aperçu recalculé (avec une pause) à chaque changement de réglage.
  useEffect(() => {
    if (!choices || step !== 'options') return;
    setPlanning(true);
    const id = window.setTimeout(() => {
      importApi
        .plan(planRequest(choices))
        .then((p) => {
          setPlan(p);
          setError(null);
        })
        .catch((e) => setError(errKey(e)))
        .finally(() => setPlanning(false));
    }, PLAN_DEBOUNCE_MS);
    return () => window.clearTimeout(id);
  }, [choices, step]);

  const onFile = async (file: File | undefined) => {
    if (!file) return;
    if (file.size > MAX_MAP_BYTES) return setError('activity__payload_too_large');
    setSource('upload');
    await upload(await file.arrayBuffer());
  };

  // Même envoi (et mêmes contrôles côté serveur) pour un fichier choisi et une carte venue d'Azgaar.
  const upload = async (bytes: ArrayBuffer) => {
    setUploading(true);
    setError(null);
    try {
      const uploaded = await uploadMapFile(bytes);
      setMap(uploaded);
      setChoices(initialChoices(uploaded));
      setNewName(uploaded.name ?? '');
      setStep('options');
    } catch (e) {
      setError(errKey(e));
    } finally {
      setUploading(false);
    }
  };

  // Suivi de la création, une fois par seconde.
  const jobId = useRef<string | null>(null);
  useEffect(() => {
    if (step !== 'running') return;
    const id = window.setInterval(async () => {
      if (!jobId.current) return;
      try {
        const state = await importApi.job(jobId.current);
        setJob(state);
        if (state.outcome.status !== 'running') setStep('finished');
      } catch (e) {
        setError(errKey(e));
      }
    }, 1000);
    return () => window.clearInterval(id);
  }, [step]);

  const expectedName = universeName ?? newName.trim();
  const execute = async () => {
    if (!choices) return;
    setError(null);
    try {
      const speed = timeModifier.trim() ? Number(timeModifier) : undefined;
      const res = await importApi.execute({
        plan: planRequest(choices),
        confirm_name: typedName,
        new_universe: universeName ? undefined : { name: newName.trim(), time_modifier: speed, fog },
        removal: plan?.removal ? removalChoices(choices) : undefined,
      });
      jobId.current = res.job_id;
      setJob({ progress: { phase: 'Creating', done: 0, total: res.total }, outcome: { status: 'running' } });
      setStep('running');
    } catch (e) {
      setError(errKey(e));
    }
  };

  const update = (f: (c: ImportChoices) => ImportChoices) => setChoices((c) => (c ? f(c) : c));
  const p = plan?.plan ?? null;
  const removal = plan?.removal ?? null;
  const blockKey = blocker(p, removal, choices);

  return (
    <div className="import-wizard">
      <header className="import-header">
        <h2>{t('activity_ui__import_title')}</h2>
        {onClose && step !== 'running' && (
          <button type="button" className="secondary small" onClick={onClose}>{t('activity_ui__close')}</button>
        )}
      </header>
      {error && <p className="import-error" role="alert">{t(error)}</p>}

      {step === 'intro' && universeName && <MapSourceBox t={t} />}
      {step === 'intro' && (
        <div className="import-choices">
          <button type="button" onClick={() => { setAzgaarOpened(true); setStep('azgaar'); }}>{t('activity_ui__import_in_azgaar')}</button>
          <button type="button" className="secondary" onClick={() => setStep('upload')}>{t('activity_ui__import_map')}</button>
          <button type="button" className="secondary" onClick={() => setStep('generate')}>{t('activity_ui__generate_map')}</button>
        </div>
      )}

      {step === 'upload' && (
        <div className="import-upload">
          <h3>{t('activity_ui__import_azgaar_steps')}</h3>
          <ol>
            <li>{t('activity_ui__import_step_open')} <button type="button" className="secondary small" onClick={() => void openExternal(AZGAAR_URL)}>azgaar.github.io</button></li>
            <li>{t('activity_ui__import_step_edit')}</li>
            <li>{t('activity_ui__import_step_save')}</li>
            <li>{t('activity_ui__import_step_upload')}</li>
          </ol>
          <label className="file-input">
            <input type="file" accept=".map,.txt,.gz" disabled={uploading} onChange={(e) => void onFile(e.target.files?.[0])} />
            <span>{uploading ? t('activity_ui__working') : t('activity_ui__import_choose_file')}</span>
          </label>
          <div className="import-nav">
            <button type="button" className="secondary" onClick={() => setStep('intro')}>{t('activity_ui__back')}</button>
          </div>
        </div>
      )}

      {azgaarOpened && (
        <AzgaarStudio
          t={t}
          hidden={step !== 'azgaar'}
          busy={uploading}
          onBack={() => setStep('intro')}
          onError={setError}
          onMap={(bytes) => {
            setSource('azgaar');
            void upload(bytes.slice().buffer);
          }}
        />
      )}

      {step === 'generate' && (
        <GenerateEditor
          t={t}
          onBack={() => setStep('intro')}
          onReady={(generated) => {
            setMap(generated);
            setChoices(initialChoices(generated));
            setPlan(null);
            setStep('options');
          }}
        />
      )}

      {step === 'options' && map && choices && (
        <div className="import-options">
          <div className="import-preview-wrap">
            <div className="lasso-tools" role="toolbar" aria-label={t('activity_ui__import_zone')}>
              <button type="button" className={lasso ? '' : 'secondary'} aria-pressed={lasso} onClick={() => setLasso((l) => !l)}>
                {t(lasso ? 'activity_ui__import_zone_drawing' : 'activity_ui__import_zone_draw')}
              </button>
              {choices.zone.length >= 3 && (
                <button type="button" className="secondary" onClick={() => update((c) => ({ ...c, zone: [] }))}>{t('activity_ui__import_zone_clear')}</button>
              )}
              <span className="muted small-text">
                {t(choices.zone.length >= 3 ? 'activity_ui__import_zone_set' : 'activity_ui__import_zone_hint', { burgs: p?.places.length ?? map.burgs.length })}
              </span>
            </div>
            <ImportPreview
              map={map}
              plan={p}
              secret={choices.secret}
              onToggleSecret={(a, b) => update((c) => ({ ...c, secret: toggleSecret(c.secret, a, b) }))}
              t={t}
              lasso={lasso}
              zone={choices.zone}
              onLasso={(points) => {
                // Pas de tracé (simple clic) : la zone actuelle est gardée.
                const zone = simplifyLasso(points, Math.max(map.width, map.height) / 400);
                if (zone.length >= 3) update((c) => ({ ...c, zone }));
                setLasso(false);
              }}
            />
          </div>
          <aside className="import-settings">
            {map.has_universe && !map.generated && (
              <fieldset className="render-only">
                <legend>{t('activity_ui__import_render_only')}</legend>
                <p className="muted small-text">{t('activity_ui__import_render_only_hint')}</p>
                {renderResult ? (
                  <p role="status">{t(renderResult)}</p>
                ) : (
                  <button type="button" className="secondary" disabled={renderBusy} onClick={() => void renderOnly()}>
                    {renderBusy ? t('activity_ui__working') : t('activity_ui__import_render_only')}
                  </button>
                )}
                {renderResult && onClose && <button type="button" onClick={onClose}>{t('activity_ui__close')}</button>}
              </fieldset>
            )}
            <p className="muted">{t('activity_ui__import_map_info', { name: map.generated ? t('activity_ui__generate_map_name') : map.name ?? '—', burgs: map.burgs.length })}</p>
            {map.has_universe && (
              <fieldset>
                <legend>{t('activity_ui__import_mode')}</legend>
                {(['merge', 'reimport', 'replace'] as const).map((m) => (
                  <label key={m}>
                    <input type="radio" name="mode" checked={choices.mode === m} onChange={() => update((c) => ({ ...c, mode: m }))} />
                    {t(MODE_LABEL[m])}
                  </label>
                ))}
              </fieldset>
            )}
            <label>
              <input type="checkbox" checked={choices.includeSea} onChange={(e) => update((c) => ({ ...c, includeSea: e.target.checked }))} />
              {t('activity_ui__import_include_sea')}
            </label>
            <p className="muted small-text">{t('activity_ui__import_secret_hint')}</p>

            <fieldset>
              <legend>{t('activity_ui__import_servers', { count: slotCount(p, choices) })}</legend>
              {Array.from({ length: slotCount(p, choices) }, (_, i) => (
                <div key={i} className="server-slot">
                  <span className="server-dot" style={{ background: serverColor(i) }} />
                  {i === 0 ? (
                    <span>{servers?.servers.find((s) => s.current)?.name ?? t('activity_ui__import_this_server')}</span>
                  ) : (
                    <select
                      value={choices.extraServers[i - 1] ?? ''}
                      onChange={(e) => update((c) => setSlot(c, i, e.target.value || null))}
                    >
                      <option value="">{t('activity_ui__import_pick_server')}</option>
                      {servers && optionsForSlot(servers.servers, choices, i).map((s) => <option key={s.guild_id} value={s.guild_id}>{s.name}</option>)}
                    </select>
                  )}
                  {p?.servers[i] && <span className="muted small-text">{t('activity_ui__import_server_budget', { places: p.servers[i].budget.places, channels: p.servers[i].budget.channels })}</span>}
                </div>
              ))}
              {missingServers(p) > 0 && <p className="import-warning">{t('activity_ui__import_servers_missing_help', { count: missingServers(p) })}</p>}
              {servers?.invite_url && (
                <button type="button" className="secondary small" onClick={() => void openExternal(servers.invite_url!)}>{t('activity_ui__import_add_server')}</button>
              )}
              <label>
                {t('activity_ui__import_setup')}
                <select value={choices.setup} onChange={(e) => update((c) => ({ ...c, setup: e.target.value as ImportChoices['setup'] }))}>
                  <option value="FullSetup">{t('activity_ui__import_setup_full')}</option>
                  <option value="PartialSetup">{t('activity_ui__import_setup_partial')}</option>
                </select>
              </label>
            </fieldset>

            {choices.mode === 'merge' && map.suggestions.length > 0 && (
              <fieldset>
                <legend>{t('activity_ui__import_attach')}</legend>
                {map.suggestions.map((s) => {
                  const current = [...choices.attach].find(([, place]) => place === s.place_id)?.[0];
                  return (
                    <label key={s.place_id} className="attach-row">
                      <span>{s.place_name}</span>
                      <select
                        value={current ?? ''}
                        onChange={(e) =>
                          update((c) => {
                            const attach = new Map([...c.attach].filter(([, place]) => place !== s.place_id));
                            if (e.target.value) attach.set(Number(e.target.value), s.place_id);
                            return { ...c, attach };
                          })
                        }
                      >
                        <option value="">{t('activity_ui__import_no_attach')}</option>
                        {(s.burgs.length ? s.burgs : []).map((b) => (
                          <option key={b} value={b}>{map.burgs.find((x) => x.id === b)?.name ?? b}</option>
                        ))}
                      </select>
                    </label>
                  );
                })}
              </fieldset>
            )}

            {p && removal && <RemovalSection plan={p} removal={removal} choices={choices} update={update} t={t} />}
            {p && <PlanSummary plan={plan!} t={t} />}
            <div className="import-nav">
              <button type="button" className="secondary" onClick={() => setStep(map.generated ? 'generate' : source)}>{t('activity_ui__back')}</button>
              <button type="button" disabled={planning || !!blockKey} title={blockKey ? t(blockKey) : undefined} onClick={() => setStep('confirm')}>
                {planning ? t('activity_ui__working') : t('activity_ui__import_next')}
              </button>
            </div>
            {blockKey && !planning && <p className="muted small-text">{t(blockKey)}</p>}
          </aside>
        </div>
      )}

      {step === 'confirm' && plan && (
        <div className="import-confirm">
          <PlanSummary plan={plan} t={t} />
          {plan.removal && (
            <p className="import-warning">
              {t('activity_ui__import_removal_warning', { places: plan.plan.remove_places.length, roads: plan.plan.remove_roads.length, characters: plan.removal.characters })}
            </p>
          )}
          {!universeName && (
            <fieldset>
              <legend>{t('activity_ui__import_new_universe')}</legend>
              <label>
                {t('activity_ui__import_universe_name')}
                <input value={newName} maxLength={100} onChange={(e) => setNewName(e.target.value)} />
              </label>
              <label>
                {t('activity_ui__import_time_speed')}
                <input type="number" min={1} max={10000} placeholder="100" value={timeModifier} onChange={(e) => setTimeModifier(e.target.value)} />
              </label>
              <label>
                <input type="checkbox" checked={fog} onChange={(e) => setFog(e.target.checked)} />
                {t('activity_ui__fog_toggle')}
              </label>
            </fieldset>
          )}
          <label>
            {t('activity_ui__import_type_name', { name: expectedName })}
            <input value={typedName} onChange={(e) => setTypedName(e.target.value)} autoComplete="off" />
          </label>
          <div className="import-nav">
            <button type="button" className="secondary" onClick={() => setStep('options')}>{t('activity_ui__back')}</button>
            <button type="button" className="danger" disabled={!nameConfirmed(expectedName, typedName) || !!blocker(plan.plan, plan.removal, choices)} onClick={() => void execute()}>
              {t('activity_ui__import_start')}
            </button>
          </div>
        </div>
      )}

      {(step === 'running' || step === 'finished') && job && (
        <div className="import-progress">
          <p>{t(PHASE_LABEL[job.progress.phase])}</p>
          <div className="progress" role="progressbar" aria-valuemin={0} aria-valuemax={100} aria-valuenow={percent(job.progress)}>
            <div className={`progress-bar${job.progress.phase === 'RollingBack' ? ' rollback' : ''}`} style={{ width: `${percent(job.progress)}%` }} />
          </div>
          <p className="muted">{job.progress.done} / {job.progress.total}</p>
          {job.outcome.status === 'running' && !!job.discord_wait_secs && (
            <p className="import-warning">{t('activity_ui__import_discord_wait', { seconds: job.discord_wait_secs })}</p>
          )}
          {job.outcome.status === 'done' && (
            <>
              <p>{t('activity_ui__import_done')}</p>
              <button type="button" onClick={() => onDone(expectedName)}>{t('activity_ui__import_open_universe')}</button>
            </>
          )}
          {job.outcome.status === 'failed' && (
            <>
              <p className="import-error">{t(job.outcome.error, { hours: Math.ceil((job.discord_wait_secs ?? 0) / 3600) })}</p>
              <p>{t(job.outcome.rolled_back ? 'activity_ui__import_rolled_back' : 'activity_ui__import_rollback_incomplete')}</p>
              <button type="button" className="secondary" onClick={() => setStep('options')}>{t('activity_ui__back')}</button>
            </>
          )}
        </div>
      )}
    </div>
  );
}

function PlanSummary({ plan, t }: { plan: PlanResponse; t: Translator }) {
  const s = summary(plan.plan);
  return (
    <ul className="import-summary">
      <li>{t('activity_ui__import_sum_places', { create: s.createPlaces, kept: s.keptPlaces })}</li>
      <li>{t('activity_ui__import_sum_roads', { create: s.createRoads, updated: s.updatedRoads })}</li>
      <li>{t('activity_ui__import_sum_servers', { count: s.servers, cut: s.cutRoads })}</li>
      {s.isolated > 0 && <li>{t('activity_ui__import_sum_isolated', { count: s.isolated })}</li>}
      <li>{t('activity_ui__import_sum_duration', { objects: plan.discord_objects, minutes: estimatedMinutes(plan.estimated_secs) })}</li>
      {!!plan.discord_wait_secs && (
        <li className="import-warning">{t('activity_ui__import_discord_blocked_now', { hours: Math.ceil(plan.discord_wait_secs / 3600) })}</li>
      )}
    </ul>
  );
}

/** Suppression des lieux existants (I15) : lieu de départ et sort des objets posés (Q2). */
function RemovalSection({ plan, removal, choices, update, t }: {
  plan: ImportPlan;
  removal: RemovalInfo;
  choices: ImportChoices;
  update: (f: (c: ImportChoices) => ImportChoices) => void;
  t: Translator;
}) {
  const places = [...plan.places].sort((a, b) => a.name.localeCompare(b.name));
  const unowned = removal.items.filter((i) => i.owner === null);
  return (
    <fieldset className="removal">
      <legend>{t('activity_ui__import_removal')}</legend>
      <p className="small-text">
        {t('activity_ui__import_removal_counts', { places: plan.remove_places.length, roads: plan.remove_roads.length, characters: removal.characters, travelling: removal.travelling })}
      </p>
      {removal.characters > 0 && (
        <label>
          {t('activity_ui__import_start_place')}
          <select value={choices.startBurg ?? ''} onChange={(e) => update((c) => ({ ...c, startBurg: e.target.value ? Number(e.target.value) : null }))}>
            <option value="">{t('activity_ui__import_pick_place')}</option>
            {places.map((p) => <option key={p.burg_id} value={p.burg_id}>{p.name}</option>)}
          </select>
        </label>
      )}
      {removal.items.filter((i) => i.owner !== null).map((i) => (
        <label key={i.tool_id} className="attach-row">
          <span>{t('activity_ui__import_item', { name: i.name, place: i.place_name, owner: i.owner ?? '', content: i.content })}</span>
          <select
            value={choices.itemActions.get(i.tool_id) ?? 'return'}
            onChange={(e) => update((c) => ({ ...c, itemActions: new Map(c.itemActions).set(i.tool_id, e.target.value as 'destroy' | 'return') }))}
          >
            <option value="return">{t('activity_ui__import_item_return')}</option>
            <option value="destroy">{t('activity_ui__import_item_destroy')}</option>
          </select>
        </label>
      ))}
      {unowned.length > 0 && (
        <>
          <p className="import-warning small-text">{t('activity_ui__import_unowned_list', { count: unowned.length })}</p>
          <ul className="small-text">
            {unowned.map((i) => <li key={i.tool_id}>{t('activity_ui__import_unowned_item', { name: i.name, place: i.place_name, content: i.content })}</li>)}
          </ul>
          <label>
            <input type="checkbox" checked={choices.confirmUnowned} onChange={(e) => update((c) => ({ ...c, confirmUnowned: e.target.checked }))} />
            {t('activity_ui__import_confirm_unowned')}
          </label>
        </>
      )}
    </fieldset>
  );
}

/** Sauvegarde .map gardée pour l'univers : graine, date, et renvoi en message privé. */
function MapSourceBox({ t }: { t: Translator }) {
  const [latest, setLatest] = useState<MapSourceInfo | null | undefined>(undefined);
  const [message, setMessage] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  useEffect(() => {
    importApi.sources().then((r) => setLatest(r.sources[0] ?? null)).catch(() => setLatest(null));
  }, []);
  if (latest === undefined) return null;
  const send = async () => {
    setBusy(true);
    try {
      setMessage(t((await importApi.sendSource()).result));
    } catch (e) {
      setMessage(t(errKey(e)));
    } finally {
      setBusy(false);
    }
  };
  return (
    <section className="map-source">
      <h3>{t('activity_ui__map_source_title')}</h3>
      {latest ? (
        <>
          <p className="muted">
            {t('activity_ui__map_source_info', { seed: latest.seed ?? '?', name: latest.name ?? '—', date: new Date(latest.saved_at * 1000).toLocaleString() })}
          </p>
          <button type="button" className="secondary small" disabled={busy} onClick={() => void send()}>{t('activity_ui__map_source_send')}</button>
        </>
      ) : (
        <p className="muted">{t('activity__map_source_none')}</p>
      )}
      {message && <p role="status">{message}</p>}
    </section>
  );
}
