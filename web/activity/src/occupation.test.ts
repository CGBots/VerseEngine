import { describe, expect, it } from 'vitest';
import type { CraftState, FarmState, TravelState } from './api';
import { availability, clampQuantity, currentOccupation, progress } from './occupation';

const travel = (in_move: boolean): TravelState => ({
  location: null,
  travel: { in_move, step_end_timestamp: null, eta_timestamp: null, remaining_km: null, traveled_km: 0, speed_kmh: 5, final_step: true },
  group_members: ['1'],
  group_names: ['Aldric'],
  destinations: [],
  group_speed_kmh: 5,
  nearby: [],
  invites: [],
  server_time: 0,
});
const craft: CraftState = {
  craft: { recipe_id: 'r', recipe_name: 'Pain', quantity: 2, completed: 0, start_timestamp: 0, end_timestamp: 10 },
  server_time: 0,
};
const farm: FarmState = { farm: { start_timestamp: 0, end_timestamp: 10 }, server_time: 0 };
const idle = { craft: { craft: null, server_time: 0 } as CraftState, farm: { farm: null, server_time: 0 } as FarmState };

describe('currentOccupation', () => {
  it("lit l'occupation dans l'état serveur", () => {
    expect(currentOccupation(null, null, null)).toBe(null);
    expect(currentOccupation(travel(false), idle.craft, idle.farm)).toBe(null);
    expect(currentOccupation(travel(true), idle.craft, idle.farm)).toBe('travel');
    expect(currentOccupation(travel(false), craft, idle.farm)).toBe('craft');
    expect(currentOccupation(travel(false), idle.craft, farm)).toBe('farm');
  });
});

describe('availability', () => {
  it('tout est permis au repos', () => {
    expect(availability(null, false)).toEqual({ travel: true, craft: true, inventory: true, farm: true, startCraft: true });
  });

  it('le voyage grise tout', () => {
    expect(availability('travel', false)).toEqual({ travel: false, craft: false, inventory: true, farm: false, startCraft: false });
  });

  it('le craft bloque voyage et farm mais garde son livre de recettes ouvert', () => {
    expect(availability('craft', false)).toEqual({ travel: false, craft: true, inventory: true, farm: false, startCraft: false });
  });

  it('le farm bloque voyage et craft', () => {
    expect(availability('farm', false)).toEqual({ travel: false, craft: false, inventory: true, farm: false, startCraft: false });
  });

  it("une requête en cours empêche d'en lancer une autre", () => {
    expect(availability(null, true)).toEqual({ travel: false, craft: true, inventory: true, farm: false, startCraft: false });
  });
});

describe('progress', () => {
  it('avance de 0 à 1 et reste borné', () => {
    expect(progress(100, 200, 50)).toBe(0);
    expect(progress(100, 200, 150)).toBe(0.5);
    expect(progress(100, 200, 500)).toBe(1);
  });

  it('une durée nulle est finie dès son échéance', () => {
    expect(progress(100, 100, 99)).toBe(0);
    expect(progress(100, 100, 100)).toBe(1);
  });
});

describe('clampQuantity', () => {
  it('ramène la saisie entre 1 et le maximum faisable', () => {
    expect(clampQuantity(5, 3)).toBe(3);
    expect(clampQuantity(0, 3)).toBe(1);
    expect(clampQuantity(-4, 3)).toBe(1);
    expect(clampQuantity(2.7, 3)).toBe(2);
    expect(clampQuantity(Number.NaN, 3)).toBe(1);
  });

  it('rien de faisable donne 0', () => {
    expect(clampQuantity(1, 0)).toBe(0);
  });
});
