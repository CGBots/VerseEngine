import { useEffect } from 'react';
import type { Translator } from '../i18n';
import { CloseIcon } from './icons/Icons';

/** Paramètres. « Gérer mon abonnement » n'est qu'un emplacement (hors périmètre du core). */
export function SettingsPanel({ t, onClose }: { t: Translator; onClose: () => void }) {
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
        <section>
          <h3>{t('activity_ui__subscription_section')}</h3>
          <button type="button" disabled title={t('activity_ui__coming_soon')}>{t('activity_ui__subscription')}</button>
          <p className="muted">{t('activity_ui__coming_soon')}</p>
        </section>
      </div>
    </div>
  );
}
