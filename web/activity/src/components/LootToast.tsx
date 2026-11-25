import { useEffect } from 'react';
import type { Translator } from '../i18n';
import { CloseIcon } from './icons/Icons';

/** Durée d'affichage de la notification de butin. */
export const LOOT_TOAST_MS = 8000;

export interface LootToastData {
  /** Identifie la notification (une nouvelle relance le minuteur). */
  id: string;
  items: { name: string; quantity: number }[];
}

/** Notification de fin de fouille : les objets obtenus, quelques secondes. */
export function LootToast({ toast, t, onClose }: { toast: LootToastData; t: Translator; onClose: () => void }) {
  useEffect(() => {
    const id = window.setTimeout(onClose, LOOT_TOAST_MS);
    return () => window.clearTimeout(id);
  }, [toast.id, onClose]);
  return (
    <div className="loot-toast" role="status" aria-live="polite">
      <header>
        <strong>{t('activity_ui__farm_done')}</strong>
        <button type="button" className="icon-button" aria-label={t('activity_ui__close')} onClick={onClose}>
          <CloseIcon size={16} />
        </button>
      </header>
      {toast.items.length === 0 ? (
        <p className="muted">{t('loot_table__no_loot_found')}</p>
      ) : (
        <ul>
          {toast.items.map((i) => (
            <li key={i.name}>
              <span className="qty">{i.quantity}×</span> {i.name}
            </li>
          ))}
        </ul>
      )}
      <div className="loot-toast-timer" style={{ animationDuration: `${LOOT_TOAST_MS}ms` }} key={toast.id} />
    </div>
  );
}
