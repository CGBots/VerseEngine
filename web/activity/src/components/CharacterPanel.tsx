import type { CharacterView } from '../api';
import type { Translator } from '../i18n';
import { Countdown } from './Countdown';

function fmt(v: unknown): string {
  if (typeof v === 'number') return Number.isInteger(v) ? String(v) : v.toFixed(2);
  return String(v);
}

interface Props {
  character: CharacterView;
  avatarUrl: string | null;
  t: Translator;
  onOpenInventory: () => void;
  onResync: () => void;
}

/** Menu vertical gauche : identité, description (lecture seule), statuts, vitesse, accès inventaire. */
export function CharacterPanel({ character, avatarUrl, t, onOpenInventory, onResync }: Props) {
  const image = character.image_url ?? avatarUrl;
  const travel = character.travel;
  const loc = character.location;
  return (
    <aside className="side-panel" aria-label={t('activity_ui__character')}>
      <header className="identity">
        {image ? <img className="portrait" src={image} alt="" /> : <div className="portrait placeholder" aria-hidden />}
        <h1>{character.name}</h1>
      </header>

      {loc && (
        <section>
          <h2>{t('activity_ui__location')}</h2>
          {loc.kind === 'road' ? (
            <p>{t('activity_ui__on_road', { road: loc.name ?? '?', destination: loc.destination_name ?? '?' })}</p>
          ) : (
            <p>{loc.name ?? loc.id}</p>
          )}
          {travel?.in_move && travel.eta_timestamp && (
            <p className="eta">
              {t('activity_ui__remaining', { time: '' })} <Countdown until={travel.eta_timestamp} onElapsed={onResync} />
            </p>
          )}
        </section>
      )}

      <section>
        <h2>{t('activity_ui__speed')}</h2>
        <p>{character.speed != null ? t('activity_ui__speed_value', { speed: fmt(character.speed) }) : '—'}</p>
      </section>

      <section>
        <h2>{t('activity_ui__description')}</h2>
        <p className="description">{character.description ?? '—'}</p>
      </section>

      <section>
        <h2>{t('activity_ui__statuses')}</h2>
        {character.statuses.length === 0 ? (
          <p className="muted">{t('activity_ui__no_status')}</p>
        ) : (
          <ul className="statuses">
            {character.statuses.map((s, i) => (
              <li key={`${s.source}-${i}`}>
                <strong>{s.stat}</strong> {s.modifier_type === 'multiplier' ? '×' : s.value != null && Number(s.value) >= 0 ? '+' : ''}
                {fmt(s.value)}
                {s.end_timestamp && (
                  <span className="muted"> · {t('activity_ui__expires', { time: '' })}<Countdown until={s.end_timestamp} onElapsed={onResync} /></span>
                )}
              </li>
            ))}
          </ul>
        )}
      </section>

      <nav className="actions">
        <button type="button" onClick={onOpenInventory}>{t('activity_ui__inventory')}</button>
      </nav>
    </aside>
  );
}
