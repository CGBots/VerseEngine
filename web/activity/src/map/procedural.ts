import type { MapView } from '../api';

/**
 * Terrain généré quand la carte n'a ni export Azgaar ni heightmap : déterministe (même univers,
 * même carte), avec des terres autour des lieux et de l'eau au loin. Valeurs 0–255 ; le seuil de
 * la terre est le même que pour Azgaar (20 % de la hauteur).
 */
export const LAND = 0.2;

/** Hachage 32 bits (FNV-1a) d'une chaîne : graine du bruit. */
export function seedOf(text: string): number {
  let h = 0x811c9dc5;
  for (let i = 0; i < text.length; i++) {
    h ^= text.charCodeAt(i);
    h = Math.imul(h, 0x01000193);
  }
  return h >>> 0;
}

function hash2(x: number, y: number, seed: number): number {
  let h = Math.imul(x | 0, 374761393) ^ Math.imul(y | 0, 668265263) ^ seed;
  h = Math.imul(h ^ (h >>> 13), 1274126177);
  return ((h ^ (h >>> 16)) >>> 0) / 4294967295;
}

function smooth(t: number): number {
  return t * t * (3 - 2 * t);
}

/** Bruit de valeur lissé, puis somme fractale (fBm) sur `octaves` octaves. */
export function fbm(x: number, y: number, seed: number, octaves = 5): number {
  let sum = 0;
  let amp = 0.5;
  let freq = 1;
  let norm = 0;
  for (let o = 0; o < octaves; o++) {
    const fx = x * freq;
    const fy = y * freq;
    const x0 = Math.floor(fx);
    const y0 = Math.floor(fy);
    const tx = smooth(fx - x0);
    const ty = smooth(fy - y0);
    const s = seed + o * 1013;
    const a = hash2(x0, y0, s);
    const b = hash2(x0 + 1, y0, s);
    const c = hash2(x0, y0 + 1, s);
    const d = hash2(x0 + 1, y0 + 1, s);
    sum += amp * (a + (b - a) * tx + (c - a) * ty + (a - b - c + d) * tx * ty);
    norm += amp;
    amp *= 0.5;
    freq *= 2;
  }
  return sum / norm;
}

export interface Heightmap {
  cols: number;
  rows: number;
  heights: number[];
}

/** Génère une heightmap `cols × rows` pour la carte. */
export function proceduralTerrain(map: MapView, cols: number, rows: number): Heightmap {
  const seed = seedOf(map.places.map((p) => p.id).sort().join(','));
  const side = Math.max(map.width, map.height) || 1;
  // Rayon d'influence d'un lieu : ~ la distance moyenne au plus proche voisin, borné pour garder
  // de la mer autour des terres même avec très peu de lieux.
  const nearest = map.places.map((p) =>
    Math.min(...map.places.filter((q) => q !== p).map((q) => Math.hypot(p.x - q.x, p.y - q.y)), side),
  );
  const mean = nearest.length ? nearest.reduce((a, b) => a + b, 0) / nearest.length : side;
  const radius = Math.min(side * 0.28, Math.max(side * 0.08, mean * 0.8));
  const heights = new Array<number>(cols * rows);
  for (let r = 0; r < rows; r++) {
    for (let c = 0; c < cols; c++) {
      const x = ((c + 0.5) / cols) * map.width;
      const y = ((r + 0.5) / rows) * map.height;
      // Proximité du lieu le plus proche (1 dessus, 0 au-delà du rayon).
      let near = 0;
      for (const p of map.places) {
        const d = Math.hypot(p.x - x, p.y - y) / radius;
        if (d < 1) near = Math.max(near, 1 - d * d);
      }
      // Déformation du domaine : côtes irrégulières plutôt qu'en arcs de cercle.
      const wx = (x / side) * 3 + fbm((x / side) * 2, (y / side) * 2, seed ^ 0x9e3779b9, 3) * 1.2;
      const wy = (y / side) * 3 + fbm((x / side) * 2 + 7, (y / side) * 2 + 3, seed ^ 0x85ebca6b, 3) * 1.2;
      const noise = fbm(wx, wy, seed, 6);
      // Relief de base (surtout de la mer) + bosse continue autour des lieux : sur un lieu,
      // v ≥ 0,4 (terre) ; au bord du rayon, la bosse s'annule sans marche.
      const v = noise * 0.7 - 0.2 + near * 0.6;
      heights[r * cols + c] = Math.round(Math.min(1, Math.max(0, v)) * 255);
    }
  }
  return { cols, rows, heights };
}

/** Couleur d'un point de terrain selon sa hauteur (0–1) : eau, plage, prairie, forêt, roche, neige. */
export function biomeColor(v: number): [number, number, number] {
  const stops: [number, [number, number, number]][] = [
    [0, [22, 52, 92]],
    [LAND - 0.01, [48, 102, 150]],
    [LAND, [214, 201, 150]],
    [LAND + 0.04, [128, 166, 92]],
    [0.45, [72, 122, 66]],
    [0.62, [92, 110, 74]],
    [0.75, [128, 120, 108]],
    [0.88, [236, 236, 240]],
    [1, [255, 255, 255]],
  ];
  for (let i = 1; i < stops.length; i++) {
    const [h1, c1] = stops[i];
    const [h0, c0] = stops[i - 1];
    if (v <= h1) {
      const t = h1 === h0 ? 0 : (v - h0) / (h1 - h0);
      return [0, 1, 2].map((k) => Math.round(c0[k] + (c1[k] - c0[k]) * t)) as [number, number, number];
    }
  }
  return stops[stops.length - 1][1];
}
