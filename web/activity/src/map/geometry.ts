import type { MapView } from '../api';

/** Côté de la scène 3D (unités three.js). La carte est ramenée dans [-SIZE/2, SIZE/2]². */
export const SCENE_SIZE = 100;

export interface ScenePoint {
  x: number;
  z: number;
}

/** Projette une position de carte (origine en haut à gauche, y vers le bas) dans la scène. */
export function toScene(x: number, y: number, width: number, height: number): ScenePoint {
  const side = Math.max(width, height) || 1;
  return {
    x: ((x - width / 2) / side) * SCENE_SIZE,
    z: ((y - height / 2) / side) * SCENE_SIZE,
  };
}

/**
 * Position d'un groupe sur une route : interpolation linéaire entre le lieu de départ et la
 * destination, selon la distance parcourue. `null` si un des lieux n'est pas sur la carte.
 */
export function positionOnRoad(
  map: MapView,
  sourceId: string,
  destinationId: string,
  traveledKm: number,
  roadKm: number,
): ScenePoint | null {
  const a = map.places.find((p) => p.id === sourceId);
  const b = map.places.find((p) => p.id === destinationId);
  if (!a || !b) return null;
  const t = roadKm > 0 ? Math.min(1, Math.max(0, traveledKm / roadKm)) : 0;
  const pa = toScene(a.x, a.y, map.width, map.height);
  const pb = toScene(b.x, b.y, map.width, map.height);
  return { x: pa.x + (pb.x - pa.x) * t, z: pa.z + (pb.z - pa.z) * t };
}

/** Distance parcourue à l'instant `now` (même formule que le serveur, `calculate_distance_at`). */
export function traveledAt(
  traveledKmAtStep: number,
  speedKmh: number,
  stepStart: number | null,
  now: number,
): number {
  if (stepStart == null || speedKmh <= 0) return traveledKmAtStep;
  return traveledKmAtStep + (Math.max(0, now - stepStart) / 3600) * speedKmh;
}

/** Hauteur (0–1) du terrain au point de carte (x, y), échantillon le plus proche. */
export function terrainHeight(map: MapView, x: number, y: number): number {
  const t = map.terrain;
  if (!t || t.cols === 0 || t.rows === 0) return 0;
  const c = Math.min(t.cols - 1, Math.max(0, Math.floor((x / map.width) * t.cols)));
  const r = Math.min(t.rows - 1, Math.max(0, Math.floor((y / map.height) * t.rows)));
  return (t.heights[r * t.cols + c] ?? 0) / 255;
}

/** Vrai si le navigateur peut créer un contexte WebGL (sinon : carte 2D SVG). */
export function webglAvailable(): boolean {
  try {
    const canvas = document.createElement('canvas');
    return !!(canvas.getContext('webgl2') ?? canvas.getContext('webgl'));
  } catch {
    return false;
  }
}
