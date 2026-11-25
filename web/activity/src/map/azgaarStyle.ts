import { isoBands } from './seaBands';
import type { BurgIcon, MapStyleView } from '../api';
import { biomeColor, LAND, type Heightmap } from './procedural';
import type { MapPoint } from './geometry';

/**
 * Rendu façon Azgaar de la carte 2D, à partir des **données** envoyées par le serveur
 * (`activity/map/azgaar_style.rs`) : on ne peut ni exécuter Azgaar chez le joueur ni afficher son
 * image sous le brouillard. Tout est calculé ici en fonctions pures (testées) ; le composant ne
 * fait que peindre le résultat dans un canevas.
 */

/** Valeurs de la grille de surface (mêmes constantes que `database/universe_maps.rs`). */
export const SURFACE_OCEAN = 200;
export const SURFACE_LAKE = 230;
export const SURFACE_HIDDEN = 255;

export type Rgb = [number, number, number];

/** Grille (valeur, longueur) en base64 → `length` valeurs (complétée par `fill` si trop courte). */
export function decodeRuns(b64: string | undefined, length: number, fill = 0): Uint8Array {
  const out = new Uint8Array(length).fill(fill);
  if (!b64) return out;
  let raw: string;
  try {
    raw = atob(b64);
  } catch {
    return out;
  }
  let k = 0;
  for (let i = 0; i + 1 < raw.length && k < length; i += 2) {
    const v = raw.charCodeAt(i);
    const n = raw.charCodeAt(i + 1);
    for (let j = 0; j < n && k < length; j++) out[k++] = v;
  }
  return out;
}

/** `#rrggbb` → composantes ; `null` si illisible (les couleurs viennent d'un fichier importé). */
export function hexRgb(c: string | undefined): Rgb | null {
  const m = /^#([0-9a-f]{6})$/i.exec(c ?? '');
  if (!m) return null;
  const n = parseInt(m[1], 16);
  return [(n >> 16) & 255, (n >> 8) & 255, n & 255];
}

export const mix = (a: Rgb, b: Rgb, t: number): Rgb => [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t];

/**
 * Mer d'Azgaar : bandes de plus en plus sombres en s'éloignant de la côte. `d` : distance à la
 * terre, en cases de la grille de surface (une case ≈ une demi-cellule d'Azgaar).
 */
const OCEAN_BANDS: [number, Rgb][] = [
  [2, [163, 190, 225]],
  [5, [147, 175, 216]],
  [10, [129, 158, 205]],
  [18, [110, 140, 192]],
  [Infinity, [98, 128, 182]],
];
export function oceanColor(d: number): Rgb {
  return (OCEAN_BANDS.find(([max]) => d <= max) ?? OCEAN_BANDS[OCEAN_BANDS.length - 1])[1];
}

/**
 * Distance de chaque case à la terre la plus proche (en cases, chanfrein 3-4 : contours presque
 * ronds). Les cases inconnues ne comptent pas comme terre : sous le brouillard, rien ne se devine.
 */
export function distanceToLand(isLand: (k: number) => boolean, cols: number, rows: number): Float32Array {
  const d = new Float32Array(cols * rows).fill(1e9);
  for (let k = 0; k < cols * rows; k++) if (isLand(k)) d[k] = 0;
  const relax = (k: number, c: number, r: number, dc: number, dr: number, w: number) => {
    const cc = c + dc, rr = r + dr;
    if (cc < 0 || rr < 0 || cc >= cols || rr >= rows) return;
    const v = d[rr * cols + cc] + w;
    if (v < d[k]) d[k] = v;
  };
  for (let r = 0; r < rows; r++) {
    for (let c = 0; c < cols; c++) {
      const k = r * cols + c;
      relax(k, c, r, -1, 0, 3), relax(k, c, r, 0, -1, 3), relax(k, c, r, -1, -1, 4), relax(k, c, r, 1, -1, 4);
    }
  }
  for (let r = rows - 1; r >= 0; r--) {
    for (let c = cols - 1; c >= 0; c--) {
      const k = r * cols + c;
      relax(k, c, r, 1, 0, 3), relax(k, c, r, 0, 1, 3), relax(k, c, r, 1, 1, 4), relax(k, c, r, -1, 1, 4);
    }
  }
  for (let k = 0; k < d.length; k++) d[k] /= 3;
  return d;
}
/** Couleur du large, aussi autour de la carte (marges), avec ou sans l'aspect parchemin. */
export function seaCss(paper: boolean): string {
  const c = paper ? parchment(oceanColor(Infinity)) : oceanColor(Infinity);
  return `rgb(${c.map((v) => Math.round(v)).join(',')})`;
}
export const LAKE: Rgb = [150, 185, 235];
export const COAST: Rgb = [46, 62, 80];
const HIDDEN: Rgb = [143, 152, 164];
const PAPER: Rgb = [233, 216, 178];
export const WHITE: Rgb = [255, 255, 255];

