import { describe, expect, it } from 'vitest';
import type { ImportPlan, ServerChoice, UploadedMap } from '../api';
import { blocker, estimatedMinutes, initialChoices, missingServers, nameConfirmed, optionsForSlot, percent, planRequest, roadKey, setSlot, slotCount, summary, toggleSecret } from './importModel';

const map = (over: Partial<UploadedMap> = {}): UploadedMap => ({
  name: 'Moria', version: '1.158', width: 100, height: 100, distance_unit: 'km', route_count: 1, burgs: [], terrain: null,
  suggestions: [], has_universe: false, ...over,
});
const plan = (servers: (string | null)[], over: Partial<ImportPlan> = {}): ImportPlan => ({
  places: [], roads: [], remove_places: [], remove_roads: [], cut_roads: 0, isolated: [],
  servers: servers.map((g) => ({ guild_id: g, budget: { places: 0, roads_hosted: 0, road_categories: 0, channels: 0, roles: 0 } })),
  ...over,
});
const server = (id: string, over: Partial<ServerChoice> = {}): ServerChoice => ({ guild_id: id, name: id, linked: 'none', eligible: true, current: false, ...over });

describe('import choices', () => {
  it('starts with replace without universe, merge with exact matches otherwise', () => {
    expect(initialChoices(map()).mode).toBe('replace');
    const c = initialChoices(map({ has_universe: true, suggestions: [
      { place_id: '10', place_name: 'Bree', status: 'exact', burgs: [3] },
      { place_id: '11', place_name: 'X', status: 'ambiguous', burgs: [4, 5] },
    ] }));
    expect(c.mode).toBe('merge');
    expect([...c.attach]).toEqual([[3, '10']]);
  });

  it('toggles secret roads whatever the order of their ends', () => {
    let s = toggleSecret(new Set(), 5, 2);
    expect([...s]).toEqual(['2-5']);
    s = toggleSecret(s, 2, 5);
    expect(s.size).toBe(0);
    expect(roadKey(9, 1)).toBe('1-9');
  });

  it('builds a request with filled, distinct servers only', () => {
    const c = { ...initialChoices(map()), extraServers: ['a', null, 'a', 'b'], secret: new Set(['1-2']) };
    const r = planRequest(c);
    expect(r.servers).toEqual(['a', 'b']);
    expect(r.secret).toEqual([[1, 2]]);
    expect(r.attach).toEqual([]); // rattachements seulement en mode « créer en plus »
  });
});

describe('server slots (Q11)', () => {
  it('shows one slot per server of the plan', () => {
    const c = initialChoices(map());
    expect(slotCount(plan(['g', null, null]), c)).toBe(3);
    expect(slotCount(null, c)).toBe(1);
  });

  it('offers eligible servers not used elsewhere', () => {
    const servers = [server('cur', { current: true }), server('a'), server('b'), server('o', { eligible: false, linked: 'other' })];
    const c = setSlot(initialChoices(map()), 1, 'a');
    expect(optionsForSlot(servers, c, 1).map((s) => s.guild_id)).toEqual(['a', 'b']);
    expect(optionsForSlot(servers, c, 2).map((s) => s.guild_id)).toEqual(['b']);
  });

  it('blocks the import while a server is missing', () => {
    expect(missingServers(plan(['g', null]))).toBe(1);
    expect(blocker(plan(['g', null]))).toBe('activity_ui__import_servers_missing');
    expect(blocker(plan(['g', 'h']))).toBeNull();
    expect(blocker(null)).toBe('activity_ui__import_wait_preview');
    expect(blocker(plan(['g'], { remove_roads: ['1'] }))).toBe('activity__import_removal_not_ready');
  });

  it('clears trailing empty slots', () => {
    let c = setSlot(initialChoices(map()), 2, 'b');
    expect(c.extraServers).toEqual([null, 'b']);
    c = setSlot(c, 2, null);
    expect(c.extraServers).toEqual([]);
  });
});

describe('confirmation and progress', () => {
  it('requires the exact universe name', () => {
    expect(nameConfirmed('Moria', ' Moria ')).toBe(true);
    expect(nameConfirmed('Moria', 'moria')).toBe(false);
    expect(nameConfirmed(' ', ' ')).toBe(false);
  });

  it('summarises the plan', () => {
    const p = plan(['g'], {
      places: [
        { burg_id: 1, name: 'A', x: 0, y: 0, capital: false, action: { kind: 'create' }, server: 0 },
        { burg_id: 2, name: 'B', x: 0, y: 0, capital: false, action: { kind: 'keep', category_id: '9' }, server: 0 },
      ],
      roads: [{ burg_a: 1, burg_b: 2, distance: 3, uses_sea: false, secret: false, path: [], action: { kind: 'update', channel_id: '7' }, host: 0 }],
      cut_roads: 0, isolated: [5],
    });
    expect(summary(p)).toEqual({ createPlaces: 1, keptPlaces: 1, createRoads: 0, updatedRoads: 1, servers: 1, cutRoads: 0, isolated: 1 });
  });

  it('estimates the duration and the progress', () => {
    expect(estimatedMinutes(10)).toBe(1);
    expect(estimatedMinutes(2288)).toBe(20);
    expect(percent({ phase: 'Creating', done: 5, total: 20 })).toBe(25);
    expect(percent({ phase: 'Done', done: 0, total: 0 })).toBe(100);
  });
});
