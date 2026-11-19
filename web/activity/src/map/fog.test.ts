import { describe, expect, it } from 'vitest';
import type { MapView } from '../api';
import { fogShapes, GLIMPSE } from './fog';

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
