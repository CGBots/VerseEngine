import type React from 'react';
import type { ItemCard, ItemUsage } from '../api';
import type { Translator } from '../i18n';
import { formatModifier, isNegative } from './CharacterPanel';

// Clés écrites en entier : le test des traductions (`every_activity_key_is_translated`) les
// cherche telles quelles dans le code.
export const USAGE_KEY: Record<ItemUsage, string> = {
  consumable: 'activity_ui__usage_consumable',
  usable: 'activity_ui__usage_usable',
  placeable: 'activity_ui__usage_placeable',
  wearable: 'activity_ui__usage_wearable',
  none: 'activity_ui__usage_none',
};

export function UsageBadge({ usage, t }: { usage: ItemUsage; t: Translator }) {
  return <span className={`usage usage-${usage}`}>{t(USAGE_KEY[usage] ?? USAGE_KEY.none)}</span>;
}

/** Effets en pastilles (vert : bonus, rouge : malus), comme les statuts du personnage. */
export function EffectList({ effects }: { effects: ItemCard['effects'] }) {
  if (effects.length === 0) return null;
  return (
    <ul className="statuses">
      {effects.map((e, i) => (
        <li key={i} className={isNegative(e.modifier_type, e.value) ? 'negative' : undefined}>
          <strong>{e.stat}</strong> {formatModifier(e.modifier_type, e.value)}
        </li>
      ))}
    </ul>
  );
}

/** Carte d'un objet de l'inventaire : nom, type, quantité, description, effets, secret. */
export function ItemCardView({ item, t, selected, onSelect, action }: {
  item: ItemCard;
  t: Translator;
  /** Carte choisie (inventaire) : son bouton d'action s'affiche. */
  selected?: boolean;
  onSelect?: () => void;
  /** Bouton affiché sous la carte choisie (« Utiliser »). */
  action?: React.ReactNode;
}) {
  return (
    <li
      className={`item-card${onSelect ? ' selectable' : ''}${selected ? ' selected' : ''}`}
      onClick={onSelect}
      onKeyDown={onSelect ? (e) => { if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); onSelect(); } } : undefined}
      tabIndex={onSelect ? 0 : undefined}
      aria-selected={onSelect ? !!selected : undefined}
    >
      <header>
        <span className="name">{item.name}</span>
        <UsageBadge usage={item.usage} t={t} />
        <span className="qty" aria-label={t('activity_ui__quantity')}>×{item.quantity}</span>
      </header>
      {item.description && <p className="description">{item.description}</p>}
      <EffectList effects={item.effects} />
      {item.secret && (
        <p className="secret">
          <span className="muted">{t('activity_ui__secret')} · </span>
          {item.secret}
        </p>
      )}
      {selected && action}
    </li>
  );
}
