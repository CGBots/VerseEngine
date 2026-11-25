import { describe, expect, it } from 'vitest';
import type { AdminItem, AdminPlayer, AdminRecipe } from '../api';
import {
  bestUnit, filterPlayers, fold, itemBody, itemDraft, itemDraftError, recipeBody, recipeDraft, recipeDraftError, selectedIds, toggle, toggleAll, toSeconds,
} from './adminModel';

const player = (id: string, name: string, place: string | null = null): AdminPlayer => ({
  user_id: id,
  name,
  location: place ? { kind: 'place', id: `p${id}`, name: place } : null,
  moving: false,
  group_size: 1,
});

describe('filtre des joueurs', () => {
  const players = [player('1', 'Éloïse', 'Port Royal'), player('2', 'Bran', 'Forêt'), player('3', 'Ana')];
  it('ignore la casse et les accents, et cherche aussi la position', () => {
    expect(fold('  ÉLOÏSE ')).toBe('eloise');
    expect(filterPlayers(players, 'elo').map((p) => p.name)).toEqual(['Éloïse']);
    expect(filterPlayers(players, 'foret').map((p) => p.name)).toEqual(['Bran']);
    expect(filterPlayers(players, '')).toHaveLength(3);
  });
});

describe('sélection des joueurs', () => {
  const players = [player('1', 'A'), player('2', 'B'), player('3', 'C')];
  it('coche et décoche un joueur', () => {
    const one = toggle(new Set(), '2');
    expect([...one]).toEqual(['2']);
    expect([...toggle(one, '2')]).toEqual([]);
  });
  it('« tout » coche la liste filtrée, puis la décoche si tout y était', () => {
    const visible = players.slice(0, 2);
    const all = toggleAll(new Set(['3']), visible);
    expect([...all].sort()).toEqual(['1', '2', '3']);
    expect([...toggleAll(all, visible)]).toEqual(['3']);
  });
  it('ne garde que les joueurs encore listés, dans l’ordre de la liste', () => {
    expect(selectedIds(players, new Set(['3', '9', '1']))).toEqual(['1', '3']);
  });
});

const item: AdminItem = {
  id: 'i1', name: 'Potion', usage: 'consumable', description: null, secret: 'amère', image: null, inventory_size: 0,
  effects_text: 'Force: +1 player', effects: [],
};

describe('brouillon d’objet', () => {
  it('un objet existant garde son nom et laisse le wiki tel quel', () => {
    const d = itemDraft(item);
    expect(d.wiki).toBe('keep');
    const body = itemBody({ ...d, description: '  douce ' });
    expect(body).toEqual({ usage: 'consumable', description: 'douce', secret: 'amère', image: '', inventory_size: 0, effects: 'Force: +1 player', wiki: 'keep' });
    expect('name' in body).toBe(false);
  });
  it('un nouvel objet envoie son nom, vérifié comme par la commande slash', () => {
    const d = { ...itemDraft(null), name: 'Epee de fer' };
    expect(itemDraftError(d)).toBeNull();
    expect(itemBody(d).name).toBe('Epee de fer');
    expect(itemDraftError({ ...d, name: 'Épée' })).toBe('create_item__invalid_name');
    expect(itemDraftError({ ...d, image: 'http://x' })).toBe('activity__invalid_image_url');
    expect(itemDraftError({ ...d, inventorySize: -1 })).toBe('activity_ui__admin_invalid_number');
  });
});

const recipe: AdminRecipe = {
  id: 'r1', name: 'Pain', delay: 120, in_wiki: true,
  ingredients: [{ item_id: 'a', name: 'Farine', quantity: 2 }],
  results: [{ item_id: 'b', name: 'Pain', quantity: 1 }],
  tools: [{ item_id: 'c', name: 'Four', quantity: 1 }],
};

describe('brouillon de recette', () => {
  it('fait l’aller-retour avec le format du serveur', () => {
    expect(recipeBody(recipeDraft(recipe))).toEqual({
      name: 'Pain', delay: 120, into_wiki: true,
      ingredients: [{ item_id: 'a', quantity: 2 }], results: [{ item_id: 'b', quantity: 1 }], tools: ['c'],
    });
  });
  it('signale le premier problème', () => {
    const d = recipeDraft(recipe);
    expect(recipeDraftError(d)).toBeNull();
    expect(recipeDraftError({ ...d, name: ' ' })).toBe('activity__admin_invalid_recipe_name');
    expect(recipeDraftError({ ...d, ingredients: [], results: [] })).toBe('recipe__empty_recipe');
    expect(recipeDraftError({ ...d, tools: [''] })).toBe('activity_ui__admin_pick_item');
    expect(recipeDraftError({ ...d, ingredients: [{ itemId: 'a', quantity: 0 }] })).toBe('recipe__invalid_quantity');
    expect(recipeDraftError({ ...d, delay: 1.5 })).toBe('activity_ui__admin_invalid_number');
  });
});

describe('durées', () => {
  it('convertit et choisit l’unité la plus lisible', () => {
    expect(toSeconds(2, 'h')).toBe(7200);
    expect(Number.isNaN(toSeconds(-1, 'm'))).toBe(true);
    expect(bestUnit(7200)).toEqual({ value: 2, unit: 'h' });
    expect(bestUnit(90)).toEqual({ value: 90, unit: 's' });
    expect(bestUnit(0)).toEqual({ value: 0, unit: 's' });
  });
});