/** Aspect parchemin : couleurs ramenées vers un sépia sur papier. */
export function parchment(c: Rgb): Rgb {
  const l = 0.3 * c[0] + 0.59 * c[1] + 0.11 * c[2];
  const sepia: Rgb = [Math.min(255, l * 1.07 + 30), Math.min(255, l * 0.95 + 20), Math.min(255, l * 0.72 + 8)];
  return mix(mix(c, sepia, 0.6), PAPER, 0.18);
}

/** Hauteur 0–1 interpolée (coordonnées en cellules, centre de cellule à `+0,5`). */
function sample(hm: Heightmap, fx: number, fy: number): number {
  const x = Math.min(hm.cols - 1, Math.max(0, fx - 0.5));
  const y = Math.min(hm.rows - 1, Math.max(0, fy - 0.5));
  const x0 = Math.floor(x), y0 = Math.floor(y);
  const x1 = Math.min(hm.cols - 1, x0 + 1), y1 = Math.min(hm.rows - 1, y0 + 1);
  const h = (c: number, r: number) => (hm.heights[r * hm.cols + c] ?? 0) / 255;
  const tx = x - x0, ty = y - y0;
  return (h(x0, y0) * (1 - tx) + h(x1, y0) * tx) * (1 - ty) + (h(x0, y1) * (1 - tx) + h(x1, y1) * tx) * ty;
}

/**
 * Relief ombré (lumière du nord-ouest, comme les cartes papier) : facteur multiplicatif, 1 sur
 * un terrain plat. `u`, `v` : position relative (0–1) dans la carte.
 */
export function hillshade(hm: Heightmap | null, u: number, v: number, strength = 1): number {
  if (!hm || hm.cols < 2 || hm.rows < 2) return 1;
  const fx = u * hm.cols, fy = v * hm.rows;
  const dx = (sample(hm, fx + 1, fy) - sample(hm, fx - 1, fy)) * 255;
  const dy = (sample(hm, fx, fy + 1) - sample(hm, fx, fy - 1)) * 255;
  const k = 0.09 * strength;
  const n = [-dx * k, -dy * k, 2];
  const len = Math.hypot(n[0], n[1], n[2]);
  const light = [-1, -1, 1.4];
  const ll = Math.hypot(light[0], light[1], light[2]);
  const shade = (n[0] * light[0] + n[1] * light[1] + n[2] * light[2]) / (len * ll);
  const flat = 1.4 / ll;
  return Math.min(1.25, Math.max(0.55, 1 + (shade - flat) * 1.1));
}

export interface Raster {
  width: number;
  height: number;
  /** RGBA, ligne par ligne. */
  data: Uint8ClampedArray<ArrayBuffer>;
}

export interface StyleOptions {
  /** Couleurs et frontières des États (si la carte en a). */
  states: boolean;
  parchment: boolean;
  /** Terres peintes comme la mer côtière : les contours vectoriels (`style.vector`) dessinent
   *  les terres par-dessus, sans double côte. */
  landAsSea?: boolean;
  /** Mer d'une seule couleur (la plus profonde) : les bandes sont dessinées en vecteurs
   *  (`seaLayers`) par-dessus, sans escaliers dessous. */
  flatSea?: boolean;
}

/** Nature d'une case : eau, terre, inconnue (sous le brouillard). */
const WATER = 0, LANDK = 1, UNKNOWN = 2;

/** Cases de base, avant l'agrandissement : couleur (sans côtes ni frontières), nature, État. */
interface Cells {
  cols: number;
  rows: number;
  color: Float32Array;
  kind: Uint8Array;
  state: Uint8Array | null;
  stateColors: (Rgb | null)[];
  /** Distance à la terre des cases de mer (`NaN` : lac, couleur propre). Interpolée pixel par
   * pixel, elle donne des bandes de mer aux contours lisses. */
  depth: Float32Array | null;
}

