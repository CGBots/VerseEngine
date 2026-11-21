import type { MapView } from '../api';
import { roadPath, type MapPoint } from './geometry';

/** Zone dégagée de la brume : disque autour d'un lieu. */
export interface FogCircle {
  x: number;
  y: number;
  r: number;
}

/** Ce qu'il faut dégager de la brume, dans le repère de la carte. */
export interface FogShapes {
  circles: FogCircle[];
  /** Couloirs le long des routes empruntées. */
  corridors: { points: MapPoint[]; width: number }[];
}

/** Rayon d'une destination entrevue, relatif au rayon d'un lieu découvert. */
export const GLIMPSE = 0.45;

/** Zones dégagées : lieux découverts (rayon plein), destinations entrevues (rayon réduit),
 * couloirs des routes empruntées. `null` : pas de brouillard. */
export function fogShapes(map: MapView): FogShapes | null {
  const fog = map.fog;
  if (!fog) return null;
  const pos = new Map(map.places.map((p) => [p.id, p]));
  const circles: FogCircle[] = [];
  for (const id of fog.discovered) {
    const p = pos.get(id);
    if (p) circles.push({ x: p.x, y: p.y, r: fog.radius });
  }
  for (const id of fog.glimpsed) {
    const p = pos.get(id);
    if (p) circles.push({ x: p.x, y: p.y, r: fog.radius * GLIMPSE });
  }
  const known = new Set(fog.roads);
  const corridors = map.roads
    .filter((r) => known.has(r.id))
    .map((r) => ({ points: roadPath(map, r.from, r.to), width: fog.radius * 0.9 }))
    .filter((c) => c.points.length >= 2);
  return { circles, corridors };
}

/** Rectangle dans le repère de la carte. */
export interface MapRect {
  x: number;
  y: number;
  w: number;
  h: number;
}

/**
 * Rectangle qui contient tout ce qui est dégagé, avec une marge de deux rayons, et au moins huit
 * rayons de côté. Sert au cadrage de départ (sinon, sur une grande carte Azgaar, la zone découverte
 * n'est qu'un point) et à la zone couverte par le masque 3D. `null` : rien de dégagé.
 */
export function fogFocus(map: MapView, shapes: FogShapes | null = fogShapes(map)): MapRect | null {
  if (!shapes || !map.fog) return null;
  const xs: number[] = [];
  const ys: number[] = [];
  for (const c of shapes.circles) xs.push(c.x - c.r, c.x + c.r), ys.push(c.y - c.r, c.y + c.r);
  for (const c of shapes.corridors) for (const p of c.points) xs.push(p.x), ys.push(p.y);
  if (!xs.length) return null;
  const r = map.fog.radius;
  const pad = 2 * r;
  const min = 8 * r;
  let x0 = Math.min(...xs) - pad, x1 = Math.max(...xs) + pad;
  let y0 = Math.min(...ys) - pad, y1 = Math.max(...ys) + pad;
  if (x1 - x0 < min) [x0, x1] = [(x0 + x1 - min) / 2, (x0 + x1 + min) / 2];
  if (y1 - y0 < min) [y0, y1] = [(y0 + y1 - min) / 2, (y0 + y1 + min) / 2];
  return { x: x0, y: y0, w: x1 - x0, h: y1 - y0 };
}

/** Agrandit `rect` aux proportions de la vue (`viewW` × `viewH`), autour de son centre. */
export function fitRect(rect: MapRect, viewW: number, viewH: number): MapRect {
  const aspect = viewW > 0 && viewH > 0 ? viewW / viewH : rect.w / rect.h;
  let { w, h } = rect;
  if (w / h < aspect) w = h * aspect;
  else h = w / aspect;
  return { x: rect.x + rect.w / 2 - w / 2, y: rect.y + rect.h / 2 - h / 2, w, h };
}

/**
 * Masque de la brume dessiné dans un canevas 2D : noir = brume, blanc = dégagé, bords adoucis.
 * `scale` : pixels du canevas par unité de carte ; `origin` : point de la carte au coin du canevas.
 */
export function drawFogMask(ctx: CanvasRenderingContext2D, shapes: FogShapes, scale: number, width: number, height: number, origin: { x: number; y: number } = { x: 0, y: 0 }): void {
  ctx.fillStyle = '#000';
  ctx.fillRect(0, 0, width, height);
  ctx.save();
  ctx.translate(-origin.x * scale, -origin.y * scale);
  for (const c of shapes.circles) {
    const g = ctx.createRadialGradient(c.x * scale, c.y * scale, 0, c.x * scale, c.y * scale, c.r * scale);
    g.addColorStop(0, 'rgba(255,255,255,1)');
    g.addColorStop(0.55, 'rgba(255,255,255,0.9)');
    g.addColorStop(1, 'rgba(255,255,255,0)');
    ctx.fillStyle = g;
    ctx.beginPath();
    ctx.arc(c.x * scale, c.y * scale, c.r * scale, 0, Math.PI * 2);
    ctx.fill();
  }
  ctx.strokeStyle = 'rgba(255,255,255,0.85)';
  ctx.lineCap = 'round';
  ctx.lineJoin = 'round';
  for (const c of shapes.corridors) {
    ctx.lineWidth = c.width * scale;
    ctx.beginPath();
    c.points.forEach((p, i) => (i ? ctx.lineTo(p.x * scale, p.y * scale) : ctx.moveTo(p.x * scale, p.y * scale)));
    ctx.stroke();
  }
  ctx.restore();
}

function distToSegment(px: number, py: number, a: MapPoint, b: MapPoint): number {
  const dx = b.x - a.x, dy = b.y - a.y;
  const len2 = dx * dx + dy * dy;
  const t = len2 === 0 ? 0 : Math.min(1, Math.max(0, ((px - a.x) * dx + (py - a.y) * dy) / len2));
  return Math.hypot(px - a.x - t * dx, py - a.y - t * dy);
}

/**
 * Relief aplani sous la brume (3D) : la hauteur est gardée dans les zones dégagées, puis descend
 * au niveau de la mer sur une demi-largeur de zone. La brume peut alors rester au ras de la mer,
 * sans décalage de parallaxe entre le trou de la brume et les lieux, et les collines découvertes
 * en émergent. Le serveur efface déjà le relief lointain ; ceci couvre aussi le relief généré par
 * le client (cartes sans heightmap). `heights` : 0–255, une valeur par cellule (centre de cellule).
 */
export function flattenUnderFog(heights: number[], cols: number, rows: number, width: number, height: number, shapes: FogShapes, sea: number): number[] {
  const radius = Math.max(1e-9, ...shapes.circles.map((c) => c.r), ...shapes.corridors.map((c) => c.width));
  const fade = radius * 0.5;
  return heights.map((h, i) => {
    if (h <= sea) return h;
    const c = i % cols, r = Math.floor(i / cols);
    const x = ((c + 0.5) / cols) * width, y = ((r + 0.5) / rows) * height;
    // Distance au bord de la zone dégagée la plus proche (négative à l'intérieur).
    let d = Infinity;
    for (const k of shapes.circles) d = Math.min(d, Math.hypot(x - k.x, y - k.y) - k.r);
    for (const k of shapes.corridors) {
      for (let j = 1; j < k.points.length; j++) d = Math.min(d, distToSegment(x, y, k.points[j - 1], k.points[j]) - k.width / 2);
    }
    const keep = Math.min(1, Math.max(0, 1 - d / fade));
    return Math.round(sea + (h - sea) * keep * keep * (3 - 2 * keep));
  });
}
