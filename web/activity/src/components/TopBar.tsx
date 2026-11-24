import { useEffect, useState, type ReactElement } from 'react';
import { clock } from '../api';
import type { Translator } from '../i18n';
import { formatClock, timeOfDay, type Phase, type UniverseClockParams } from '../universeClock';
import { GearIcon, MoonIcon, ShieldIcon, SunIcon, SunriseIcon, SunsetIcon } from './icons/Icons';

const PHASE_ICON: Record<Phase, () => ReactElement> = {
  midnight: () => <MoonIcon size={18} />,
  sunrise: () => <SunriseIcon size={18} />,
  noon: () => <SunIcon size={18} />,
  sunset: () => <SunsetIcon size={18} />,
};

const PHASE_LABEL: Record<Phase, string> = {
  midnight: 'activity_ui__phase_midnight',
  sunrise: 'activity_ui__phase_sunrise',
  noon: 'activity_ui__phase_noon',
  sunset: 'activity_ui__phase_sunset',
};

/** Heure de l'univers, recalculée chaque seconde à partir de l'heure du serveur. */
function UniverseClock({ params, t }: { params: UniverseClockParams | null; t: Translator }) {
  const [now, setNow] = useState(() => clock.now());
  useEffect(() => {
    const id = window.setInterval(() => setNow(clock.now()), 1000);
    return () => window.clearInterval(id);
  }, []);
  if (!params) return <span className="universe-clock muted" title={t('activity_ui__universe_time')}>{t('activity_ui__time_stopped')}</span>;
  const { hours, minutes, phase } = timeOfDay(now, params);
  const Icon = PHASE_ICON[phase];
  return (
    <span className="universe-clock" title={`${t('activity_ui__universe_time')} · ${t(PHASE_LABEL[phase])}`}>
      <Icon />
      <span className="time">{formatClock(hours, minutes)}</span>
    </span>
  );
}

interface Props {
  /** Bouton (photo du personnage) qui ouvre ou ferme la fiche, sur tous les écrans. */
  characterButton?: { picture: string | null; open: boolean; onToggle: () => void };
  universeName: string;
  clockParams: UniverseClockParams | null;
  adminAvailable: boolean;
  adminMode: boolean;
  onToggleAdmin: () => void;
  onOpenSettings: () => void;
  t: Translator;
}

export function TopBar({ characterButton, universeName, clockParams, adminAvailable, adminMode, onToggleAdmin, onOpenSettings, t }: Props) {
  return (
    <header className="toolbar">
      {characterButton && (
        <button
          type="button"
          className="character-button"
          aria-expanded={characterButton.open}
          aria-controls="character-panel"
          aria-label={t('activity_ui__character')}
          title={t('activity_ui__character')}
          onClick={characterButton.onToggle}
        >
          {characterButton.picture ? <img src={characterButton.picture} alt="" /> : <span aria-hidden>?</span>}
        </button>
      )}
      <span className="universe-name">{universeName}</span>
      {adminMode && <span className="admin-badge">{t('activity_ui__admin_mode')}</span>}
      <div className="toolbar-right">
        <UniverseClock params={clockParams} t={t} />
        {adminAvailable && (
          <button
            type="button"
            className={`icon-button${adminMode ? ' active' : ''}`}
            aria-pressed={adminMode}
            aria-label={t('activity_ui__admin_mode')}
            title={t(adminMode ? 'activity_ui__admin_mode_leave' : 'activity_ui__admin_mode_enter')}
            onClick={onToggleAdmin}
          >
            <ShieldIcon />
          </button>
        )}
        <button type="button" className="icon-button" aria-label={t('activity_ui__settings')} title={t('activity_ui__settings')} onClick={onOpenSettings}>
          <GearIcon />
        </button>
      </div>
    </header>
  );
}