/**
 * Part de terre autour de chaque case, adoucie (flou binomial 5 × 5) mais qui garde la nature de la
 * case en son centre : interpolée puis coupée à 0,5, elle donne des côtes arrondies au lieu
 * d'escaliers, sans effacer les îlots d'une case.
 */
export function landField(kind: Uint8Array, cols: number, rows: number): Float32Array {
  const out = new Float32Array(cols * rows);
  const at = (c: number, r: number) => kind[Math.min(rows - 1, Math.max(0, r)) * cols + Math.min(cols - 1, Math.max(0, c))] === LANDK ? 1 : 0;
  const w = [1, 4, 6, 4, 1];
  for (let r = 0; r < rows; r++) {
    for (let c = 0; c < cols; c++) {
      let sum = 0;
      for (let dr = -2; dr <= 2; dr++) for (let dc = -2; dc <= 2; dc++) sum += at(c + dc, r + dr) * w[dr + 2] * w[dc + 2];
      const blur = sum / 256;
      out[r * cols + c] = at(c, r) ? Math.max(blur, 0.6) : Math.min(blur, 0.4);
    }
  }
  return out;
}

/**
 * Agrandit les cases (`factor` pixels par case) : couleurs interpolées entre cases de même
 * nature, et nature de chaque pixel d'après la part de terre interpolée (`landField`). Les côtes
 * sont donc des courbes nettes au lieu d'escaliers ; elles sont ensuite tracées d'un pixel
 * sombre, comme les frontières des États.
 */
/**
 * Part adoucie (flou binomial 5 × 5) de chaque État autour de chaque case de terre : au plus
 * `STATE_SLOTS` États par case (les plus présents). Interpolée pixel par pixel, l'État dominant
 * donne des frontières arrondies au lieu d'escaliers.
 */
const STATE_SLOTS = 3;
export function stateField(kind: Uint8Array, state: Uint8Array, cols: number, rows: number): { ids: Uint16Array; vals: Float32Array } {
  const ids = new Uint16Array(cols * rows * STATE_SLOTS).fill(0xffff);
  const vals = new Float32Array(cols * rows * STATE_SLOTS);
  const w = [1, 4, 6, 4, 1];
  const acc = new Map<number, number>();
  for (let r = 0; r < rows; r++) {
    for (let c = 0; c < cols; c++) {
      const k = r * cols + c;
      if (kind[k] !== LANDK) continue;
      acc.clear();
      let total = 0;
      for (let dr = -2; dr <= 2; dr++) {
        const rr = Math.min(rows - 1, Math.max(0, r + dr));
        for (let dc = -2; dc <= 2; dc++) {
          const q = rr * cols + Math.min(cols - 1, Math.max(0, c + dc));
          if (kind[q] !== LANDK) continue;
          const wt = w[dr + 2] * w[dc + 2];
          acc.set(state[q], (acc.get(state[q]) ?? 0) + wt);
          total += wt;
        }
      }
      // La case garde son propre État en tête (pas d'enclave d'une case effacée).
      const own = acc.get(state[k]) ?? 0;
      acc.set(state[k], Math.max(own, total * 0.6));
      [...acc.entries()].sort((a, b) => b[1] - a[1]).slice(0, STATE_SLOTS).forEach(([id, v], i) => {
        ids[k * STATE_SLOTS + i] = id;
        vals[k * STATE_SLOTS + i] = v / total;
      });
    }
  }
  return { ids, vals };
}

