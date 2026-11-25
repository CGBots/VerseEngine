import { useCallback, useEffect, useState, type ReactNode } from 'react';
import { ApiError } from '../api';
import type { Translator } from '../i18n';
import { CloseIcon } from '../components/icons/Icons';

export const errorKey = (e: unknown) => (e instanceof ApiError ? e.key : 'activity__internal_error');

/** Fenêtre du mode admin, ouverte depuis la barre d'actions (même cadre que Voyage et Craft). */
export function AdminWindowFrame({ t, title, className, onClose, children }: { t: Translator; title: string; className?: string; onClose: () => void; children: ReactNode }) {
  return (
    <div className={`admin-window floating-window${className ? ` ${className}` : ''}`} role="dialog" aria-label={title}>
      <header className="window-header">
        <h2>{title}</h2>
        <button type="button" className="icon-button" aria-label={t('activity_ui__close')} title={t('activity_ui__close')} onClick={onClose}>
          <CloseIcon size={18} />
        </button>
      </header>
      {children}
    </div>
  );
}

/**
 * Données d'une fenêtre admin, relues depuis le serveur : au chargement et après chaque écriture
 * (`reload`). `error` : clé Fluent du dernier échec de lecture.
 */
export function useServerData<T>(load: () => Promise<T>): { data: T | null; error: string | null; reload: () => Promise<void> } {
  const [data, setData] = useState<T | null>(null);
  const [error, setError] = useState<string | null>(null);
  const reload = useCallback(async () => {
    try {
      setData(await load());
      setError(null);
    } catch (e) {
      setError(errorKey(e));
    }
  }, [load]);
  useEffect(() => {
    void reload();
  }, [reload]);
  return { data, error, reload };
}

/** Bouton de suppression en deux temps : le premier clic demande confirmation (pas de
 *  `window.confirm`, que l'iframe de Discord peut bloquer). */
export function DeleteButton({ t, disabled, onConfirm }: { t: Translator; disabled: boolean; onConfirm: () => void }) {
  const [armed, setArmed] = useState(false);
  useEffect(() => {
    if (!armed) return;
    const id = window.setTimeout(() => setArmed(false), 5000);
    return () => window.clearTimeout(id);
  }, [armed]);
  return (
    <button
      type="button"
      className="danger"
      disabled={disabled}
      onClick={() => {
        if (armed) {
          setArmed(false);
          onConfirm();
        } else setArmed(true);
      }}
    >
      {t(armed ? 'activity_ui__admin_confirm_delete' : 'activity_ui__admin_delete')}
    </button>
  );
}
