import { useEffect, useState } from 'react';
import type { TravelState } from '../api';
import type { Translator } from '../i18n';
import { Countdown } from './Countdown';

interface Props {
  state: TravelState;
  selectedId: string | null;
  busy: boolean;
  message: string | null;
  speed: number | null;
  /** Écran compact : replié au départ, pour laisser voir la carte. */
  compact: boolean;
  t: Translator;
  onStart: (destinationId: string) => void;
  onStop: () => void;
  onLeave: () => void;
  onElapsed: () => void;
}

/** Panneau de voyage : destinations depuis la position, départ, arrêt, compte à rebours. */
export function TravelPanel({ state, selectedId, busy, message, speed, compact, t, onStart, onStop, onLeave, onElapsed }: Props) {
  const travel = state.travel;
  const moving = !!travel?.in_move;
  const leader = state.group_members[0];
  const [collapsed, setCollapsed] = useState(compact);
  useEffect(() => setCollapsed(compact), [compact]);
  // Un lieu choisi sur la carte, ou un message (départ, arrivée…) : le panneau s'ouvre.
  useEffect(() => {
    if (selectedId || message) setCollapsed(false);
  }, [selectedId, message]);
  const summary = moving && travel?.eta_timestamp ? null : t('activity_ui__destination_count', { count: state.destinations.length });
  return (
    <div className={`travel-panel${collapsed ? ' collapsed' : ''}`}>
      <button type="button" className="panel-toggle" aria-expanded={!collapsed} onClick={() => setCollapsed((c) => !c)}>
        <h2>{t('activity_ui__travel')}</h2>
        {collapsed && (
          <span className="muted">
            {summary ?? (
              <>
                {t('activity_ui__arrival_in')} <Countdown until={travel!.eta_timestamp!} onElapsed={onElapsed} />
              </>
            )}
          </span>
        )}
        <span className="chevron" aria-hidden>{collapsed ? '▴' : '▾'}</span>
      </button>
      {!collapsed && <TravelBody state={state} selectedId={selectedId} busy={busy} message={message} speed={speed} t={t} onStart={onStart} onStop={onStop} onLeave={onLeave} onElapsed={onElapsed} leader={leader} moving={moving} />}
    </div>
  );
}

function TravelBody({ state, selectedId, busy, message, speed, t, onStart, onStop, onLeave, onElapsed, leader, moving }: Omit<Props, 'compact'> & { leader: string | undefined; moving: boolean }) {
  const travel = state.travel;
  return (
    <>
      {moving && travel?.eta_timestamp ? (
        <p className="eta">
          {t('activity_ui__arrival_in')} <Countdown until={travel.eta_timestamp} onElapsed={onElapsed} />
          {travel.remaining_km != null && <span className="muted"> · {travel.remaining_km.toFixed(1)} km</span>}
        </p>
      ) : null}
      {state.destinations.length === 0 && !moving && <p className="muted">{t('activity_ui__no_destination')}</p>}
      {state.destinations.length > 0 && (
        <ul className="destinations">
          {state.destinations.map((d) => {
            const hours = speed && speed > 0 ? d.distance_km / speed : null;
            return (
              <li key={d.id} className={d.id === selectedId ? 'selected' : undefined}>
                <span>
                  <span className="name">{d.name}</span> <span className="muted">· {d.distance_km.toFixed(1)} km{hours != null ? ` · ~${hours.toFixed(1)} h` : ''}</span>
                </span>
                <button type="button" className="small" disabled={busy} onClick={() => onStart(d.id)}>{t('activity_ui__go')}</button>
              </li>
            );
          })}
        </ul>
      )}
      <div className="row">
        {moving && <button type="button" className="danger" disabled={busy} onClick={onStop}>{t('activity_ui__stop_travel')}</button>}
        {state.group_members.length > 1 && (
          <button type="button" className="secondary" disabled={busy} onClick={onLeave}>{t('activity_ui__leave_group')}</button>
        )}
      </div>
      {state.group_members.length > 1 && leader && <p className="muted">{t('activity_ui__group_size', { count: state.group_members.length })}</p>}
      {message && <p className="travel-message" role="status">{message}</p>}
    </>
  );
}
