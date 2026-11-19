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

/** Point dans le repère de la carte (pixels Azgaar ou unités du placement calculé). */
export interface MapPoint {
  x: number;
  y: number;
}

/**
 * Position d'un groupe sur une route, dans le repère de la carte : interpolation linéaire entre le
 * lieu de départ et la destination, selon la distance parcourue. `null` si un des lieux n'est pas
 * sur la carte.
 */
export function positionOnRoad(
  map: MapView,
  sourceId: string,
  destinationId: string,
  traveledKm: number,
  roadKm: number,
): MapPoint | null {
  const a = map.places.find((p) => p.id === sourceId);
  const b = map.places.find((p) => p.id === destinationId);
  if (!a || !b) return null;
  const t = roadKm > 0 ? Math.min(1, Math.max(0, traveledKm / roadKm)) : 0;
  return { x: a.x + (b.x - a.x) * t, y: a.y + (b.y - a.y) * t };
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

/** Mouvement du joueur, tel que le serveur l'a décrit à l'instant `refTime` (secondes, heure serveur). */
export type PlayerMotion =
  | { kind: 'place'; at: MapPoint }
  | { kind: 'road'; from: MapPoint; to: MapPoint; traveledKm: number; roadKm: number; speedKmh: number; refTime: number; moving: boolean };

/**
 * Avancement (0–1) sur la route à l'instant `now`, extrapolé à la vitesse courante, comme le
 * compte à rebours. Ne dépasse jamais l'arrivée : le serveur confirme l'arrivée lui-même.
 */
export function roadProgress(m: Extract<PlayerMotion, { kind: 'road' }>, now: number): number {
  if (m.roadKm <= 0) return 0;
  const km = m.moving ? traveledAt(m.traveledKm, m.speedKmh, m.refTime, now) : m.traveledKm;
  return Math.min(1, Math.max(0, km / m.roadKm));
}

export function lerpPoint(a: MapPoint, b: MapPoint, t: number): MapPoint {
  return { x: a.x + (b.x - a.x) * t, y: a.y + (b.y - a.y) * t };
}

/**
 * Rapproche la valeur affichée de la valeur cible : une correction venue du serveur (changement de
 * vitesse, groupe rejoint) fait glisser l'épingle au lieu de la faire sauter. `tau` : constante de
 * temps en secondes.
 */
export function smoothToward(shown: number, target: number, dtSeconds: number, tau = 0.4): number {
  if (!Number.isFinite(shown)) return target;
  const k = 1 - Math.exp(-Math.max(0, dtSeconds) / tau);
  const next = shown + (target - shown) * k;
  return Math.abs(target - next) < 1e-6 ? target : next;
}
