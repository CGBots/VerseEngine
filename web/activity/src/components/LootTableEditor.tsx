import { useEffect, useId, useMemo, useState } from 'react';
import { ApiError, placeApi, type LootState, type LootTarget } from '../api';
import type { Translator } from '../i18n';
import { formFromState, formToBody, newEntry, newItemRow, type EntryRow, type ItemRow, type LootError, type LootForm } from '../placeSheet';
import { formatDuration } from '../time';
import { CloseIcon } from './icons/Icons';

const errorKey = (e: unknown) => (e instanceof ApiError ? e.key : 'activity__internal_error');

interface Props {
  t: Translator;
  placeId: string;
  placeName: string;
  hasCentral: boolean;
  onClose: () => void;
  /** Table enregistrée : la fiche relit son résumé. */
  onSaved: () => void;
}

/** Ligne commune (objet seul, en-tête d'ensemble ou objet d'un ensemble). */
function RowFields({ t, row, readOnly, isSetHeader, inSet, listId, onChange, onRemove }: {
  t: Translator;
  row: ItemRow;
  readOnly: boolean;
  isSetHeader: boolean;
  inSet: boolean;
  listId: string;
  onChange: (patch: Partial<ItemRow>) => void;
  onRemove: () => void;
}) {
  return (
    <div className="loot-row">
      <label className="grow">
        <span>{isSetHeader ? t('activity_ui__loot_set_name') : t('activity_ui__loot_item')}</span>
        <input
          value={row.name}
          readOnly={readOnly}
          list={isSetHeader ? undefined : listId}
          autoComplete="off"
          placeholder={isSetHeader ? '' : t('activity_ui__loot_pick_item')}
          onChange={(e) => onChange({ name: e.target.value })}
        />
      </label>
      <label>
        <span>{inSet ? t('activity_ui__loot_weight') : t('activity_ui__loot_probability')}</span>
        <input inputMode="decimal" value={row.probability} readOnly={readOnly} onChange={(e) => onChange({ probability: e.target.value })} />
      </label>
      <label>
        <span>{isSetHeader ? t('activity_ui__loot_picks_min') : t('activity_ui__loot_min')}</span>
        <input inputMode="numeric" value={row.min} readOnly={readOnly} onChange={(e) => onChange({ min: e.target.value })} />
      </label>
      <label>
        <span>{isSetHeader ? t('activity_ui__loot_picks_max') : t('activity_ui__loot_max')}</span>
        <input inputMode="numeric" value={row.max} readOnly={readOnly} onChange={(e) => onChange({ max: e.target.value })} />
      </label>
      <label>
        <span>{t('activity_ui__loot_stock')}</span>
        <input inputMode="numeric" value={row.stock} readOnly={readOnly} placeholder="∞" onChange={(e) => onChange({ stock: e.target.value })} />
      </label>
      <label className="check">
        <input type="checkbox" checked={row.secret} disabled={readOnly} onChange={(e) => onChange({ secret: e.target.checked })} />
        <span>{t('activity_ui__loot_secret')}</span>
      </label>
      {!readOnly && (
        <button type="button" className="icon-button" aria-label={t('activity_ui__loot_remove')} title={t('activity_ui__loot_remove')} onClick={onRemove}>
          <CloseIcon size={16} />
        </button>
      )}
    </div>
  );
}

/** Éditeur complet de la table de butin d'un lieu (catégorie ou salon central) : objets seuls et
 *  ensembles, probabilités, quantités, stock, secret, recharge et délai. Validé ici puis par le
 *  serveur ; l'état affiché après l'enregistrement est celui relu sur le serveur. */
