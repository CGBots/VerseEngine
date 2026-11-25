import { useMemo, useState } from 'react';
import { adminApi, type AdminItem } from '../api';
import type { Translator } from '../i18n';
import { EffectList, UsageBadge, USAGE_KEY } from '../components/ItemCard';
import { filterByName, itemBody, itemDraft, itemDraftError, USAGES, type ItemDraft } from './adminModel';
import { AdminWindowFrame, DeleteButton, errorKey, useServerData } from './AdminWindow';

/**
 * Objets (mode admin, à la place d'Inventaire) : tous les objets de l'univers, filtrables, et
 * leur fiche éditable. Mêmes services que `/item create` et `/item delete` (post wiki compris).
 * Un modérateur consulte sans modifier (D3) : le serveur le refuse de toute façon.
 */
export function ItemEditor({ t, onClose }: { t: Translator; onClose: () => void }) {
  const { data, error, reload } = useServerData(adminApi.items);
  const [query, setQuery] = useState('');
  const [draft, setDraft] = useState<ItemDraft | null>(null);
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState<string | null>(null);
  const items = useMemo(() => filterByName(data?.items ?? [], query, (i) => [i.name, i.description]), [data, query]);
  const canEdit = !!data?.can_edit;

  const open = (item: AdminItem | null) => {
    setDraft(itemDraft(item));
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
      // Toujours relire : l'état du serveur fait foi, même après un échec.
      void reload();
    }
  };
  const save = (d: ItemDraft) =>
    run(async () => {
      const res = d.id ? await adminApi.updateItem(d.id, itemBody(d)) : await adminApi.createItem(itemBody(d));
      // Un objet créé devient l'objet édité (la suite modifie celui-ci).
      setDraft({ ...d, id: res.id, wiki: 'keep' });
      return t(res.result);
    }, false);
  const remove = (id: string) =>
    run(async () => {
      const res = await adminApi.deleteItem(id);
      return res.affected_recipes.length
        ? t('activity_ui__admin_item_deleted_recipes', { name: res.name, recipes: res.affected_recipes.join(', ') })
        : t('activity_ui__admin_item_deleted', { name: res.name });
    }, true);

  return (
    <AdminWindowFrame t={t} title={t('activity_ui__admin_items')} className="editor-window" onClose={onClose}>
      {error && <p className="travel-message" role="alert">{t(error)}</p>}
      {!data && !error && <p className="muted">{t('activity_ui__loading')}</p>}
      {data && !draft && (
        <>
          <div className="admin-toolbar">
            <input type="search" placeholder={t('activity_ui__admin_search_item')} aria-label={t('activity_ui__admin_search_item')} value={query} onChange={(e) => setQuery(e.target.value)} />
            {canEdit && <button type="button" className="small" onClick={() => open(null)}>{t('activity_ui__admin_new_item')}</button>}
          </div>
          {!canEdit && <p className="muted small-text">{t('activity_ui__admin_read_only')}</p>}
          {message && <p className="travel-message" role="status">{message}</p>}
          {data.items.length === 0 ? (
            <p className="muted">{t('activity_ui__admin_no_item')}</p>
          ) : (
            <ul className="admin-list">
              {items.map((i) => (
                <li key={i.id}>
                  <button type="button" className="admin-row" onClick={() => open(i)}>
                    <span className="name">{i.name}</span>
                    <UsageBadge usage={i.usage} t={t} />
                    {i.effects.length > 0 && <EffectList effects={i.effects} />}
                  </button>
                </li>
              ))}
            </ul>
          )}
        </>
      )}
      {data && draft && (
        <ItemForm
          t={t}
          draft={draft}
          stats={data.stats}
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
  draft: ItemDraft;
  stats: string[];
  canEdit: boolean;
  busy: boolean;
  message: string | null;
  onChange: (d: ItemDraft) => void;
  onBack: () => void;
  onSave: () => void;
  onDelete: () => void;
}

function ItemForm({ t, draft, stats, canEdit, busy, message, onChange, onBack, onSave, onDelete }: FormProps) {
  const set = <K extends keyof ItemDraft>(k: K, v: ItemDraft[K]) => onChange({ ...draft, [k]: v });
  const problem = itemDraftError(draft);
  const isNew = draft.id === null;
  return (
    <form className="admin-form" onSubmit={(e) => { e.preventDefault(); if (!problem) onSave(); }}>
      <div className="row">
        <button type="button" className="secondary small" onClick={onBack}>← {t('activity_ui__back')}</button>
        <h3 className="form-title">{isNew ? t('activity_ui__admin_new_item') : draft.name}</h3>
      </div>
      <fieldset disabled={!canEdit || busy}>
        {isNew && (
          <label>
            {t('activity_ui__admin_name')}
            <input required maxLength={100} value={draft.name} onChange={(e) => set('name', e.target.value)} />
            <span className="muted small-text">{t('activity_ui__admin_item_name_help')}</span>
          </label>
        )}
        <div className="form-grid">
          <label>
            {t('activity_ui__admin_usage')}
            <select value={draft.usage} onChange={(e) => set('usage', e.target.value as ItemDraft['usage'])}>
              {USAGES.map((u) => <option key={u} value={u}>{t(USAGE_KEY[u])}</option>)}
            </select>
          </label>
          <label>
            {t('activity_ui__admin_inventory_size')}
            <input type="number" inputMode="numeric" min={0} max={10000} value={draft.inventorySize} onChange={(e) => set('inventorySize', Number(e.target.value))} />
          </label>
        </div>
        <label>
          {t('activity_ui__description')}
          <textarea rows={3} maxLength={4000} value={draft.description} onChange={(e) => set('description', e.target.value)} />
        </label>
        <label>
          {t('activity_ui__secret')}
          <textarea rows={2} maxLength={1024} value={draft.secret} onChange={(e) => set('secret', e.target.value)} />
        </label>
        <label>
          {t('activity_ui__admin_effects')}
          <textarea className="mono" rows={3} maxLength={1024} spellCheck={false} placeholder="Force: +5 10m player" value={draft.effects} onChange={(e) => set('effects', e.target.value)} />
          <span className="muted small-text">{t('activity_ui__admin_effects_help', { stats: stats.join(', ') || '—' })}</span>
        </label>
        <label>
          {t('activity_ui__admin_image')}
          <input type="url" inputMode="url" placeholder="https://…" value={draft.image} onChange={(e) => set('image', e.target.value)} />
        </label>
        <label>
          {t('activity_ui__admin_wiki')}
          <select value={draft.wiki} onChange={(e) => set('wiki', e.target.value as ItemDraft['wiki'])}>
            {isNew ? (
              <>
                <option value="publish">{t('activity_ui__admin_wiki_publish')}</option>
                <option value="keep">{t('activity_ui__admin_wiki_none')}</option>
              </>
            ) : (
              <>
                <option value="keep">{t('activity_ui__admin_wiki_keep')}</option>
                <option value="publish">{t('activity_ui__admin_wiki_update')}</option>
                <option value="remove">{t('activity_ui__admin_wiki_remove')}</option>
              </>
            )}
          </select>
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
