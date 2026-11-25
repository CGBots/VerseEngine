import { useState } from 'react';
import type { RecipeBook, RecipeView } from '../api';
import type { Translator } from '../i18n';
import { clampQuantity } from '../occupation';
import { formatDuration } from '../time';
import { CloseIcon } from './icons/Icons';

interface Props {
  t: Translator;
  book: RecipeBook | null;
  /** Lancer une recette est permis (aucun voyage, craft ni farm en cours). */
  canStart: boolean;
  busy: boolean;
  message: string | null;
  onStart: (recipeId: string, quantity: number) => void;
  onClose: () => void;
}

function RecipeCard({ recipe, t, canStart, busy, onStart }: { recipe: RecipeView; t: Translator; canStart: boolean; busy: boolean; onStart: Props['onStart'] }) {
  const [wanted, setWanted] = useState(1);
  const craftable = recipe.max_craftable > 0;
  const quantity = clampQuantity(wanted, recipe.max_craftable);
  return (
    <li className={`recipe${craftable ? ' craftable' : ' unavailable'}`} aria-disabled={!craftable}>
      <header>
        <span className="name">{recipe.name}</span>
        <span className="muted">{recipe.delay > 0 ? formatDuration(recipe.delay) : t('activity_ui__instant')}</span>
      </header>
      <ul className="recipe-lines">
        {recipe.ingredients.map((i) => (
          <li key={i.item_id} className={i.owned >= i.quantity ? 'ok' : 'missing'}>
            {i.quantity}× {i.name} <span className="muted">({t('activity_ui__owned', { count: i.owned })})</span>
          </li>
        ))}
        {recipe.tools.map((tool, i) => (
          <li key={`tool-${i}`} className={tool.available ? 'ok' : 'missing'}>
            {t('activity_ui__tool', { name: tool.name })}
          </li>
        ))}
      </ul>
      {recipe.results.length > 0 && (
        <p className="recipe-results">→ {recipe.results.map((r) => `${r.quantity}× ${r.name}`).join(', ')}</p>
      )}
      {craftable && (
        <div className="recipe-craft">
          <label>
            {t('activity_ui__quantity')}
            <input
              type="number"
              inputMode="numeric"
              min={1}
              max={recipe.max_craftable}
              value={wanted}
              onChange={(e) => setWanted(Number(e.target.value))}
              onBlur={() => setWanted(quantity)}
            />
          </label>
          <span className="muted">{t('activity_ui__max_craftable', { max: recipe.max_craftable })}</span>
          <button type="button" className="small" disabled={!canStart || busy || quantity < 1} onClick={() => onStart(recipe.id, quantity)}>
            {t('activity_ui__craft_start')}
          </button>
        </div>
      )}
    </li>
  );
}

/** Fenêtre de craft : le livre de recettes. Les recettes réalisables viennent d'abord, avec le
 *  maximum faisable calculé par le serveur. L'inventaire a sa propre fenêtre (bouton
 *  Inventaire) ; chaque ingrédient indique déjà la quantité détenue. */
export function CraftWindow({ t, book, canStart, busy, message, onStart, onClose }: Props) {
  const craftable = book?.recipes.filter((r) => r.max_craftable > 0) ?? [];
  const unavailable = book?.recipes.filter((r) => r.max_craftable === 0) ?? [];
  return (
    <div className="craft-window floating-window" role="dialog" aria-label={t('activity_ui__craft')}>
      <header className="window-header">
        <h2>{t('activity_ui__craft')}</h2>
        <button type="button" className="icon-button" aria-label={t('activity_ui__close')} title={t('activity_ui__close')} onClick={onClose}>
          <CloseIcon size={18} />
        </button>
      </header>
      {message && <p className="travel-message" role="status">{message}</p>}

      <details className="fold" open>
        <summary>{t('activity_ui__recipe_book')}</summary>
        {!book ? (
          <p className="muted">{t('activity_ui__loading')}</p>
        ) : book.recipes.length === 0 ? (
          <p className="muted">{t('activity_ui__no_recipe')}</p>
        ) : (
          <>
            {!canStart && craftable.length > 0 && <p className="muted small-text">{t('activity_ui__craft_blocked')}</p>}
            <ul className="recipes">
              {craftable.map((r) => <RecipeCard key={r.id} recipe={r} t={t} canStart={canStart} busy={busy} onStart={onStart} />)}
            </ul>
            {unavailable.length > 0 && (
              <>
                <h3 className="recipes-divider">{t('activity_ui__recipes_unavailable')}</h3>
                <ul className="recipes">
                  {unavailable.map((r) => <RecipeCard key={r.id} recipe={r} t={t} canStart={false} busy={busy} onStart={onStart} />)}
                </ul>
              </>
            )}
          </>
        )}
      </details>
    </div>
  );
}
