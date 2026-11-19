import type { GeneratedMap, MapView } from '../api';

/** Points d'un tracé, comme côté serveur (`PATH_POINTS`). */
export const PATH_POINTS = 12;
export const MIN_PLACES = 2;
export const MAX_PLACES = 600;

type Pt = [number, number];
type Place = GeneratedMap['places'][number];

const placeOf = (m: GeneratedMap, id: number) => m.places.find((p) => p.id === id);

/** Courbe de Bézier quadratique de `a` à `b` par `control` (même formule que le serveur). */
export function bezier(a: Pt, b: Pt, control: Pt): Pt[] {
  return Array.from({ length: PATH_POINTS }, (_, k) => {
    const t = k / (PATH_POINTS - 1);
    const u = 1 - t;
    return [u * u * a[0] + 2 * u * t * control[0] + t * t * b[0], u * u * a[1] + 2 * u * t * control[1] + t * t * b[1]];
  });
}

export function roadPoints(m: GeneratedMap, road: GeneratedMap['roads'][number]): Pt[] {
  const a = placeOf(m, road.a);
  const b = placeOf(m, road.b);
  return a && b ? bezier([a.x, a.y], [b.x, b.y], road.control) : [];
}

/** Longueur du tracé en km. */
export function roadKm(m: GeneratedMap, road: GeneratedMap['roads'][number]): number {
  const pts = roadPoints(m, road);
  let len = 0;
  for (let i = 1; i < pts.length; i++) len += Math.hypot(pts[i][0] - pts[i - 1][0], pts[i][1] - pts[i - 1][1]);
  return len * m.km_per_unit;
}

export function renamePlace(m: GeneratedMap, id: number, name: string): GeneratedMap {
  return { ...m, places: m.places.map((p) => (p.id === id ? { ...p, name: name.slice(0, 100) } : p)) };
}

/** Supprime un lieu et ses routes. */
export function deletePlace(m: GeneratedMap, id: number): GeneratedMap {
  if (m.places.length <= MIN_PLACES) return m;
  return { ...m, places: m.places.filter((p) => p.id !== id), roads: m.roads.filter((r) => r.a !== id && r.b !== id) };
}

export function addPlace(m: GeneratedMap, x: number, y: number, name: string): GeneratedMap {
  if (m.places.length >= MAX_PLACES) return m;
  const clamp = (v: number, max: number) => Math.round(Math.min(max, Math.max(0, v)) * 10) / 10;
  const id = Math.max(0, ...m.places.map((p) => p.id)) + 1;
  const place: Place = { id, name, x: clamp(x, m.width), y: clamp(y, m.height), capital: false };
  return { ...m, places: [...m.places, place] };
}

const samePair = (r: { a: number; b: number }, a: number, b: number) => (r.a === a && r.b === b) || (r.a === b && r.b === a);

/** Ajoute une route droite (contrôle au milieu) ; ignorée si elle existe déjà. */
export function addRoad(m: GeneratedMap, a: number, b: number): GeneratedMap {
  const pa = placeOf(m, a);
  const pb = placeOf(m, b);
  if (!pa || !pb || a === b || m.roads.some((r) => samePair(r, a, b))) return m;
  return { ...m, roads: [...m.roads, { a, b, control: [(pa.x + pb.x) / 2, (pa.y + pb.y) / 2] }] };
}

export function deleteRoad(m: GeneratedMap, a: number, b: number): GeneratedMap {
  return { ...m, roads: m.roads.filter((r) => !samePair(r, a, b)) };
}

/** Retouche du tracé : le milieu de la courbe suit le pointeur (contrôle = 2 × milieu − moyenne). */
export function reshapeRoad(m: GeneratedMap, a: number, b: number, middle: Pt): GeneratedMap {
  const pa = placeOf(m, a);
  const pb = placeOf(m, b);
  if (!pa || !pb) return m;
  const control: Pt = [2 * middle[0] - (pa.x + pb.x) / 2, 2 * middle[1] - (pa.y + pb.y) / 2];
  return { ...m, roads: m.roads.map((r) => (samePair(r, a, b) ? { ...r, control } : r)) };
}

/** Milieu de la courbe (poignée de retouche). */
export function roadMiddle(m: GeneratedMap, road: GeneratedMap['roads'][number]): Pt | null {
  const a = placeOf(m, road.a);
  const b = placeOf(m, road.b);
  if (!a || !b) return null;
  return [0.25 * a.x + 0.5 * road.control[0] + 0.25 * b.x, 0.25 * a.y + 0.5 * road.control[1] + 0.25 * b.y];
}

/** Ce qui empêche d'utiliser la carte (clé de traduction), ou `null`. */
export function mapProblem(m: GeneratedMap): string | null {
  if (m.places.length < MIN_PLACES || m.places.length > MAX_PLACES) return 'activity__generate_bad_count';
  if (m.places.some((p) => !p.name.trim())) return 'activity_ui__generate_empty_name';
  return null;
}

export const randomSeed = () => Math.floor(Math.random() * 0xffffffff);

/** Vue de carte pour la scène 3D de l'activité (terrain procédural, pas de relief stocké). */
export function toMapView(m: GeneratedMap): MapView {
  return {
    source: 'Manual',
    width: m.width,
    height: m.height,
    places: m.places.map((p) => ({ id: String(p.id), name: p.name, x: p.x, y: p.y, z: 0 })),
    roads: m.roads.map((r, i) => ({
      id: String(i), name: '', from: String(r.a), to: String(r.b), distance_km: roadKm(m, r), secret: false,
      path: roadPoints(m, r),
    })),
    terrain: null,
    zone: null,
    background: null,
    server_time: 0,
  };
}
