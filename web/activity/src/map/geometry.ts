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
  | { kind: 'road'; from: MapPoint; to: MapPoint; path: MapPoint[]; traveledKm: number; roadKm: number; speedKmh: number; refTime: number; moving: boolean };

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

/** Longueurs cumulées le long d'une polyligne (0 au premier point). */
function cumulative(points: MapPoint[]): number[] {
  const acc = [0];
  for (let i = 1; i < points.length; i++) acc.push(acc[i - 1] + Math.hypot(points[i].x - points[i - 1].x, points[i].y - points[i - 1].y));
  return acc;
}

/** Point situé à la fraction `t` (0–1) de la longueur d'une polyligne. */
export function pointAlong(points: MapPoint[], t: number): MapPoint {
  if (points.length === 0) return { x: 0, y: 0 };
  if (points.length === 1) return points[0];
  const acc = cumulative(points);
  const target = Math.min(1, Math.max(0, t)) * acc[acc.length - 1];
  let i = 1;
  while (i < acc.length - 1 && acc[i] < target) i++;
  const seg = acc[i] - acc[i - 1];
  return lerpPoint(points[i - 1], points[i], seg > 0 ? (target - acc[i - 1]) / seg : 0);
}

/** Coupe une polyligne à la fraction `t` : la partie parcourue et la partie restante. */
export function splitAlong(points: MapPoint[], t: number): { traveled: MapPoint[]; remaining: MapPoint[] } {
  if (points.length < 2) return { traveled: points, remaining: points };
  const acc = cumulative(points);
  const target = Math.min(1, Math.max(0, t)) * acc[acc.length - 1];
  const at = pointAlong(points, t);
  const before = points.filter((_, i) => acc[i] < target);
  const after = points.filter((_, i) => acc[i] > target);
  return { traveled: [...before, at], remaining: [at, ...after] };
}

/**
 * Tracé d'une route entre deux lieux, orienté de `from` vers `to` : le tracé Azgaar s'il existe,
 * sinon la ligne droite. Les extrémités sont raccordées aux lieux (les points Azgaar sont au
 * centre des cellules, pas exactement sur le bourg).
 */
export function roadPath(map: MapView, fromId: string, toId: string): MapPoint[] {
  const a = map.places.find((p) => p.id === fromId);
  const b = map.places.find((p) => p.id === toId);
  if (!a || !b) return [];
  const road = map.roads.find((r) => (r.from === fromId && r.to === toId) || (r.from === toId && r.to === fromId));
  let pts: MapPoint[] = (road?.path ?? []).map(([x, y]) => ({ x, y }));
  if (road && road.from !== fromId) pts = pts.reverse();
  const start = { x: a.x, y: a.y };
  const end = { x: b.x, y: b.y };
  const near = (p: MapPoint, q: MapPoint) => Math.hypot(p.x - q.x, p.y - q.y) < 1e-6;
  if (pts.length === 0 || !near(pts[0], start)) pts = [start, ...pts];
  if (!near(pts[pts.length - 1], end)) pts = [...pts, end];
  return pts;
}

/**
 * Cadrage de départ de la carte 2D : remplit toute la vue (pas de bande vide, même en portrait
 * sur téléphone), centré sur `focus` (le personnage) sans sortir de la carte.
 */
export function initialBox(mapW: number, mapH: number, viewW: number, viewH: number, focus: MapPoint | null) {
  const aspect = viewW > 0 && viewH > 0 ? viewW / viewH : mapW / mapH;
  // Plus grande fenêtre de proportions `aspect` contenue dans la carte.
  let w = mapW;
  let h = w / aspect;
  if (h > mapH) {
    h = mapH;
    w = h * aspect;
  }
  const c = focus ?? { x: mapW / 2, y: mapH / 2 };
  const x = Math.min(mapW - w, Math.max(0, c.x - w / 2));
  const y = Math.min(mapH - h, Math.max(0, c.y - h / 2));
  return { x, y, w, h };
}

/**
 * Échelle des repères 3D (pastilles des lieux, routes), entre 0,2 et 1 : sur une carte dense
 * (Azgaar, des centaines de bourgs), des pastilles de taille fixe se chevaucheraient. L'écart entre
 * lieux est la distance médiane au plus proche voisin ; sous brouillard, où seuls quelques lieux
 * sont envoyés, on le déduit du rayon de la brume (60 % de la longueur médiane des routes).
 */
export function markerScale(map: MapView): number {
  const side = Math.max(map.width, map.height) || 1;
  let spacing: number | null = null;
  if (map.fog && map.fog.radius > 0) spacing = map.fog.radius / 0.6;
  else if (map.places.length >= 2) {
    const nearest = map.places.map((p) =>
      Math.min(...map.places.filter((q) => q !== p).map((q) => Math.hypot(p.x - q.x, p.y - q.y))),
    );
    nearest.sort((a, b) => a - b);
    spacing = nearest[Math.floor(nearest.length / 2)];
  }
  if (spacing === null || !Number.isFinite(spacing)) return 1;
  // Pastille de 1,25 unité de scène pour un écart d'au moins 4 unités (≈ 30 % de l'écart).
  const scene = (spacing / side) * SCENE_SIZE;
  return Math.min(1, Math.max(0.2, (scene * 0.3) / 1.25));
}
