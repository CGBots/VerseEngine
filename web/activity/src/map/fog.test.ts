import { describe, expect, it } from 'vitest';
import type { MapView } from '../api';
import { fitRect, flattenUnderFog, fogFocus, fogShapes, GLIMPSE } from './fog';

const map = (fog: MapView['fog']): MapView => ({
  source: 'Azgaar', width: 100, height: 100, terrain: null, zone: null, background: null, server_time: 0, fog,
  places: [
    { id: 'a', name: 'A', x: 10, y: 10, z: 0 },
    { id: 'b', name: 'B', x: 50, y: 10, z: 0 },
  ],
  roads: [{ id: 'r', name: '', from: 'a', to: 'b', distance_km: 4, secret: false, path: [[10, 10], [30, 20], [50, 10]] }],
});

describe('fog shapes', () => {
  it('is absent without fog', () => {
    expect(fogShapes(map(null))).toBeNull();
    expect(fogShapes(map(undefined))).toBeNull();
  });

  it('clears discovered places fully and glimpsed ones partly', () => {
    const s = fogShapes(map({ radius: 8, discovered: ['a'], glimpsed: ['b'], roads: [] }))!;
    expect(s.circles).toEqual([{ x: 10, y: 10, r: 8 }, { x: 50, y: 10, r: 8 * GLIMPSE }]);
    expect(s.corridors).toEqual([]);
  });

  it('opens a corridor along travelled roads, following their path', () => {
    const s = fogShapes(map({ radius: 8, discovered: ['a', 'b'], glimpsed: [], roads: ['r'] }))!;
    expect(s.corridors).toHaveLength(1);
    expect(s.corridors[0].points).toEqual([{ x: 10, y: 10 }, { x: 30, y: 20 }, { x: 50, y: 10 }]);
    expect(s.corridors[0].width).toBeCloseTo(7.2);
  });

  it('ignores ids it does not know', () => {
    const s = fogShapes(map({ radius: 8, discovered: ['zz'], glimpsed: [], roads: ['nope'] }))!;
    expect(s.circles).toEqual([]);
    expect(s.corridors).toEqual([]);
  });
});

describe('fog focus', () => {
  it('is absent without fog or without anything cleared', () => {
    expect(fogFocus(map(null))).toBeNull();
    expect(fogFocus(map({ radius: 8, discovered: [], glimpsed: [], roads: [] }))).toBeNull();
  });

  it('frames what is cleared, with a margin of two radii', () => {
    const r = fogFocus(map({ radius: 8, discovered: ['a', 'b'], glimpsed: [], roads: ['r'] }))!;
    // Cercles de 2 à 58 en x ; en y, de 2 à 20 (le tracé descend à 20) ; marge 16 ; hauteur portée à 8 rayons.
    expect(r.x).toBeCloseTo(2 - 16);
    expect(r.w).toBeCloseTo(56 + 32);
    expect(r.h).toBeCloseTo(64);
    expect(r.y + r.h / 2).toBeCloseTo(11);
  });

  it('is at least eight radii wide around a single place', () => {
    const r = fogFocus(map({ radius: 2, discovered: ['a'], glimpsed: [], roads: [] }))!;
    expect(r).toEqual({ x: 2, y: 2, w: 16, h: 16 });
  });

  it('fits the view proportions around its centre', () => {
    expect(fitRect({ x: 0, y: 0, w: 10, h: 10 }, 200, 100)).toEqual({ x: -5, y: 0, w: 20, h: 10 });
    expect(fitRect({ x: 0, y: 0, w: 10, h: 10 }, 100, 200)).toEqual({ x: 0, y: -5, w: 10, h: 20 });
  });
});

describe('relief under fog', () => {
  const shapes = { circles: [{ x: 5, y: 5, r: 2 }], corridors: [] };
  it('keeps the relief in cleared zones and sinks it to sea level in the fog', () => {
    // Ligne de 10 cellules, x = 0,5 … 9,5 ; zone dégagée de 3 à 7, fondu sur 1 unité.
    const out = flattenUnderFog(new Array(10).fill(200), 10, 1, 10, 1, { circles: [{ x: 5, y: 0.5, r: 2 }], corridors: [] }, 51);
    expect(out.slice(3, 7)).toEqual([200, 200, 200, 200]);
    expect(out[0]).toBe(51);
    expect(out[9]).toBe(51);
    expect(out[2]).toBeGreaterThan(51);
    expect(out[2]).toBeLessThan(200);
  });
  it('leaves the sea alone and follows corridors', () => {
    const corridor = { circles: [], corridors: [{ points: [{ x: 0, y: 0.5 }, { x: 10, y: 0.5 }], width: 2 }] };
    expect(flattenUnderFog([10, 200, 200], 3, 1, 10, 1, corridor, 51)).toEqual([10, 200, 200]);
    expect(flattenUnderFog([200], 1, 1, 1, 1, shapes, 51)).toEqual([51]);
  });
});
