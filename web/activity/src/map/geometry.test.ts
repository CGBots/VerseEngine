import { describe, expect, it } from 'vitest';
import type { MapView } from '../api';
import { fitDistance, frameView, viewElevation, initialBox, lerpPoint, markerScale, placesBox, placesFrame, pointAlong, positionOnRoad, roadPath, roadProgress, SCENE_SIZE, smoothToward, splitAlong, terrainHeight, toScene, traveledAt } from './geometry';

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
  const m = { kind: 'road' as const, from: { x: 0, y: 0 }, to: { x: 100, y: 0 }, path: [{ x: 0, y: 0 }, { x: 100, y: 0 }], traveledKm: 10, roadKm: 100, speedKmh: 36, refTime: 1000, moving: true };
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

describe('suivi du tracé des routes', () => {
  // Route en L : (0,0) → (10,0) → (10,10), longueur 20.
  const L = [{ x: 0, y: 0 }, { x: 10, y: 0 }, { x: 10, y: 10 }];
  it('avance le long du tracé et non en ligne droite', () => {
    expect(pointAlong(L, 0)).toEqual({ x: 0, y: 0 });
    expect(pointAlong(L, 0.25)).toEqual({ x: 5, y: 0 });
    expect(pointAlong(L, 0.5)).toEqual({ x: 10, y: 0 }); // le coude, pas (5, 5)
    expect(pointAlong(L, 0.75)).toEqual({ x: 10, y: 5 });
    expect(pointAlong(L, 2)).toEqual({ x: 10, y: 10 });
  });
  it('coupe le tracé au point courant', () => {
    const { traveled, remaining } = splitAlong(L, 0.75);
    expect(traveled).toEqual([{ x: 0, y: 0 }, { x: 10, y: 0 }, { x: 10, y: 5 }]);
    expect(remaining).toEqual([{ x: 10, y: 5 }, { x: 10, y: 10 }]);
  });
  const withPath: MapView = {
    ...map,
    roads: [{ id: '10', name: 'r', from: '1', to: '2', distance_km: 100, secret: false, path: [[0, 250], [500, 100], [1000, 250]] }],
  };
  it("prend le tracé Azgaar, dans le sens du voyage", () => {
    expect(roadPath(withPath, '1', '2')).toEqual([{ x: 0, y: 250 }, { x: 500, y: 100 }, { x: 1000, y: 250 }]);
    expect(roadPath(withPath, '2', '1')).toEqual([{ x: 1000, y: 250 }, { x: 500, y: 100 }, { x: 0, y: 250 }]);
  });
  it('raccorde le tracé aux lieux et se rabat sur la ligne droite', () => {
    const shifted: MapView = { ...withPath, roads: [{ ...withPath.roads[0], path: [[3, 249], [500, 100], [998, 251]] }] };
    const p = roadPath(shifted, '1', '2');
    expect(p[0]).toEqual({ x: 0, y: 250 });
    expect(p[p.length - 1]).toEqual({ x: 1000, y: 250 });
    expect(roadPath(map, '1', '2')).toEqual([{ x: 0, y: 250 }, { x: 1000, y: 250 }]);
    expect(roadPath(map, '1', '99')).toEqual([]);
  });
});

describe('initialBox', () => {
  it('remplit une vue portrait sans bande vide, centrée sur le personnage', () => {
    const b = initialBox(1600, 1000, 390, 600, { x: 800, y: 500 });
    expect(b.h).toBe(1000);
    expect(b.w).toBeCloseTo(650);
    expect(b.x).toBeCloseTo(475);
    expect(b.y).toBe(0);
  });
  it('ne sort pas de la carte quand le personnage est au bord', () => {
    const b = initialBox(1600, 1000, 390, 600, { x: 10, y: 10 });
    expect(b.x).toBe(0);
    expect(b.y).toBe(0);
  });
  it('montre toute la largeur en paysage large', () => {
    const b = initialBox(1600, 1000, 1600, 900, null);
    expect(b.w).toBe(1600);
    expect(b.h).toBe(900);
    expect(b.y).toBe(50);
  });
});