export function LootTableEditor({ t, placeId, placeName, hasCentral, onClose, onSaved }: Props) {
  const listId = useId();
  const [target, setTarget] = useState<LootTarget>('category');
  const [state, setState] = useState<LootState | null>(null);
  const [form, setForm] = useState<LootForm | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [saved, setSaved] = useState(false);

  useEffect(() => {
    let alive = true;
    setState(null);
    setForm(null);
    setError(null);
    setSaved(false);
    placeApi
      .loot(placeId, target)
      .then((s) => {
        if (!alive) return;
        setState(s);
        setForm(formFromState(s));
      })
      .catch((e) => alive && setError(t(errorKey(e))));
    return () => {
      alive = false;
    };
  }, [placeId, target, t]);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => e.key === 'Escape' && onClose();
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [onClose]);

  const known = useMemo(() => new Set(state?.items ?? []), [state]);
  const readOnly = !state?.can_edit;

  const describe = (err: LootError) => {
    const where =
      err.entry == null ? '' : err.item == null ? t('activity_ui__loot_at_entry', { entry: err.entry + 1 }) : t('activity_ui__loot_at_item', { entry: err.entry + 1, item: err.item + 1 });
    return where ? `${where} : ${t(err.key)}` : t(err.key);
  };

  const update = (f: (entries: EntryRow[]) => EntryRow[]) => {
    setSaved(false);
    setForm((prev) => (prev ? { ...prev, entries: f(prev.entries) } : prev));
  };
  const patchEntry = (e: number, patch: Partial<EntryRow>) => update((rows) => rows.map((r, i) => (i === e ? { ...r, ...patch } : r)));
  const patchSub = (e: number, s: number, patch: Partial<ItemRow>) =>
    update((rows) => rows.map((r, i) => (i === e ? { ...r, items: r.items.map((it, j) => (j === s ? { ...it, ...patch } : it)) } : r)));

  const save = async () => {
    if (!form) return;
    const checked = formToBody(form, known);
    if (!checked.ok) return setError(describe(checked.error));
    setBusy(true);
    setError(null);
    try {
      const fresh = await placeApi.saveLoot(placeId, target, checked.value);
      setState(fresh);
      setForm(formFromState(fresh));
      setSaved(true);
      onSaved();
    } catch (e) {
      setError(t(errorKey(e)));
    } finally {
      setBusy(false);
    }
  };

  const seconds = (v: string) => (/^\d+$/.test(v.trim()) && Number(v) > 0 ? formatDuration(Number(v)) : '');

  return (
    <div className="dialog-backdrop" onClick={onClose}>
      <div className="dialog loot-dialog" role="dialog" aria-modal="true" aria-labelledby={`${listId}-title`} onClick={(e) => e.stopPropagation()}>
        <header>
          <h2 id={`${listId}-title`}>{t('activity_ui__loot_title', { place: placeName })}</h2>
          <button type="button" className="icon-button" aria-label={t('activity_ui__close')} onClick={onClose}>
            <CloseIcon />
          </button>
        </header>
        {hasCentral && (
          <div className="loot-targets" role="tablist">
            {(['category', 'central'] as const).map((tg) => (
              <button key={tg} type="button" role="tab" aria-selected={target === tg} className={target === tg ? '' : 'secondary'} disabled={busy} onClick={() => setTarget(tg)}>
                {tg === 'category' ? t('activity_ui__loot_target_category') : t('activity_ui__loot_target_central')}
              </button>
            ))}
          </div>
        )}
        <p className="muted">{target === 'category' ? t('activity_ui__loot_target_category_help') : t('activity_ui__loot_target_central_help')}</p>
        {!form ? (
          <p className="muted">{error ?? t('activity_ui__loading')}</p>
        ) : (
          <>
            <datalist id={listId}>
              {state?.items.map((n) => <option key={n} value={n} />)}
            </datalist>
            <div className="loot-settings">
              <label>
                <span>{t('activity_ui__loot_rate_limit')}</span>
                <input inputMode="numeric" value={form.rateLimit} readOnly={readOnly} placeholder="0" onChange={(e) => { setSaved(false); setForm({ ...form, rateLimit: e.target.value }); }} />
                <span className="muted">{seconds(form.rateLimit)}</span>
              </label>
              <label>
                <span>{t('activity_ui__loot_delay')}</span>
                <input inputMode="numeric" value={form.delay} readOnly={readOnly} placeholder="0" onChange={(e) => { setSaved(false); setForm({ ...form, delay: e.target.value }); }} />
                <span className="muted">{seconds(form.delay)}</span>
              </label>
            </div>
            {form.entries.length === 0 && <p className="muted">{t('activity_ui__loot_empty')}</p>}
            <ol className="loot-entries">
              {form.entries.map((row, e) => (
                <li key={row.key} className={`loot-entry loot-${row.kind}`}>
                  <span className={`usage ${row.kind === 'set' ? 'usage-usable' : 'usage-consumable'}`}>
                    {row.kind === 'set' ? t('activity_ui__loot_kind_set') : t('activity_ui__loot_kind_item')}
                  </span>
                  <RowFields
                    t={t}
                    row={row}
                    readOnly={readOnly}
                    isSetHeader={row.kind === 'set'}
                    inSet={false}
                    listId={listId}
                    onChange={(p) => patchEntry(e, p)}
                    onRemove={() => update((rows) => rows.filter((_, i) => i !== e))}
                  />
                  {row.kind === 'set' && (
                    <div className="loot-set-items">
                      {row.items.map((sub, s) => (
                        <RowFields
                          key={sub.key}
                          t={t}
                          row={sub}
                          readOnly={readOnly}
                          isSetHeader={false}
                          inSet
                          listId={listId}
                          onChange={(p) => patchSub(e, s, p)}
                          onRemove={() => patchEntry(e, { items: row.items.filter((_, j) => j !== s) })}
                        />
                      ))}
                      {!readOnly && (
                        <button type="button" className="small secondary" onClick={() => patchEntry(e, { items: [...row.items, newItemRow()] })}>
                          {t('activity_ui__loot_add_set_item')}
                        </button>
                      )}
                    </div>
                  )}
                </li>
              ))}
            </ol>
            {!readOnly && (
              <div className="row">
                <button type="button" className="secondary" onClick={() => update((rows) => [...rows, newEntry('item')])}>{t('activity_ui__loot_add_item')}</button>
                <button type="button" className="secondary" onClick={() => update((rows) => [...rows, newEntry('set')])}>{t('activity_ui__loot_add_set')}</button>
              </div>
            )}
            <p className="muted">{t('activity_ui__loot_help')}</p>
            {error && <p className="travel-message" role="alert">{error}</p>}
            {saved && <p className="travel-message" role="status">{t('activity_ui__loot_saved')}</p>}
            <div className="row end">
              <button type="button" className="secondary" onClick={onClose}>{t('activity_ui__close')}</button>
              {!readOnly && <button type="button" disabled={busy} onClick={() => void save()}>{t('activity_ui__loot_save')}</button>}
            </div>
          </>
        )}
      </div>
    </div>
  );
}
