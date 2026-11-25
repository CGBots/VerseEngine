import { useMemo, useState } from 'react';
import { adminApi, type TeleportResponse } from '../api';
import type { Translator } from '../i18n';
import { filterByName, filterPlayers, selectedIds, toggle, toggleAll } from './adminModel';
import { AdminWindowFrame, errorKey, useServerData } from './AdminWindow';

/**
 * Téléportation (mode admin, à la place de Voyage) : un ou plusieurs joueurs de l'univers vers un
 * lieu. Même service que `/teleport player` : un joueur en voyage quitte son groupe, les rôles de
 * lieu et l'accès au salon de la route sont mis à jour, l'arrivée est annoncée.
 */
export function TeleportWindow({ t, onClose }: { t: Translator; onClose: () => void }) {
  const { data, error, reload } = useServerData(adminApi.players);
  const [query, setQuery] = useState('');
  const [placeQuery, setPlaceQuery] = useState('');
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [destination, setDestination] = useState('');
  const [busy, setBusy] = useState(false);
  const [outcome, setOutcome] = useState<TeleportResponse | null>(null);
  const [message, setMessage] = useState<string | null>(null);

  const players = data?.players ?? [];
  const visible = useMemo(() => filterPlayers(players, query), [players, query]);
  // Le lieu choisi reste dans la liste même s'il ne correspond plus à la recherche.
  const places = useMemo(() => {
    const all = data?.places ?? [];
    const found = filterByName(all, placeQuery, (p) => [p.name]);
    const current = all.find((p) => p.id === destination);
    return current && !found.includes(current) ? [current, ...found] : found;
  }, [data, placeQuery, destination]);
  const chosen = selectedIds(players, selected);
  const names = new Map(players.map((p) => [p.user_id, p.name]));
  const allVisible = visible.length > 0 && visible.every((p) => selected.has(p.user_id));

  const go = async () => {
    setBusy(true);
    setMessage(null);
    setOutcome(null);
    try {
      const res = await adminApi.teleport(chosen, destination);
      setOutcome(res);
      // Les joueurs téléportés quittent la sélection ; les échecs y restent, pour réessayer.
      setSelected(new Set(res.results.filter((r) => !r.ok).map((r) => r.user_id)));
    } catch (e) {
      setMessage(t(errorKey(e)));
    } finally {
      setBusy(false);
      void reload();
    }
  };

  return (
    <AdminWindowFrame t={t} title={t('activity_ui__admin_teleport')} className="teleport-window" onClose={onClose}>
      {error && <p className="travel-message" role="alert">{t(error)}</p>}
      {!data && !error && <p className="muted">{t('activity_ui__loading')}</p>}
      {data && (
        <>
          <section className="admin-section">
            <div className="admin-toolbar">
              <input type="search" placeholder={t('activity_ui__admin_search_player')} aria-label={t('activity_ui__admin_search_player')} value={query} onChange={(e) => setQuery(e.target.value)} />
              <button type="button" className="secondary small" disabled={visible.length === 0} onClick={() => setSelected((s) => toggleAll(s, visible))}>
                {t(allVisible ? 'activity_ui__admin_select_none' : 'activity_ui__admin_select_all')}
              </button>
            </div>
            {players.length === 0 ? (
              <p className="muted">{t('activity_ui__admin_no_player')}</p>
            ) : (
              <ul className="admin-list player-list">
                {visible.map((p) => (
                  <li key={p.user_id}>
                    <label>
                      <input type="checkbox" checked={selected.has(p.user_id)} onChange={() => setSelected((s) => toggle(s, p.user_id))} />
                      <span className="name">{p.name}</span>
                      <span className="muted">
                        {p.location
                          ? p.location.kind === 'road'
                            ? t('activity_ui__admin_on_road', { road: p.location.name ?? '?' })
                            : p.location.name ?? '?'
                          : t('activity_ui__admin_nowhere')}
                        {p.group_size > 1 ? ` · ${t('activity_ui__group_size', { count: p.group_size })}` : ''}
                      </span>
                      {p.moving && <span className="usage usage-usable">{t('activity_ui__travel_in_progress')}</span>}
                    </label>
                  </li>
                ))}
              </ul>
            )}
          </section>

          <section className="admin-section">
            <h3>{t('activity_ui__admin_destination')}</h3>
            <div className="admin-toolbar">
              <input type="search" placeholder={t('activity_ui__admin_search_place')} aria-label={t('activity_ui__admin_search_place')} value={placeQuery} onChange={(e) => setPlaceQuery(e.target.value)} />
              <select aria-label={t('activity_ui__admin_destination')} value={destination} onChange={(e) => setDestination(e.target.value)}>
                <option value="">{t('activity_ui__admin_pick_place')}</option>
                {places.map((p) => <option key={p.id} value={p.id}>{p.name}</option>)}
              </select>
            </div>
          </section>

          <div className="row admin-actions-row">
            <button type="button" disabled={busy || chosen.length === 0 || !destination} onClick={() => void go()}>
              {t('activity_ui__admin_teleport_go', { count: chosen.length })}
            </button>
          </div>
          {message && <p className="travel-message" role="alert">{message}</p>}
          {outcome && (
            <ul className="admin-results" role="status">
              {outcome.results.map((r) => (
                <li key={r.user_id} className={r.ok ? 'ok' : 'missing'}>
                  <strong>{names.get(r.user_id) ?? r.user_id}</strong> — {r.ok ? t('activity_ui__admin_teleported', { place: outcome.place }) : t(r.key)}
                </li>
              ))}
            </ul>
          )}
        </>
      )}
    </AdminWindowFrame>
  );
}
