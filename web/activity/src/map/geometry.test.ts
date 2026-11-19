import { describe, expect, it } from 'vitest';
import type { MapView } from '../api';
import { positionOnRoad, SCENE_SIZE, terrainHeight, toScene, traveledAt } from './geometry';

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
