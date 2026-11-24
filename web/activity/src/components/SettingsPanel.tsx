import { useEffect } from 'react';
import type { Translator } from '../i18n';
import { CloseIcon } from './icons/Icons';

/** Brouillard de l'univers : réglé par son créateur seulement (I17). */
export interface FogSetting {
  enabled: boolean;
  busy: boolean;
  onToggle: (enabled: boolean) => void;
}

/** Paramètres. « Gérer mon abonnement » n'est qu'un emplacement (hors périmètre du core). */
export function SettingsPanel({ t, onClose, fog }: { t: Translator; onClose: () => void; fog?: FogSetting }) {
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => e.key === 'Escape' && onClose();
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [onClose]);
  return (
    <div className="dialog-backdrop" onClick={onClose}>
      <div className="dialog" role="dialog" aria-modal="true" aria-labelledby="settings-title" onClick={(e) => e.stopPropagation()}>
        <header>
          <h2 id="settings-title">{t('activity_ui__settings')}</h2>
          <button type="button" className="icon-button" aria-label={t('activity_ui__close')} onClick={onClose}>
            <CloseIcon />
          </button>
        </header>
        {fog && (
          <section>
            <h3>{t('activity_ui__universe_section')}</h3>
            <label>
              <input type="checkbox" checked={fog.enabled} disabled={fog.busy} onChange={(e) => fog.onToggle(e.target.checked)} />
              {t('activity_ui__fog_toggle')}
            </label>
            <p className="muted">{t('activity_ui__fog_help')}</p>
          </section>
        )}
        <section>
          <h3>{t('activity_ui__subscription_section')}</h3>
          <button type="button" disabled title={t('activity_ui__coming_soon')}>{t('activity_ui__subscription')}</button>
          <p className="muted">{t('activity_ui__coming_soon')}</p>
        </section>
      </div>
    </div>
  );
}
