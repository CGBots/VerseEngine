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

/**
 * Masque de la brume dessiné dans un canevas 2D : noir = brume, blanc = dégagé, bords adoucis.
 * `scale` : pixels du canevas par unité de carte.
 */
export function drawFogMask(ctx: CanvasRenderingContext2D, shapes: FogShapes, scale: number, width: number, height: number): void {
  ctx.fillStyle = '#000';
  ctx.fillRect(0, 0, width, height);
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
}
