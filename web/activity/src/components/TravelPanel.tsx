import type { TravelState } from '../api';
import type { Translator } from '../i18n';
import { Countdown } from './Countdown';

interface Props {
  state: TravelState;
  selectedId: string | null;
  busy: boolean;
  message: string | null;
  speed: number | null;
  t: Translator;
  onStart: (destinationId: string) => void;
  onStop: () => void;
  onLeave: () => void;
  onElapsed: () => void;
}

/** Panneau de voyage : destinations depuis la position, départ, arrêt, compte à rebours. */
export function TravelPanel({ state, selectedId, busy, message, speed, t, onStart, onStop, onLeave, onElapsed }: Props) {
  const travel = state.travel;
  const moving = !!travel?.in_move;
  const leader = state.group_members[0];
  return (
    <div className="travel-panel">
      {moving && travel?.eta_timestamp ? (
        <p className="eta">
          {t('activity_ui__arrival_in')} <Countdown until={travel.eta_timestamp} onElapsed={onElapsed} />
          {travel.remaining_km != null && <span className="muted"> · {travel.remaining_km.toFixed(1)} km</span>}
        </p>
      ) : null}
      {state.destinations.length > 0 && (
        <ul className="destinations">
          {state.destinations.map((d) => {
            const hours = speed && speed > 0 ? d.distance_km / speed : null;
            return (
              <li key={d.id} className={d.id === selectedId ? 'selected' : undefined}>
                <span>
                  {d.name} <span className="muted">· {d.distance_km.toFixed(1)} km{hours != null ? ` · ~${hours.toFixed(1)} h` : ''}</span>
                </span>
                <button type="button" disabled={busy} onClick={() => onStart(d.id)}>{t('activity_ui__go')}</button>
              </li>
            );
          })}
        </ul>
      )}
      <div className="row">
        {moving && <button type="button" disabled={busy} onClick={onStop}>{t('activity_ui__stop_travel')}</button>}
        {state.group_members.length > 1 && (
          <button type="button" disabled={busy} onClick={onLeave}>{t('activity_ui__leave_group')}</button>
        )}
      </div>
      {state.group_members.length > 1 && leader && <p className="muted">{t('activity_ui__group_size', { count: state.group_members.length })}</p>}
      {message && <p className="travel-message" role="status">{message}</p>}
    </div>
  );
}