describe('markerScale', () => {
  const dense = (n: number, step: number): MapView => ({
    ...map,
    width: 1000,
    height: 1000,
    places: Array.from({ length: n }, (_, i) => ({ id: String(i), name: `P${i}`, x: (i % 10) * step, y: Math.floor(i / 10) * step, z: 0 })),
  });

  it('keeps full-size markers when places are far apart', () => {
    expect(markerScale(map)).toBe(1);
    expect(markerScale({ ...map, places: [] })).toBe(1);
  });

  it('shrinks markers on a dense map, never below a fifth', () => {
    // Écart 20 sur 1000 : 2 unités de scène, pastille de 0,6 unité.
    expect(markerScale(dense(100, 20))).toBeCloseTo(0.48);
    expect(markerScale(dense(100, 1))).toBe(0.2);
  });

  it('uses the fog radius under fog, since only a few places are sent', () => {
    const fogged = { ...dense(2, 500), fog: { radius: 12, discovered: ['0'], glimpsed: [], roads: [] } };
    // Écart 12 / 0,6 = 20 : comme la carte dense ci-dessus.
    expect(markerScale(fogged)).toBeCloseTo(0.48);
  });
});

describe('3D framing', () => {
  const frame = { x: 0, z: 0, w: 100, d: 100 };
  it('moves the camera back on a tall screen so the width still fits', () => {
    const wide = fitDistance(frame, 45, 16 / 9, 0.75);
    const tall = fitDistance(frame, 45, 0.45, 0.75);
    expect(tall).toBeGreaterThan(wide * 2.5);
    // Écran large : c'est la profondeur qui limite.
    expect(wide).toBeCloseTo((50 * 0.75) / Math.tan(Math.PI / 8) * 1.08, 5);
  });

  it('frames the places, not the whole sea around them', () => {
    const f = placesFrame(map);
    expect(f.w).toBeLessThanOrEqual(SCENE_SIZE);
    expect(f.x).toBeCloseTo(0);
    const one = placesFrame({ ...map, places: map.places.slice(0, 1) });
    expect(one.w).toBe(SCENE_SIZE);
  });
});

describe('viewElevation', () => {
  it('looks down more steeply on a tall screen', () => {
    expect(viewElevation(16 / 9)).toBe(48);
    expect(viewElevation(0.45)).toBe(72);
    const mid = viewElevation(0.95);
    expect(mid).toBeGreaterThan(48);
    expect(mid).toBeLessThan(72);
  });
});

describe('placesBox (2D)', () => {
  const base = { source: 'Computed' as const, width: 1000, height: 1000, terrain: null, zone: null, background: null, server_time: 0, roads: [] };
  it('cadre les lieux avec une marge, plus grande en haut pour les noms', () => {
    const f = placesBox({ ...base, places: [{ id: '1', name: 'A', x: 0, y: 0, z: 0 }, { id: '2', name: 'B', x: 1000, y: 1000, z: 0 }] });
    expect(f.x).toBeCloseTo(-60);
    expect(f.y).toBeCloseTo(-78);
    expect(f.x + f.w).toBeCloseTo(1060);
    expect(f.y + f.h).toBeCloseTo(1060);
  });
  it('un lieu seul : au moins un quart de la carte ; aucun lieu : toute la carte', () => {
    const f = placesBox({ ...base, places: [{ id: '1', name: 'A', x: 500, y: 500, z: 0 }] });
    expect([f.w, f.h]).toEqual([250, 250]);
    expect(f.x + f.w / 2).toBeCloseTo(500);
    expect(placesBox({ ...base, places: [] })).toEqual({ x: 0, y: 0, w: 1000, h: 1000 });
  });
});

describe('frameView', () => {
  const frame = { x: 0, y: 0, w: 2000, h: 1000 };
  it('montre toute la zone quand les proportions sont proches', () => {
    const v = frameView(frame, 1600, 1000);
    expect(v.w).toBeCloseTo(2000);
    expect(v.h).toBeCloseTo(1250);
    expect(v.y).toBeCloseTo(-125);
  });
  it('sur un téléphone en portrait, resserre autour du personnage sans sortir de la zone', () => {
    const v = frameView(frame, 400, 800, { x: 1900, y: 500 });
    // Remplir exactement : 500 × 1000 ; on montre au plus 1,5 fois cela.
    expect(v.w).toBeCloseTo(750);
    expect(v.h).toBeCloseTo(1500);
    expect(v.x + v.w).toBeCloseTo(2000);
    expect(v.y).toBeCloseTo(-250);
  });
});