function paint(cells: Cells, factor: number, opts: StyleOptions): Raster {
  const { cols, rows } = cells;
  const field = landField(cells.kind, cols, rows);
  const sf = cells.state ? stateField(cells.kind, cells.state, cols, rows) : null;
  const f = Math.max(1, Math.round(factor));
  const W = cols * f, H = rows * f;
  const kind = new Uint8Array(W * H);
  const state = new Uint8Array(W * H);
  const color = new Float32Array(W * H * 3);
  for (let Y = 0; Y < H; Y++) {
    const fy = Math.min(rows - 1, Math.max(0, (Y + 0.5) / f - 0.5));
    const y0 = Math.floor(fy), y1 = Math.min(rows - 1, y0 + 1), ty = fy - y0;
    for (let X = 0; X < W; X++) {
      const fx = Math.min(cols - 1, Math.max(0, (X + 0.5) / f - 0.5));
      const x0 = Math.floor(fx), x1 = Math.min(cols - 1, x0 + 1), tx = fx - x0;
      const ks = [y0 * cols + x0, y0 * cols + x1, y1 * cols + x0, y1 * cols + x1];
      const ws = [(1 - tx) * (1 - ty), tx * (1 - ty), (1 - tx) * ty, tx * ty];
      const sums = [0, 0, 0];
      let land = 0, top = 0;
      for (let i = 0; i < 4; i++) {
        sums[cells.kind[ks[i]]] += ws[i];
        land += field[ks[i]] * ws[i];
        if (ws[i] > ws[top]) top = i;
      }
      let k = sums[UNKNOWN] > sums[LANDK] && sums[UNKNOWN] > sums[WATER] ? UNKNOWN : land >= 0.5 ? LANDK : WATER;
      // Aucune case de cette nature autour (terre et inconnu mêlés) : la case la plus proche.
      if (!ks.some((q) => cells.kind[q] === k)) k = cells.kind[ks[top]];
      let r = 0, g = 0, b = 0, total = 0, best = -1, st = 0, depth = 0, deep = 0;
      for (let i = 0; i < 4; i++) {
        if (cells.kind[ks[i]] !== k) continue;
        const w = ws[i] + 1e-6;
        const dk = cells.depth ? cells.depth[ks[i]] : NaN;
        if (k === WATER && !Number.isNaN(dk)) (depth += dk * w), (deep += w);
        r += cells.color[ks[i] * 3] * w;
        g += cells.color[ks[i] * 3 + 1] * w;
        b += cells.color[ks[i] * 3 + 2] * w;
        total += w;
        if (w > best && cells.state) [best, st] = [w, cells.state[ks[i]]];
      }
      // Terre : l'État dominant d'après les parts adoucies interpolées (frontières lisses).
      if (sf && k === LANDK) {
        const score = new Map<number, number>();
        for (let i = 0; i < 4; i++) {
          if (cells.kind[ks[i]] !== LANDK) continue;
          for (let j = 0; j < STATE_SLOTS; j++) {
            const id = sf.ids[ks[i] * STATE_SLOTS + j];
            if (id === 0xffff) break;
            score.set(id, (score.get(id) ?? 0) + sf.vals[ks[i] * STATE_SLOTS + j] * ws[i]);
          }
        }
        let top = -1;
        for (const [id, v] of score) if (v > top) [top, st] = [v, id];
      }
      const o = Y * W + X;
      kind[o] = k;
      state[o] = st;
      // Mer (pas un lac) : la couleur suit la distance interpolée.
      const band = deep > total / 2 ? oceanColor(depth / deep) : null;
      color[o * 3] = band ? band[0] : r / total;
      color[o * 3 + 1] = band ? band[1] : g / total;
      color[o * 3 + 2] = band ? band[2] : b / total;
    }
  }
  const borders = opts.states && !!cells.state;
  // Épaisseur du trait de côte (pixels) : ~1/4 de case.
  const line = Math.max(1, Math.round(f / 4));
  const data = new Uint8ClampedArray(W * H * 4);
  for (let Y = 0; Y < H; Y++) {
    for (let X = 0; X < W; X++) {
      const o = Y * W + X;
      let c: Rgb = [color[o * 3], color[o * 3 + 1], color[o * 3 + 2]];
      const near = [X > 0 ? o - 1 : -1, X < W - 1 ? o + 1 : -1, Y > 0 ? o - W : -1, Y < H - 1 ? o + W : -1];
      // Trait de côte : épaisseur proportionnelle à l'agrandissement (fin, mais visible de près).
      const coast = kind[o] === WATER && (near.some((i) => i >= 0 && kind[i] === LANDK) || (line > 1 && [
        X >= line ? o - line : -1, X < W - line ? o + line : -1, Y >= line ? o - line * W : -1, Y < H - line ? o + line * W : -1,
      ].some((i) => i >= 0 && kind[i] === LANDK)));
      if (coast) c = mix(c, COAST, 0.55);
      else if (borders && kind[o] === LANDK) {
        const other = near.find((i) => i >= 0 && kind[i] === LANDK && state[i] !== state[o]);
        // Frontière : un pixel, à la couleur assombrie de l'État (ou du voisin pour les neutres).
        if (other !== undefined) {
          const edge = cells.stateColors[state[o]] ?? cells.stateColors[state[other]] ?? [70, 70, 80];
          c = mix(c, scale(edge, 0.45), 0.7);
        }
      }
      put(data, o, opts.parchment ? parchment(c) : c);
    }
  }
  return { width: W, height: H, data };
}

