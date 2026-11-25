import type { AdminItem, AdminPlayer, AdminRecipe, ItemBody, ItemUsage, RecipeBody, WikiAction } from '../api';

/** Texte comparable : minuscules, sans accents (« Éloïse » se trouve en tapant « elo »). */
export function fold(s: string): string {
  return s.normalize('NFD').replace(/\p{Diacritic}/gu, '').toLowerCase().trim();
}

/** Éléments dont le nom (ou un des textes donnés) contient la recherche. */
export function filterByName<T>(list: T[], query: string, texts: (x: T) => (string | null | undefined)[]): T[] {
  const q = fold(query);
  if (!q) return list;
  return list.filter((x) => texts(x).some((t) => t != null && fold(t).includes(q)));
}

/** Joueurs trouvés par leur nom ou par le nom de leur position. */
export function filterPlayers(players: AdminPlayer[], query: string): AdminPlayer[] {
  return filterByName(players, query, (p) => [p.name, p.location?.name]);
}

/** Coche ou décoche un joueur (nouvel ensemble : état React immuable). */
export function toggle(selected: ReadonlySet<string>, id: string): Set<string> {
  const next = new Set(selected);
  if (!next.delete(id)) next.add(id);
  return next;
}

/** « Tout cocher » sur la liste filtrée : coche tout, ou décoche tout si tout l'était déjà. */
export function toggleAll(selected: ReadonlySet<string>, visible: AdminPlayer[]): Set<string> {
  const next = new Set(selected);
  const all = visible.length > 0 && visible.every((p) => next.has(p.user_id));
  for (const p of visible) {
    if (all) next.delete(p.user_id);
    else next.add(p.user_id);
  }
  return next;
}

/** Joueurs réellement cochés, dans l'ordre de la liste (une sélection peut viser un joueur parti). */
export function selectedIds(players: AdminPlayer[], selected: ReadonlySet<string>): string[] {
  return players.filter((p) => selected.has(p.user_id)).map((p) => p.user_id);
}

// ---------------------------------------------------------------------------
// Objets
// ---------------------------------------------------------------------------

export const USAGES: ItemUsage[] = ['consumable', 'usable', 'placeable', 'wearable', 'none'];

/** Même règle que `/item create` (`VALID_NAME_RE`) : le serveur la revérifie. */
export const ITEM_NAME_RE = /^[A-Za-z0-9_ -]+$/;

export interface ItemDraft {
  /** `null` : nouvel objet. */
  id: string | null;
  name: string;
  usage: ItemUsage;
  description: string;
  secret: string;
  image: string;
  inventorySize: number;
  effects: string;
  wiki: WikiAction;
}

export function itemDraft(item: AdminItem | null): ItemDraft {
  if (!item) return { id: null, name: '', usage: 'none', description: '', secret: '', image: '', inventorySize: 0, effects: '', wiki: 'publish' };
  return {
    id: item.id,
    name: item.name,
    usage: item.usage,
    description: item.description ?? '',
    secret: item.secret ?? '',
    image: item.image ?? '',
    inventorySize: item.inventory_size,
    effects: item.effects_text,
    // L'objet ne retient pas ses posts wiki : par défaut, on n'y touche pas.
    wiki: 'keep',
  };
}

/** Clé Fluent du premier problème du brouillon, `null` s'il peut être envoyé. */
export function itemDraftError(d: ItemDraft): string | null {
  if (d.id === null && (!ITEM_NAME_RE.test(d.name) || d.name.length > 100)) return 'create_item__invalid_name';
  if (!Number.isInteger(d.inventorySize) || d.inventorySize < 0 || d.inventorySize > 10000) return 'activity_ui__admin_invalid_number';
  if (d.image.trim() && !d.image.trim().startsWith('https://')) return 'activity__invalid_image_url';
  return null;
}

export function itemBody(d: ItemDraft): ItemBody {
  const body: ItemBody = {
    usage: d.usage,
    description: d.description.trim(),
    secret: d.secret.trim(),
    image: d.image.trim(),
    inventory_size: d.inventorySize,
    effects: d.effects.trim(),
    wiki: d.wiki,
  };
  if (d.id === null) body.name = d.name.trim();
  return body;
}

// ---------------------------------------------------------------------------
// Recettes
// ---------------------------------------------------------------------------

export interface LineDraft {
  itemId: string;
  quantity: number;
}

export interface RecipeDraft {
  id: string | null;
  name: string;
  delay: number;
  ingredients: LineDraft[];
  results: LineDraft[];
  tools: string[];
  intoWiki: boolean;
}

export function recipeDraft(recipe: AdminRecipe | null): RecipeDraft {
  if (!recipe) return { id: null, name: '', delay: 0, ingredients: [], results: [], tools: [], intoWiki: false };
  const lines = (v: AdminRecipe['ingredients']) => v.map((l) => ({ itemId: l.item_id, quantity: l.quantity }));
  return {
    id: recipe.id,
    name: recipe.name,
    delay: recipe.delay,
    ingredients: lines(recipe.ingredients),
    results: lines(recipe.results),
    tools: recipe.tools.map((t) => t.item_id),
    intoWiki: recipe.in_wiki,
  };
}

/** Clé Fluent du premier problème de la recette, `null` si elle peut être envoyée. */
export function recipeDraftError(d: RecipeDraft): string | null {
  const name = d.name.trim();
  if (!name || name.length > 100) return 'activity__admin_invalid_recipe_name';
  if (!Number.isInteger(d.delay) || d.delay < 0 || d.delay > 30 * 24 * 3600) return 'activity_ui__admin_invalid_number';
  const lines = [...d.ingredients, ...d.results];
  if (lines.length === 0) return 'recipe__empty_recipe';
  if (lines.some((l) => !l.itemId) || d.tools.some((t) => !t)) return 'activity_ui__admin_pick_item';
  if (lines.some((l) => !Number.isInteger(l.quantity) || l.quantity < 1 || l.quantity > 10000)) return 'recipe__invalid_quantity';
  if (lines.length + d.tools.length > 50) return 'activity_ui__admin_too_many_lines';
  return null;
}

export function recipeBody(d: RecipeDraft): RecipeBody {
  const lines = (v: LineDraft[]) => v.map((l) => ({ item_id: l.itemId, quantity: l.quantity }));
  return { name: d.name.trim(), delay: d.delay, ingredients: lines(d.ingredients), results: lines(d.results), tools: d.tools, into_wiki: d.intoWiki };
}

/** Durée saisie en secondes, minutes, heures ou jours → secondes (entier ≥ 0, `NaN` sinon). */
export function toSeconds(value: number, unit: 's' | 'm' | 'h' | 'd'): number {
  const factor = { s: 1, m: 60, h: 3600, d: 86400 }[unit];
  return Number.isFinite(value) && value >= 0 ? Math.round(value * factor) : NaN;
}

/** Plus grande unité qui divise exactement la durée (affichage d'une recette existante). */
export function bestUnit(seconds: number): { value: number; unit: 's' | 'm' | 'h' | 'd' } {
  for (const [unit, f] of [['d', 86400], ['h', 3600], ['m', 60]] as const) {
    if (seconds > 0 && seconds % f === 0) return { value: seconds / f, unit };
  }
  return { value: seconds, unit: 's' };
}
