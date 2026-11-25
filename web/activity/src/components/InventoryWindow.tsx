import { useState } from 'react';
import type { InventoryView } from '../api';
import type { Translator } from '../i18n';
import { CloseIcon } from './icons/Icons';
import { ItemCardView } from './ItemCard';

interface Props {
  t: Translator;
  inventory: InventoryView | null;
  /** Consommer un objet (effets appliqués tout de suite) ; renvoie le message à afficher. */
  onUse: (itemId: string) => Promise<string>;
  onClose: () => void;
}

/** Inventaire du joueur, consultable à tout moment, même en voyage, en craft ou en farm. Un clic
 *  sur un objet consommable propose de l'utiliser : ses effets s'appliquent aussitôt (la vitesse
 *  d'un groupe en route est recalculée par le serveur). */
export function InventoryWindow({ t, inventory, onUse, onClose }: Props) {
  const [selected, setSelected] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState<string | null>(null);
  const use = async (itemId: string) => {
    setBusy(true);
    setMessage(null);
    try {
      setMessage(await onUse(itemId));
    } finally {
      setBusy(false);
    }
  };
  return (
    <div className="inventory-window floating-window" role="dialog" aria-label={t('activity_ui__inventory')}>
      <header className="window-header">
        <h2>
          {t('activity_ui__inventory')}
          {inventory && <span className="muted"> · {inventory.items.length}</span>}
        </h2>
        <button type="button" className="icon-button" aria-label={t('activity_ui__close')} title={t('activity_ui__close')} onClick={onClose}>
          <CloseIcon size={18} />
        </button>
      </header>
      {message && <p className="action-message" role="status">{message}</p>}
      {!inventory ? (
        <p className="muted">{t('activity_ui__loading')}</p>
      ) : inventory.items.length === 0 ? (
        <p className="muted">{t('activity_ui__inventory_empty')}</p>
      ) : (
        <ul className="item-cards">
          {inventory.items.map((item) => {
            const key = item.inventory_id || item.item_id;
            return (
              <ItemCardView
                key={key}
                item={item}
                t={t}
                selected={selected === key}
                onSelect={() => setSelected(selected === key ? null : key)}
                action={
                  <div className="item-actions">
                    {item.usage === 'consumable' ? (
                      <button
                        type="button"
                        disabled={busy}
                        onClick={(e) => {
                          e.stopPropagation();
                          void use(item.item_id);
                        }}
                      >
                        {t('activity_ui__item_use')}
                      </button>
                    ) : (
                      <span className="muted">{t('activity_ui__item_not_usable')}</span>
                    )}
                  </div>
                }
              />
            );
          })}
        </ul>
      )}
    </div>
  );
}