/** Pixels par case : environ 3 600 pixels de large (net même zoomé), 10 au plus. */
export function upscaleFactor(cols: number): number {
  return Math.min(10, Math.max(1, Math.round(3600 / Math.max(1, cols))));
}

/** Rendu façon Azgaar : biomes (ou États façon carte politique), mers en bandes, lacs, côtes, relief. */
export function styleRaster(style: MapStyleView, terrain: Heightmap | null, opts: StyleOptions, factor = 1): Raster {
  const { cols, rows } = style;
  const n = cols * rows;
  const surface = decodeRuns(style.surface, n, SURFACE_HIDDEN);
  const states = opts.states && style.states ? decodeRuns(style.states, n) : null;
  const biomes = style.biome_colors.map(hexRgb);
  const stateColors = (style.state_colors ?? []).map(hexRgb);
  const cells: Cells = { cols, rows, color: new Float32Array(n * 3), kind: new Uint8Array(n), state: states, stateColors, depth: null };
  const depth = distanceToLand((k) => surface[k] < SURFACE_OCEAN, cols, rows);
  for (let r = 0; r < rows; r++) {
    for (let c = 0; c < cols; c++) {
      const k = r * cols + c;
      const v = surface[k];
      let color: Rgb;
      if (v === SURFACE_HIDDEN) {
        color = HIDDEN;
        cells.kind[k] = UNKNOWN;
      } else if (opts.landAsSea && v < SURFACE_OCEAN) {
        color = oceanColor(0);
        cells.kind[k] = WATER;
      } else if (v >= SURFACE_OCEAN) {
        color = v === SURFACE_LAKE ? LAKE : opts.flatSea ? oceanColor(Infinity) : oceanColor(depth[k]);
        if (v === SURFACE_LAKE) depth[k] = NaN;
        cells.kind[k] = WATER;
      } else {
        color = biomes[v] ?? biomes[4] ?? [200, 214, 143];
        if (states) {
          // Carte politique d'Azgaar : couleur de l'État sur un fond de biomes pâli.
          color = mix(color, WHITE, 0.35);
          const sc = stateColors[states[k]];
          if (states[k] > 0 && sc) color = mix(color, sc, 0.55);
        }
        color = scale(color, hillshade(terrain, (c + 0.5) / cols, (r + 0.5) / rows));
        cells.kind[k] = LANDK;
      }
      cells.color.set(color, k * 3);
    }
  }
  cells.depth = depth;
  return paint(cells, factor, opts);
}

/**
 * Repli sans données Azgaar (cartes générées ou calculées) : même palette, d'après le relief. Le
 * relief généré par le client (une colline par lieu) est adouci : ombrage et altitude réduits,
 * sinon chaque lieu serait un pic enneigé.
 */
export function reliefRaster(hm: Heightmap, opts: Pick<StyleOptions, 'parchment'> & { soft?: boolean }, factor = 1): Raster {
  const relief = opts.soft ? 0.5 : 1;
  const { cols, rows } = hm;
  const sea = LAND * 255;
  const n = cols * rows;
  const cells: Cells = { cols, rows, color: new Float32Array(n * 3), kind: new Uint8Array(n), state: null, stateColors: [], depth: null };
  const depth = distanceToLand((k) => (hm.heights[k] ?? 0) >= sea, cols, rows);
  for (let r = 0; r < rows; r++) {
    for (let c = 0; c < cols; c++) {
      const k = r * cols + c;
      const h = hm.heights[k] ?? 0;
      const land = h >= sea;
      // Distance à la côte rapportée à une grille de surface Azgaar (≈ 270 cases de large).
      depth[k] *= 270 / Math.max(1, cols);
      const v = LAND + (h / 255 - LAND) * relief;
      const color = land ? scale(biomeColor(v), hillshade(hm, (c + 0.5) / cols, (r + 0.5) / rows, relief * 0.6)) : oceanColor(depth[k]);
      cells.kind[k] = land ? LANDK : WATER;
      cells.color.set(color, k * 3);
    }
  }
  cells.depth = depth;
  return paint(cells, factor, { states: false, parchment: opts.parchment });
}

