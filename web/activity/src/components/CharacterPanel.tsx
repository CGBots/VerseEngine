import type { ReactNode } from 'react';
import type { CharacterView } from '../api';
import type { Translator } from '../i18n';
import { Countdown } from './Countdown';

function fmt(v: unknown): string {
  if (typeof v === 'number') return Number.isInteger(v) ? String(v) : v.toFixed(2);
  return String(v);
}

/** Valeur d'un effet telle qu'affichée : `×1.5` pour un multiplicateur, `+2` / `-1` sinon. */
export function formatModifier(modifierType: string, value: unknown): string {
  if (modifierType === 'multiplier') return `×${fmt(value)}`;
  const sign = value != null && Number(value) >= 0 ? '+' : '';
  return `${sign}${fmt(value)}`;
}

interface Props {
  character: CharacterView;
  avatarUrl: string | null;
  t: Translator;
  /** Boutons d'action (farm, craft, voyage), en bas du panneau. */
  actions?: ReactNode;
  onResync: () => void;
}

/** Un effet est négatif s'il réduit la statistique (multiplicateur < 1, ajout < 0). */
export function isNegative(modifierType: string, value: unknown): boolean {
  const v = Number(value);
  if (!Number.isFinite(v)) return false;
  return modifierType === 'multiplier' ? v < 1 : v < 0;
}

/** Menu vertical gauche, à la manière d'un profil Discord : identité, position, vitesse,
 *  description (lecture seule), effets actifs, puis les actions (farm, craft, voyage). */
export function CharacterPanel({ character, avatarUrl, t, actions, onResync }: Props) {
  const image = character.image_url ?? avatarUrl;
  const travel = character.travel;
  const loc = character.location;
  return (
    <aside className="side-panel" aria-label={t('activity_ui__character')}>
      <div className="profile">
        <div className="profile-banner" aria-hidden />
        <header className="identity">
          {image ? <img className="portrait" src={image} alt="" /> : <div className="portrait placeholder" aria-hidden />}
          <h1>{character.name}</h1>
        </header>
        <div className="profile-body">

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
              <li key={`${s.source}-${i}`} className={isNegative(s.modifier_type, s.value) ? 'negative' : undefined}>
                <strong>{s.stat}</strong> {formatModifier(s.modifier_type, s.value)}
                {s.end_timestamp && (
                  <span className="muted"> · {t('activity_ui__expires', { time: '' })}<Countdown until={s.end_timestamp} onElapsed={onResync} /></span>
                )}
              </li>
            ))}
          </ul>
        )}
      </section>
        </div>
      </div>

      {actions}
    </aside>
  );
}
