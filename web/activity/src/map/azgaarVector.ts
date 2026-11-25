import type { MapStyleView } from '../api';
import { COAST, LAKE, WHITE, hexRgb, mix, parchment, type Rgb } from './azgaarStyle';

/** Ce que le composant dessine par-dessus le raster, d'après les contours vectoriels du serveur. */
export interface VectorLayers {
  /** Couleur sous les biomes (cache les interstices entre la côte lissée et les polygones bruts). */
  landFill: string;
  biomes: { d: string; fill: string }[];
  states: { d: string; fill: string; opacity: number }[];
  lakes: { d: string; fill: string; stroke: string } | null;
  coast: { d: string; stroke: string } | null;
  borders: { d: string; stroke: string } | null;
  /** Chemin de la terre (sert de découpe). */
  land: string;
}

const DEFAULT_BIOME: Rgb = [200, 214, 143];
const BORDER: Rgb = [70, 70, 80];
const STATE_OPACITY = 0.55;

const css = (c: Rgb) => `rgb(${c.map((v) => Math.round(v)).join(',')})`;

export function vectorLayers(style: MapStyleView, opts: { states: boolean; parchment: boolean }): VectorLayers | null {
  const v = style.vector;
  if (!v) return null;
  const tone = (c: Rgb) => css(opts.parchment ? parchment(c) : c);
  // Mode politique : biomes pâlis, États par-dessus.
  const biome = (id: number): Rgb => {
    const c = hexRgb(style.biome_colors[id]) ?? DEFAULT_BIOME;
    return opts.states ? mix(c, WHITE, 0.35) : c;
  };
  const biomes = v.biomes.filter(([, d]) => !!d).map(([id, d]) => ({ d, fill: tone(biome(id)) }));
  const states = opts.states
    ? v.states.flatMap(([id, d]) => {
        const c = hexRgb(style.state_colors?.[id]);
        return d && c ? [{ d, fill: tone(c), opacity: STATE_OPACITY }] : [];
      })
    : [];
  return {
    land: v.land,
    landFill: tone(v.biomes.length ? biome(v.biomes[0][0]) : DEFAULT_BIOME),
    biomes,
    states,
    lakes: v.lakes ? { d: v.lakes, fill: tone(LAKE), stroke: tone(mix(LAKE, COAST, 0.4)) } : null,
    coast: v.land ? { d: v.land, stroke: tone(mix(COAST, WHITE, 0.15)) } : null,
    // Frontières plus discrètes hors mode politique.
    borders: v.borders ? { d: v.borders, stroke: tone(opts.states ? BORDER : mix(BORDER, WHITE, 0.5)) } : null,
  };
}