function scale(c: Rgb, f: number): Rgb {
  return [c[0] * f, c[1] * f, c[2] * f];
}

function put(data: Uint8ClampedArray, k: number, c: Rgb) {
  data[k * 4] = c[0];
  data[k * 4 + 1] = c[1];
  data[k * 4 + 2] = c[2];
  data[k * 4 + 3] = 255;
}

/** Lissage de Chaikin (extrémités conservées) : rivières aux méandres arrondis. */
export function chaikin(points: MapPoint[], iterations = 2): MapPoint[] {
  let pts = points;
  for (let it = 0; it < iterations && pts.length > 2; it++) {
    const next: MapPoint[] = [pts[0]];
    for (let i = 0; i + 1 < pts.length; i++) {
      const a = pts[i], b = pts[i + 1];
      next.push({ x: a.x * 0.75 + b.x * 0.25, y: a.y * 0.75 + b.y * 0.25 }, { x: a.x * 0.25 + b.x * 0.75, y: a.y * 0.25 + b.y * 0.75 });
    }
    next.push(pts[pts.length - 1]);
    pts = next;
  }
  return pts;
}

/**
 * Contour d'une rivière (chemin SVG fermé), qui s'élargit de la source à l'embouchure, comme
 * chez Azgaar. `minWidth` : largeur minimale (unités de carte), pour rester visible de loin.
 */
export function riverOutline(points: [number, number][], width: [number, number], minWidth = 0): string {
  const pts = chaikin(points.map(([x, y]) => ({ x, y })));
  if (pts.length < 2) return '';
  const acc = [0];
  for (let i = 1; i < pts.length; i++) acc.push(acc[i - 1] + Math.hypot(pts[i].x - pts[i - 1].x, pts[i].y - pts[i - 1].y));
  const total = acc[acc.length - 1] || 1;
  const left: MapPoint[] = [];
  const right: MapPoint[] = [];
  for (let i = 0; i < pts.length; i++) {
    const a = pts[Math.max(0, i - 1)], b = pts[Math.min(pts.length - 1, i + 1)];
    const len = Math.hypot(b.x - a.x, b.y - a.y) || 1;
    const nx = -(b.y - a.y) / len, ny = (b.x - a.x) / len;
    const t = acc[i] / total;
    const half = Math.max(minWidth, width[0] + (width[1] - width[0]) * t) / 2;
    left.push({ x: pts[i].x + nx * half, y: pts[i].y + ny * half });
    right.push({ x: pts[i].x - nx * half, y: pts[i].y - ny * half });
  }
  const f = (p: MapPoint) => `${p.x.toFixed(1)},${p.y.toFixed(1)}`;
  return `M${left.map(f).join('L')}L${right.reverse().map(f).join('L')}Z`;
}

/** Rayon de l'icône d'un lieu (pixels à l'écran), selon le bourg : capitale > ville > bourg > village. */
export function burgRadius(burg: BurgIcon | undefined): number {
  if (!burg) return 7;
  if (burg.capital) return 8.5;
  return [4.2, 5.5, 7][Math.min(2, Math.max(0, burg.size))];
}

/** Taille du nom (pixels à l'écran), selon le bourg. */
export function burgFont(burg: BurgIcon | undefined): number {
  if (!burg) return 13;
  if (burg.capital) return 15.5;
  return [11.5, 12.5, 14][Math.min(2, Math.max(0, burg.size))];
}

/** Ancre de port (chemin SVG centré sur l'origine, hauteur ≈ 2 × `s`). */
export function anchorPath(s: number): string {
  const f = (v: number) => v.toFixed(2);
  return [
    `M0,${f(-s)}V${f(s)}`,
    `M${f(-s * 0.55)},${f(-s * 0.45)}H${f(s * 0.55)}`,
    `M${f(-s * 0.85)},${f(s * 0.2)}Q${f(-s * 0.7)},${f(s)} 0,${f(s)}Q${f(s * 0.7)},${f(s)} ${f(s * 0.85)},${f(s * 0.2)}`,
  ].join('');
}

/** Lieu à étiqueter : position, nom, rayon de l'icône et taille du nom (unités de carte). */
export interface LabelCandidate {
  id: string;
  x: number;
  y: number;
  name: string;
  radius: number;
  font: number;
  /** Plus grand : placé d'abord (lieu actuel, destinations, capitales, villes…). */
  priority: number;
}

