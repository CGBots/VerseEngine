import { describe, expect, it } from 'vitest';
import type { MapView } from '../api';
import { lerpPoint, positionOnRoad, roadProgress, SCENE_SIZE, smoothToward, terrainHeight, toScene, traveledAt } from './geometry';

const map: MapView = {
  source: 'Computed',
  width: 1000,
  height: 500,
  places: [
    { id: '1', name: 'A', x: 0, y: 250, z: 0 },
    { id: '2', name: 'B', x: 1000, y: 250, z: 0 },
  ],
  roads: [{ id: '10', name: 'r', from: '1', to: '2', distance_km: 100, secret: false }],
  terrain: { cols: 2, rows: 1, heights: [0, 255] },
  zone: null,
  background: null,
  server_time: 0,
};

describe('toScene', () => {
  it('centre la carte et garde ses proportions', () => {
    expect(toScene(500, 250, 1000, 500)).toEqual({ x: 0, z: 0 });
    expect(toScene(1000, 500, 1000, 500)).toEqual({ x: SCENE_SIZE / 2, z: SCENE_SIZE / 4 });
  });
});

describe('positionOnRoad', () => {
  it('interpole selon la distance parcourue, dans le sens du trajet', () => {
    expect(positionOnRoad(map, '1', '2', 25, 100)).toEqual({ x: 250, y: 250 });
    expect(positionOnRoad(map, '2', '1', 25, 100)).toEqual({ x: 750, y: 250 });
  });
  it('reste sur la route même si la distance déborde', () => {
    expect(positionOnRoad(map, '1', '2', 150, 100)).toEqual({ x: 1000, y: 250 });
    expect(positionOnRoad(map, '1', '2', -3, 100)).toEqual({ x: 0, y: 250 });
  });
  it("renvoie null si un lieu n'est pas sur la carte (route secrète masquée)", () => {
    expect(positionOnRoad(map, '1', '99', 10, 100)).toBeNull();
  });
});

describe('traveledAt', () => {
  it("avance à la vitesse de l'étape", () => {
    expect(traveledAt(10, 5, 1000, 1000 + 3600)).toBe(15);
    expect(traveledAt(10, 5, null, 99999)).toBe(10);
    expect(traveledAt(10, 0, 1000, 99999)).toBe(10);
  });
});

describe('terrainHeight', () => {
  it('lit la heightmap au point demandé', () => {
    expect(terrainHeight(map, 100, 100)).toBe(0);
    expect(terrainHeight(map, 900, 100)).toBe(1);
    expect(terrainHeight({ ...map, terrain: null }, 900, 100)).toBe(0);
  });
});

describe('mouvement en temps réel', () => {
  const m = { kind: 'road' as const, from: { x: 0, y: 0 }, to: { x: 100, y: 0 }, traveledKm: 10, roadKm: 100, speedKmh: 36, refTime: 1000, moving: true };
  it("avance à la vitesse de l'étape", () => {
    expect(roadProgress(m, 1000)).toBeCloseTo(0.1);
    expect(roadProgress(m, 1000 + 3600)).toBeCloseTo(0.46); // +36 km en 1 h
  });
  it("s'arrête à l'arrivée et ne bouge pas à l'arrêt", () => {
    expect(roadProgress(m, 1000 + 100 * 3600)).toBe(1);
    expect(roadProgress({ ...m, moving: false }, 99999)).toBeCloseTo(0.1);
    expect(roadProgress({ ...m, roadKm: 0 }, 1000)).toBe(0);
  });
  it('interpole un point', () => {
    expect(lerpPoint({ x: 0, y: 10 }, { x: 100, y: 30 }, 0.25)).toEqual({ x: 25, y: 15 });
  });
  it('rejoint la cible en douceur, sans la dépasser', () => {
    let v = 0;
    const seen: number[] = [];
    for (let i = 0; i < 30; i++) seen.push((v = smoothToward(v, 1, 0.1)));
    expect(seen[0]).toBeGreaterThan(0);
    expect(seen[0]).toBeLessThan(0.5);
    expect(seen.every((x, i) => i === 0 || x >= seen[i - 1])).toBe(true);
    expect(seen[seen.length - 1]).toBeCloseTo(1, 2);
    expect(Math.max(...seen)).toBeLessThanOrEqual(1);
    expect(smoothToward(NaN, 0.7, 0.1)).toBe(0.7);
  });
});
