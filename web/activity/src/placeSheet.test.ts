import { describe, expect, it } from 'vitest';
import type { LootState } from './api';
import { charCount, formFromState, formToBody, newEntry, newItemRow, writableName, type LootForm } from './placeSheet';

const known = new Set(['or', 'plastron', 'Épée']);

function state(partial: Partial<LootState> = {}): LootState {
  return {
    target: 'category', channel_id: '1', central_available: true, exists: true, raw_text: '', items: [...known], can_edit: true, server_time: 0,
    rate_limit: 3600, delay: null,
    entries: [
      { kind: 'item', name: 'or', probability: 40, min: 5, max: 20, stock: null, secret: false },
      { kind: 'set', name: 'armure', probability: 20.5, min: 1, max: 2, stock: 3, secret: true, items: [{ name: 'plastron', probability: 5.2, min: 1, max: 1, stock: 2, secret: true }] },
    ],
    ...partial,
  };
}

describe('éditeur de table de butin', () => {
  it('relit la table du serveur et la renvoie à l’identique', () => {
    const form = formFromState(state());
    expect(form.rateLimit).toBe('3600');
    expect(form.delay).toBe('');
    const body = formToBody(form, known);
    expect(body.ok && body.value).toEqual({ entries: state().entries, rate_limit: 3600, delay: null });
  });

  it('accepte la virgule décimale et ignore les espaces autour des noms', () => {
    const form: LootForm = { entries: [{ ...newEntry('item'), name: ' Épée ', probability: '12,5' }], rateLimit: '', delay: '0' };
    const body = formToBody(form, known);
    expect(body.ok && body.value.entries[0]).toMatchObject({ name: 'Épée', probability: 12.5 });
    expect(body.ok && body.value.delay).toBeNull();
  });

  it('situe la première erreur (entrée, objet de l’ensemble)', () => {
    const form = formFromState(state());
    form.entries[1].items[0].min = '3';
    form.entries[1].items[0].max = '2';
    const r = formToBody(form, known);
    expect(r.ok ? null : r.error).toEqual({ key: 'activity__loot_min_above_max', entry: 1, item: 0 });
  });

  it('refuse un objet absent de l’univers, une probabilité hors bornes, un ensemble vide', () => {
    const bad = (patch: Partial<ReturnType<typeof newEntry>>) => {
      const r = formToBody({ entries: [{ ...newEntry('item'), name: 'or', ...patch }], rateLimit: '', delay: '' }, known);
      return r.ok ? null : r.error.key;
    };
    expect(bad({ name: 'inconnu' })).toBe('activity__loot_unknown_item');
    expect(bad({ probability: '0' })).toBe('activity__loot_bad_probability');
    expect(bad({ probability: '101' })).toBe('activity__loot_bad_probability');
    expect(bad({ min: '1e3' })).toBe('activity__loot_bad_quantity');
    expect(bad({ max: '1001' })).toBe('activity__loot_bad_quantity');
    expect(bad({ stock: '0' })).toBe('activity__loot_bad_stock');
    const emptySet = formToBody({ entries: [{ ...newEntry('set'), name: 'lot', items: [] }], rateLimit: '', delay: '' }, known);
    expect(emptySet.ok ? null : emptySet.error.key).toBe('activity__loot_empty_set');
    // Le nom d'un ensemble n'est pas un objet : seul son format compte.
    const set = formToBody({ entries: [{ ...newEntry('set'), name: 'lot', items: [{ ...newItemRow(), name: 'or' }] }], rateLimit: '', delay: '' }, known);
    expect(set.ok).toBe(true);
  });

  it('refuse les noms que la commande slash ne saurait pas relire', () => {
    expect(writableName('épée longue')).toBe(true);
    expect(writableName('a: b')).toBe(false);
    expect(writableName('-tiret')).toBe(false);
    expect(writableName('deux\nlignes')).toBe(false);
  });

  it('borne la recharge et le délai', () => {
    const r = formToBody({ entries: [], rateLimit: '99999999', delay: '' }, known);
    expect(r.ok ? null : r.error).toEqual({ key: 'activity__loot_bad_seconds', entry: null, item: null });
  });

  it('compte les caractères comme le serveur', () => {
    expect(charCount('é😀')).toBe(2);
  });
});