/**
 * Noms affichés sans chevauchement, comme Azgaar qui n'affiche pas tout à petite échelle : les
 * plus importants d'abord, puis tout nom qui ne recouvre aucun nom déjà placé.
 */
export function declutterLabels(candidates: LabelCandidate[]): Set<string> {
  const placed: { x0: number; y0: number; x1: number; y1: number }[] = [];
  const cell = Math.max(1e-6, ...candidates.map((c) => c.font * 8));
  const grid = new Map<string, number[]>();
  const keys = (b: { x0: number; y0: number; x1: number; y1: number }) => {
    const out: string[] = [];
    for (let gx = Math.floor(b.x0 / cell); gx <= Math.floor(b.x1 / cell); gx++) for (let gy = Math.floor(b.y0 / cell); gy <= Math.floor(b.y1 / cell); gy++) out.push(`${gx},${gy}`);
    return out;
  };
  const shown = new Set<string>();
  const sorted = [...candidates].sort((a, b) => b.priority - a.priority || (a.id < b.id ? -1 : 1));
  for (const c of sorted) {
    const w = c.name.length * c.font * 0.56;
    const yb = c.y - c.radius - c.font * 0.35;
    const box = { x0: c.x - w / 2, x1: c.x + w / 2, y0: yb - c.font, y1: yb + c.font * 0.15 };
    const ks = keys(box);
    const hit = ks.some((k) => (grid.get(k) ?? []).some((i) => {
      const o = placed[i];
      return box.x0 < o.x1 && box.x1 > o.x0 && box.y0 < o.y1 && box.y1 > o.y0;
    }));
    if (hit) continue;
    placed.push(box);
    for (const k of ks) grid.set(k, [...(grid.get(k) ?? []), placed.length - 1]);
    shown.add(c.id);
  }
  return shown;
}

/**
 * Bandes de la mer en vecteurs (courbes lisses, nettes de près) : régions à moins de 18, 10, 5 et
 * 2 cases de la terre, de la plus large à la plus étroite, aux couleurs d'Azgaar. Les cases
 * cachées (brouillard, hors zone) n'entrent dans aucune bande.
 */
export function seaLayers(style: MapStyleView, width: number, height: number, paper: boolean): { d: string; fill: string }[] {
  const { cols, rows } = style;
  const n = cols * rows;
  const surface = decodeRuns(style.surface, n, SURFACE_HIDDEN);
  const depth = distanceToLand((k) => surface[k] < SURFACE_OCEAN, cols, rows);
  const field = new Float32Array(n);
  for (let k = 0; k < n; k++) field[k] = surface[k] === SURFACE_HIDDEN ? -1e9 : -depth[k];
  const bands = OCEAN_BANDS.filter(([max]) => Number.isFinite(max)).reverse();
  const color = (c: Rgb) => {
    const v = paper ? parchment(c) : c;
    return `rgb(${Math.round(v[0])} ${Math.round(v[1])} ${Math.round(v[2])})`;
  };
  return isoBands(field, cols, rows, bands.map(([max]) => -max), width, height).map((b) => ({
    d: b.d,
    fill: color(bands.find(([max]) => -max === b.level)![1]),
  }));
}

/**
 * Ombrage du relief seul (noir et blanc translucides), `widthPx` pixels de large : posé sur les
 * biomes vectoriels, il redonne les montagnes. Le relief caché (brouillard) est plat : sans ombre.
 */
export function reliefShade(hm: Heightmap, widthPx: number, heightPx: number): Raster {
  const W = Math.max(1, Math.round(widthPx)), H = Math.max(1, Math.round(heightPx));
  const data = new Uint8ClampedArray(W * H * 4);
  for (let Y = 0; Y < H; Y++) {
    for (let X = 0; X < W; X++) {
      const s = hillshade(hm, (X + 0.5) / W, (Y + 0.5) / H);
      const o = (Y * W + X) * 4;
      if (s < 1) {
        data.set([25, 22, 35, Math.min(255, (1 - s) * 255 * 1.5)], o);
      } else {
        data.set([255, 252, 235, Math.min(255, (s - 1) * 255 * 1.3)], o);
      }
    }
  }
  return { width: W, height: H, data };
}
