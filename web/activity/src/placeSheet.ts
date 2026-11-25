import type { LootEntry, LootItem, LootState, LootTableBody } from './api';

/**
 * Éditeur de table de butin : formulaire (champs en texte, pour pouvoir taper librement) et
 * validation, mêmes règles que le serveur (`src/loot_table/service.rs`). Le serveur revalide ;
 * ici on montre l'endroit fautif avant l'envoi.
 */

export const MAX_ENTRIES = 100;
export const MAX_SET_ITEMS = 50;
export const MAX_NAME_CHARS = 100;
export const MAX_QUANTITY = 1000;
export const MAX_PICKS = 100;
export const MAX_STOCK = 1_000_000;
export const MAX_SECONDS = 365 * 24 * 3600;

export interface ItemRow {
  key: number;
  name: string;
  probability: string;
  min: string;
  max: string;
  /** Vide : stock illimité. */
  stock: string;
  secret: boolean;
}

export interface EntryRow extends ItemRow {
  kind: 'item' | 'set';
  /** Objets d'un ensemble (vide pour un objet seul). */
  items: ItemRow[];
}

export interface LootForm {
  entries: EntryRow[];
  rateLimit: string;
  delay: string;
}

/** Erreur : clé Fluent (`activity__loot_*`, la même que le serveur) et position (à partir de 0). */
export interface LootError {
  key: string;
  entry: number | null;
  item: number | null;
}

let nextKey = 1;
const key = () => nextKey++;

const itemRow = (i: LootItem): ItemRow => ({
  key: key(),
  name: i.name,
  probability: String(i.probability),
  min: String(i.min),
  max: String(i.max),
  stock: i.stock == null ? '' : String(i.stock),
  secret: i.secret,
});

export function newItemRow(): ItemRow {
  return { key: key(), name: '', probability: '10', min: '1', max: '1', stock: '', secret: false };
}

export function newEntry(kind: 'item' | 'set'): EntryRow {
  return { ...newItemRow(), kind, items: kind === 'set' ? [newItemRow()] : [] };
}

export function formFromState(state: LootState): LootForm {
  return {
    entries: state.entries.map((e) => ({ ...itemRow(e), kind: e.kind, items: e.kind === 'set' ? e.items.map(itemRow) : [] })),
    rateLimit: state.rate_limit ? String(state.rate_limit) : '',
    delay: state.delay ? String(state.delay) : '',
  };
}

/** Entier positif écrit en chiffres seulement (`'1e3'`, `'2.5'`, `'-1'` refusés). */
function integer(s: string): number | null {
  const t = s.trim();
  if (!/^\d{1,10}$/.test(t)) return null;
  return Number(t);
}

function decimal(s: string): number | null {
  const t = s.trim().replace(',', '.');
  if (!/^\d+(\.\d+)?$/.test(t)) return null;
  return Number(t);
}

/** Nom lisible par la commande slash (`nom: prob, …`) : ni `:` ni retour à la ligne, pas de `-` en tête. */
export function writableName(name: string): boolean {
  // eslint-disable-next-line no-control-regex
  return !/[:\u0000-\u001f\u007f]/.test(name) && !name.startsWith('-');
}

type Checked<T> = { ok: true; value: T } | { ok: false; error: LootError };

function checkName(name: string, err: (key: string) => LootError): Checked<string> {
  const n = name.trim();
  if (!n) return { ok: false, error: err('activity__loot_empty_name') };
  if ([...n].length > MAX_NAME_CHARS) return { ok: false, error: err('activity__loot_name_too_long') };
  if (!writableName(n)) return { ok: false, error: err('activity__loot_name_not_writable') };
  return { ok: true, value: n };
}

function checkCommon(row: ItemRow, maxQuantity: number, err: (key: string) => LootError) {
  const p = decimal(row.probability);
  if (p == null || !(p > 0 && p <= 100)) return { ok: false as const, error: err('activity__loot_bad_probability') };
  const min = integer(row.min);
  const max = integer(row.max);
  if (min == null || max == null) return { ok: false as const, error: err('activity__loot_bad_quantity') };
  if (min > max) return { ok: false as const, error: err('activity__loot_min_above_max') };
  if (max === 0 || max > maxQuantity) return { ok: false as const, error: err('activity__loot_bad_quantity') };
  let stock: number | null = null;
  if (row.stock.trim()) {
    stock = integer(row.stock);
    if (stock == null || stock === 0 || stock > MAX_STOCK) return { ok: false as const, error: err('activity__loot_bad_stock') };
  }
  return { ok: true as const, value: { probability: p, min, max, stock, secret: row.secret } };
}

function checkItem(row: ItemRow, known: ReadonlySet<string>, err: (key: string) => LootError): Checked<LootItem> {
  const name = checkName(row.name, err);
  if (!name.ok) return name;
  if (!known.has(name.value)) return { ok: false, error: err('activity__loot_unknown_item') };
  const c = checkCommon(row, MAX_QUANTITY, err);
  if (!c.ok) return c;
  return { ok: true, value: { name: name.value, ...c.value } };
}

function seconds(s: string, err: LootError): Checked<number | null> {
  if (!s.trim()) return { ok: true, value: null };
  const v = integer(s);
  if (v == null || v > MAX_SECONDS) return { ok: false, error: err };
  return { ok: true, value: v === 0 ? null : v };
}

/** Formulaire → corps de `PUT /admin/places/{id}/loot`, ou la première erreur. */
export function formToBody(form: LootForm, known: ReadonlySet<string>): Checked<LootTableBody> {
  const at = (entry: number | null, item: number | null) => (k: string): LootError => ({ key: k, entry, item });
  if (form.entries.length > MAX_ENTRIES) return { ok: false, error: at(null, null)('activity__loot_too_many_entries') };
  const rate = seconds(form.rateLimit, at(null, null)('activity__loot_bad_seconds'));
  if (!rate.ok) return rate;
  const delay = seconds(form.delay, at(null, null)('activity__loot_bad_seconds'));
  if (!delay.ok) return delay;
  const entries: LootEntry[] = [];
  for (const [e, row] of form.entries.entries()) {
    if (row.kind === 'item') {
      const it = checkItem(row, known, at(e, null));
      if (!it.ok) return it;
      entries.push({ kind: 'item', ...it.value });
      continue;
    }
    const name = checkName(row.name, at(e, null));
    if (!name.ok) return name;
    const c = checkCommon(row, MAX_PICKS, at(e, null));
    if (!c.ok) return c;
    if (row.items.length === 0) return { ok: false, error: at(e, null)('activity__loot_empty_set') };
    if (row.items.length > MAX_SET_ITEMS) return { ok: false, error: at(e, null)('activity__loot_too_many_set_items') };
    const items: LootItem[] = [];
    for (const [i, sub] of row.items.entries()) {
      const it = checkItem(sub, known, at(e, i));
      if (!it.ok) return it;
      items.push(it.value);
    }
    entries.push({ kind: 'set', name: name.value, ...c.value, items });
  }
  return { ok: true, value: { entries, rate_limit: rate.value, delay: delay.value } };
}

/** Population en clair (`23 735`), dans la langue de l'interface. */
export function formatPopulation(n: number, locale: string): string {
  return new Intl.NumberFormat(locale).format(n);
}

/** Nombre de caractères tel que le serveur le compte (points de code, pas unités UTF-16). */
export function charCount(s: string): number {
  return [...s].length;
}
