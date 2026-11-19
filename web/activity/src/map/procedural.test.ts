import { describe, expect, it } from 'vitest';
import type { MapView } from '../api';
import { biomeColor, fbm, LAND, proceduralTerrain, seedOf } from './procedural';

const map: MapView = {
  source: 'Computed', width: 1000, height: 600, terrain: null, zone: null, background: null, server_time: 0, roads: [],
  places: [
    { id: '1', name: 'A', x: 150, y: 300, z: 0 },
    { id: '2', name: 'B', x: 500, y: 200, z: 0 },
    { id: '3', name: 'C', x: 850, y: 450, z: 0 },
  ],
};

const at = (t: ReturnType<typeof proceduralTerrain>, x: number, y: number) =>
  t.heights[Math.floor((y / map.height) * t.rows) * t.cols + Math.floor((x / map.width) * t.cols)] / 255;

describe('proceduralTerrain', () => {
  it('est déterministe et ne dépend pas de l’ordre des lieux', () => {
    const a = proceduralTerrain(map, 40, 24);
    const b = proceduralTerrain({ ...map, places: [...map.places].reverse() }, 40, 24);
    expect(a).toEqual(b);
  });
  it('met chaque lieu sur la terre ferme', () => {
    const t = proceduralTerrain(map, 80, 48);
    for (const p of map.places) expect(at(t, p.x, p.y)).toBeGreaterThanOrEqual(LAND);
  });
  it('laisse de l’eau loin des lieux', () => {
    const t = proceduralTerrain(map, 80, 48);
    expect(t.heights.some((h) => h / 255 < LAND)).toBe(true);
  });
  it('change avec l’univers', () => {
    const other = { ...map, places: map.places.map((p) => ({ ...p, id: p.id + 'x' })) };
    expect(proceduralTerrain(map, 40, 24)).not.toEqual(proceduralTerrain(other, 40, 24));
  });
});

describe('bruit et couleurs', () => {
  it('reste dans [0, 1]', () => {
    for (let i = 0; i < 200; i++) {
      const v = fbm(i * 0.37, i * 0.11, seedOf('s'));
      expect(v).toBeGreaterThanOrEqual(0);
      expect(v).toBeLessThanOrEqual(1);
    }
  });
  it("distingue l'eau de la terre", () => {
    const [, , blueWater] = biomeColor(0.1);
    const [, greenLand] = biomeColor(0.35);
    expect(blueWater).toBeGreaterThan(80);
    expect(greenLand).toBeGreaterThan(110);
  });
});
