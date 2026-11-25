import { useMemo, useState } from 'react';
import { adminApi, type AdminRecipe } from '../api';
import type { Translator } from '../i18n';
import { formatDuration } from '../time';
import { bestUnit, filterByName, recipeBody, recipeDraft, recipeDraftError, toSeconds, type LineDraft, type RecipeDraft } from './adminModel';
import { AdminWindowFrame, DeleteButton, errorKey, useServerData } from './AdminWindow';

type Unit = 's' | 'm' | 'h' | 'd';
const UNIT_KEY: Record<Unit, string> = {
  s: 'activity_ui__admin_unit_s',
  m: 'activity_ui__admin_unit_m',
  h: 'activity_ui__admin_unit_h',
  d: 'activity_ui__admin_unit_d',
};

/**
 * Recettes (mode admin, à la place de Craft) : toutes les recettes de l'univers, création,
 * modification et suppression. Même service que `/recipe create` : la recette est enregistrée
 * dans sa syntaxe texte, et son post wiki publié, mis à jour ou retiré.
 */
export function RecipeEditor({ t, onClose }: { t: Translator; onClose: () => void }) {
  const { data, error, reload } = useServerData(adminApi.recipes);
  const [query, setQuery] = useState('');
  const [draft, setDraft] = useState<RecipeDraft | null>(null);
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState<string | null>(null);
  const recipes = useMemo(
    () => filterByName(data?.recipes ?? [], query, (r) => [r.name, ...r.ingredients.map((l) => l.name), ...r.results.map((l) => l.name)]),
    [data, query],
  );
  const canEdit = !!data?.can_edit;

  const open = (r: AdminRecipe | null) => {
    setDraft(recipeDraft(r));
    setMessage(null);
  };
  const run = async (action: () => Promise<string>, close: boolean) => {
    setBusy(true);
    setMessage(null);
    try {
      setMessage(await action());
      if (close) setDraft(null);
    } catch (e) {
      setMessage(t(errorKey(e)));
    } finally {
      setBusy(false);
      void reload();
    }
  };
  const save = (d: RecipeDraft) =>
    run(async () => {
      const res = d.id ? await adminApi.updateRecipe(d.id, recipeBody(d)) : await adminApi.createRecipe(recipeBody(d));
      setDraft({ ...d, id: res.id });
      return t(res.result);
    }, false);
  const remove = (id: string) =>
    run(async () => t('activity_ui__admin_recipe_deleted', { name: (await adminApi.deleteRecipe(id)).name }), true);

  return (
    <AdminWindowFrame t={t} title={t('activity_ui__admin_recipes')} className="editor-window" onClose={onClose}>
      {error && <p className="travel-message" role="alert">{t(error)}</p>}
      {!data && !error && <p className="muted">{t('activity_ui__loading')}</p>}
      {data && !draft && (
        <>
          <div className="admin-toolbar">
            <input type="search" placeholder={t('activity_ui__admin_search_recipe')} aria-label={t('activity_ui__admin_search_recipe')} value={query} onChange={(e) => setQuery(e.target.value)} />
            {canEdit && <button type="button" className="small" onClick={() => open(null)}>{t('activity_ui__admin_new_recipe')}</button>}
          </div>
          {!canEdit && <p className="muted small-text">{t('activity_ui__admin_read_only')}</p>}
          {message && <p className="travel-message" role="status">{message}</p>}
          {data.recipes.length === 0 ? (
            <p className="muted">{t('activity_ui__no_recipe')}</p>
          ) : (
            <ul className="admin-list">
              {recipes.map((r) => (
                <li key={r.id}>
                  <button type="button" className="admin-row" onClick={() => open(r)}>
                    <span className="name">{r.name}</span>
                    <span className="muted">
                      {r.ingredients.map((l) => `${l.quantity}× ${l.name}`).join(', ') || '—'} → {r.results.map((l) => `${l.quantity}× ${l.name}`).join(', ') || '—'}
                    </span>
                    <span className="muted">{r.delay > 0 ? formatDuration(r.delay) : t('activity_ui__instant')}</span>
                  </button>
                </li>
              ))}
            </ul>
          )}
        </>
      )}
      {data && draft && (
        <RecipeForm
          t={t}
          draft={draft}
          items={data.items}
          canEdit={canEdit}
          busy={busy}
          message={message}
          onChange={setDraft}
          onBack={() => { setDraft(null); setMessage(null); }}
          onSave={() => void save(draft)}
          onDelete={() => draft.id && void remove(draft.id)}
        />
      )}
    </AdminWindowFrame>
  );
}

interface FormProps {
  t: Translator;
  draft: RecipeDraft;
  items: { id: string; name: string }[];
  canEdit: boolean;
  busy: boolean;
  message: string | null;
  onChange: (d: RecipeDraft) => void;
  onBack: () => void;
  onSave: () => void;
  onDelete: () => void;
}

function ItemSelect({ t, items, value, onChange }: { t: Translator; items: FormProps['items']; value: string; onChange: (id: string) => void }) {
  return (
    <select aria-label={t('activity_ui__admin_item')} value={value} onChange={(e) => onChange(e.target.value)}>
      <option value="">{t('activity_ui__admin_pick_item')}</option>
      {items.map((i) => <option key={i.id} value={i.id}>{i.name}</option>)}
    </select>
  );
}

