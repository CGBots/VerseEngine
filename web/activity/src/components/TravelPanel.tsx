import { useEffect, useState } from 'react';
import type { TravelState } from '../api';
import type { Translator } from '../i18n';
import { Countdown } from './Countdown';
import { CloseIcon } from './icons/Icons';

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
  /** Rejoindre le groupe de ce chef (`/travel_join`). */
  onJoin: (leaderId: string) => void;
  /** Inviter ce joueur dans son groupe. */
  onInvite: (userId: string) => void;
  onAcceptInvite: (fromUserId: string) => void;
  onDeclineInvite: (fromUserId: string) => void;
  onElapsed: () => void;
  /** Fermeture de la fenêtre ; impossible pendant un déplacement (le voyage doit rester visible). */
  onClose: () => void;
}

/** Fenêtre de voyage, ouverte depuis le bouton Voyage du panneau : destinations depuis la
 *  position, départ, arrêt, compte à rebours. Repliable (écran compact : repliée au départ). */
export function TravelPanel({ state, selectedId, busy, message, speed, compact, t, onStart, onStop, onLeave, onJoin, onInvite, onAcceptInvite, onDeclineInvite, onElapsed, onClose }: Props) {
  const travel = state.travel;
  const moving = !!travel?.in_move;
  const leader = state.group_members[0];
  const [collapsed, setCollapsed] = useState(compact);
  useEffect(() => setCollapsed(compact), [compact]);
  // Un lieu choisi sur la carte, ou un message (départ, arrivée…) : le panneau s'ouvre.
  useEffect(() => {
    if (selectedId || message || state.invites?.length) setCollapsed(false);
  }, [selectedId, message, state.invites?.length]);
  const summary = moving && travel?.eta_timestamp ? null : t('activity_ui__destination_count', { count: state.destinations.length });
  return (
    <div className={`travel-panel floating-window${collapsed ? ' collapsed' : ''}`} role="dialog" aria-label={t('activity_ui__travel')}>
      <header className="window-header">
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
        <button
          type="button"
          className="icon-button"
          disabled={moving}
          title={moving ? t('activity_ui__travel_close_locked') : t('activity_ui__close')}
          aria-label={t('activity_ui__close')}
          onClick={onClose}
        >
          <CloseIcon size={18} />
        </button>
      </header>
      {!collapsed && (
        <>
          <TravelBody state={state} selectedId={selectedId} busy={busy} message={message} speed={speed} t={t} onStart={onStart} onStop={onStop} onLeave={onLeave} onElapsed={onElapsed} leader={leader} moving={moving} />
          <GroupSection state={state} busy={busy} t={t} onJoin={onJoin} onInvite={onInvite} onAccept={onAcceptInvite} onDecline={onDeclineInvite} />
        </>
      )}
    </div>
  );
}

type BodyProps = Omit<Props, 'compact' | 'onClose' | 'onJoin' | 'onInvite' | 'onAcceptInvite' | 'onDeclineInvite'> & { leader: string | undefined; moving: boolean };

/** Groupe de voyage : membres, invitations reçues, « Créer un groupe » (inviter les joueurs
 *  présents) et « Rejoindre un groupe » (groupes à la même position, comme `/travel_join`). */
function GroupSection({ state, busy, t, onJoin, onInvite, onAccept, onDecline }: {
  state: TravelState; busy: boolean; t: Translator;
  onJoin: (leaderId: string) => void; onInvite: (userId: string) => void; onAccept: (fromId: string) => void; onDecline: (fromId: string) => void;
}) {
  const [mode, setMode] = useState<null | 'create' | 'join'>(null);
  const nearby = state.nearby ?? [];
  const invites = state.invites ?? [];
  const names = state.group_names ?? [];
  const inGroup = state.group_members.length > 1;
  if (!state.location) return null;
  return (
    <section className="group-section" aria-label={t('activity_ui__group')}>
      <h3>{t('activity_ui__group')}</h3>
      <p className="muted">{inGroup ? names.join(', ') : t('activity_ui__group_alone')}</p>
      {inGroup && state.group_speed_kmh != null && (
        <p className="group-speed">
          {t('activity_ui__group_speed')} <strong>{state.group_speed_kmh.toFixed(1)} km/h</strong>
        </p>
      )}
      {invites.length > 0 && (
        <ul className="group-list invites" aria-label={t('activity_ui__group_invites')}>
          {invites.map((i) => (
            <li key={i.from_user_id}>
              <span>{t('activity_ui__group_invite_from', { name: i.from_name })}</span>
              <span className="row">
                <button type="button" className="small" disabled={busy} onClick={() => onAccept(i.from_user_id)}>{t('activity_ui__accept')}</button>
                <button type="button" className="small secondary" disabled={busy} onClick={() => onDecline(i.from_user_id)}>{t('activity_ui__decline')}</button>
              </span>
            </li>
          ))}
        </ul>
      )}
      <div className="row">
        <button type="button" className="secondary" aria-pressed={mode === 'create'} onClick={() => setMode(mode === 'create' ? null : 'create')}>
          {t(inGroup ? 'activity_ui__group_invite_more' : 'activity_ui__group_create')}
        </button>
        <button type="button" className="secondary" aria-pressed={mode === 'join'} onClick={() => setMode(mode === 'join' ? null : 'join')}>
          {t('activity_ui__group_join')}
        </button>
      </div>
      {mode && (
        <>
          <p className="muted">{t(mode === 'create' ? 'activity_ui__group_create_hint' : 'activity_ui__group_join_hint')}</p>
          {nearby.length === 0 ? (
            <p className="muted">{t('activity_ui__group_nobody_here')}</p>
          ) : mode === 'create' ? (
            <ul className="group-list">
              {nearby.flatMap((g) => g.members).map((m) => (
                <li key={m.user_id}>
                  <span className="name">{m.name}</span>
                  <button type="button" className="small" disabled={busy} onClick={() => onInvite(m.user_id)}>{t('activity_ui__group_invite')}</button>
                </li>
              ))}
            </ul>
          ) : (
            <ul className="group-list">
              {nearby.map((g) => (
                <li key={g.leader_id}>
                  <span>
                    <span className="name">{g.members.map((m) => m.name).join(', ')}</span>
                    {g.moving && <span className="muted"> · {t('activity_ui__group_moving')}</span>}
                  </span>
                  <button type="button" className="small" disabled={busy} onClick={() => onJoin(g.leader_id)}>{t('activity_ui__group_join_one')}</button>
                </li>
              ))}
            </ul>
          )}
        </>
      )}
    </section>
  );
}

function TravelBody({ state, selectedId, busy, message, speed, t, onStart, onStop, onLeave, onElapsed, leader, moving }: BodyProps) {
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