/** Lignes « objet × quantité » (ingrédients ou résultats). */
function Lines({ t, title, lines, items, onChange }: { t: Translator; title: string; lines: LineDraft[]; items: FormProps['items']; onChange: (l: LineDraft[]) => void }) {
  const set = (i: number, l: Partial<LineDraft>) => onChange(lines.map((x, j) => (j === i ? { ...x, ...l } : x)));
  return (
    <section className="admin-section">
      <h3>{title}</h3>
      <ul className="recipe-edit-lines">
        {lines.map((l, i) => (
          <li key={i}>
            <ItemSelect t={t} items={items} value={l.itemId} onChange={(itemId) => set(i, { itemId })} />
            <input type="number" inputMode="numeric" min={1} max={10000} aria-label={t('activity_ui__quantity')} value={l.quantity} onChange={(e) => set(i, { quantity: Number(e.target.value) })} />
            <button type="button" className="secondary small" aria-label={t('activity_ui__admin_remove_line')} title={t('activity_ui__admin_remove_line')} onClick={() => onChange(lines.filter((_, j) => j !== i))}>✕</button>
          </li>
        ))}
      </ul>
      <button type="button" className="secondary small" onClick={() => onChange([...lines, { itemId: '', quantity: 1 }])}>+ {t('activity_ui__admin_add_line')}</button>
    </section>
  );
}

function RecipeForm({ t, draft, items, canEdit, busy, message, onChange, onBack, onSave, onDelete }: FormProps) {
  const set = <K extends keyof RecipeDraft>(k: K, v: RecipeDraft[K]) => onChange({ ...draft, [k]: v });
  // Délai saisi dans l'unité la plus lisible ; le serveur reçoit des secondes.
  const [delay, setDelay] = useState(() => bestUnit(draft.delay));
  const setDelayPart = (value: number, unit: Unit) => {
    setDelay({ value, unit });
    set('delay', toSeconds(value, unit));
  };
  const problem = recipeDraftError(draft);
  const isNew = draft.id === null;
  return (
    <form className="admin-form" onSubmit={(e) => { e.preventDefault(); if (!problem) onSave(); }}>
      <div className="row">
        <button type="button" className="secondary small" onClick={onBack}>← {t('activity_ui__back')}</button>
        <h3 className="form-title">{isNew ? t('activity_ui__admin_new_recipe') : draft.name}</h3>
      </div>
      <fieldset disabled={!canEdit || busy}>
        <div className="form-grid">
          <label>
            {t('activity_ui__admin_name')}
            <input required maxLength={100} value={draft.name} onChange={(e) => set('name', e.target.value)} />
          </label>
          <label>
            {t('activity_ui__admin_delay')}
            <span className="delay-input">
              <input type="number" inputMode="decimal" min={0} value={delay.value} onChange={(e) => setDelayPart(Number(e.target.value), delay.unit)} />
              <select aria-label={t('activity_ui__admin_delay_unit')} value={delay.unit} onChange={(e) => setDelayPart(delay.value, e.target.value as Unit)}>
                {(Object.keys(UNIT_KEY) as Unit[]).map((u) => <option key={u} value={u}>{t(UNIT_KEY[u])}</option>)}
              </select>
            </span>
          </label>
        </div>
        <Lines t={t} title={t('activity_ui__admin_ingredients')} lines={draft.ingredients} items={items} onChange={(l) => set('ingredients', l)} />
        <Lines t={t} title={t('activity_ui__admin_results')} lines={draft.results} items={items} onChange={(l) => set('results', l)} />
        <section className="admin-section">
          <h3>{t('activity_ui__admin_tools')}</h3>
          <ul className="recipe-edit-lines">
            {draft.tools.map((id, i) => (
              <li key={i}>
                <ItemSelect t={t} items={items} value={id} onChange={(v) => set('tools', draft.tools.map((x, j) => (j === i ? v : x)))} />
                <button type="button" className="secondary small" aria-label={t('activity_ui__admin_remove_line')} title={t('activity_ui__admin_remove_line')} onClick={() => set('tools', draft.tools.filter((_, j) => j !== i))}>✕</button>
              </li>
            ))}
          </ul>
          <button type="button" className="secondary small" onClick={() => set('tools', [...draft.tools, ''])}>+ {t('activity_ui__admin_add_tool')}</button>
        </section>
        <label className="checkbox">
          <input type="checkbox" checked={draft.intoWiki} onChange={(e) => set('intoWiki', e.target.checked)} />
          {t('activity_ui__admin_recipe_wiki')}
        </label>
      </fieldset>
      {canEdit && problem && <p className="muted small-text" role="alert">{t(problem)}</p>}
      {message && <p className="travel-message" role="status">{message}</p>}
      {canEdit && (
        <div className="row admin-actions-row">
          <button type="submit" disabled={busy || !!problem}>{t(isNew ? 'activity_ui__admin_create' : 'activity_ui__admin_save')}</button>
          {!isNew && <DeleteButton t={t} disabled={busy} onConfirm={onDelete} />}
        </div>
      )}
    </form>
  );
}
