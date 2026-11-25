import { describe, expect, it } from 'vitest';
import { mapPath, type MapStyleView } from '../api';
import { anchorPath, burgFont, burgRadius, chaikin, declutterLabels, distanceToLand, landField, upscaleFactor, decodeRuns, hexRgb, hillshade, oceanColor, parchment, reliefRaster, riverOutline, seaCss, styleRaster, SURFACE_HIDDEN, SURFACE_LAKE, SURFACE_OCEAN } from './azgaarStyle';

/** Même encodage que `encode_runs` côté serveur. */
function runs(values: number[]): string {
  let raw = '';
  for (let i = 0; i < values.length; ) {
    let n = 1;
    while (i + n < values.length && values[i + n] === values[i] && n < 255) n++;
    raw += String.fromCharCode(values[i], n);
    i += n;
  }
  return btoa(raw);
}

const px = (r: { width: number; data: Uint8ClampedArray }, c: number, row: number) => {
  const k = (row * r.width + c) * 4;
  return [r.data[k], r.data[k + 1], r.data[k + 2]];
};

describe('decodeRuns', () => {
  it('relit les suites du serveur (« BAI= » = 4 répété 2 fois)', () => {
    expect([...decodeRuns('BAI=', 2)]).toEqual([4, 4]);
    const v = [...Array(300).fill(4), 201, 201, 7];
    expect([...decodeRuns(runs(v), v.length)]).toEqual(v);
  });
  it('complète une grille trop courte et ignore un texte illisible', () => {
    expect([...decodeRuns('BAI=', 4, 9)]).toEqual([4, 4, 9, 9]);
    expect([...decodeRuns('%%%', 2, 1)]).toEqual([1, 1]);
    expect([...decodeRuns(undefined, 2)]).toEqual([0, 0]);
  });
});

describe('couleurs', () => {
  it('lit seulement les couleurs #rrggbb', () => {
    expect(hexRgb('#466eab')).toEqual([0x46, 0x6e, 0xab]);
    expect(hexRgb('url(#x)')).toBeNull();
    expect(hexRgb(undefined)).toBeNull();
  });
  it('la mer fonce vers le large', () => {
    const lum = (c: number[]) => c[0] + c[1] + c[2];
    expect(lum(oceanColor(1))).toBeGreaterThan(lum(oceanColor(5)));
    expect(lum(oceanColor(5))).toBeGreaterThan(lum(oceanColor(9)));
  });
  it('la marge autour de la carte a la couleur du large', () => {
    expect(seaCss(false)).toBe('rgb(98,128,182)');
    expect(seaCss(true)).not.toBe(seaCss(false));
  });
  it('le parchemin tire vers le sépia', () => {
    const [r, , b] = parchment([50, 84, 138]);
    expect(r).toBeGreaterThan(b);
  });
});

describe('styleRaster', () => {
  // 4 × 2 : mer côtière, terre (biome 1, État 1), terre (biome 2, État 2), lac ; puis large et inconnu.
  const style: MapStyleView = {
    cols: 4, rows: 2,
    surface: runs([SURFACE_OCEAN + 1, 1, 2, SURFACE_LAKE, SURFACE_OCEAN + 9, SURFACE_OCEAN + 9, SURFACE_HIDDEN, SURFACE_HIDDEN]),
    states: runs([0, 1, 2, 0, 0, 0, 0, 0]),
    biome_colors: ['#466eab', '#ff0000', '#00ff00'],
    state_colors: ['', '#0000ff', '#ffff00'],
    rivers: [], exact: true,
  };
  it('peint biomes, mers, lacs et cases inconnues', () => {
    const r = styleRaster(style, null, { states: false, parchment: false });
    expect([r.width, r.height]).toEqual([4, 2]);
    expect(px(r, 1, 0)).toEqual([255, 0, 0]);
    expect(px(r, 2, 0)).toEqual([0, 255, 0]);
    expect(px(r, 2, 1)).toEqual([143, 152, 164]);
    // Mer : bande de la côte (à une diagonale de la terre), sans trait de côte (aucune terre voisine).
    expect(px(r, 0, 1)).toEqual(oceanColor(4 / 3));
    expect(r.data.every((v, i) => i % 4 !== 3 || v === 255)).toBe(true);
  });
  it('teinte les États et marque leur frontière', () => {
    const plain = styleRaster(style, null, { states: false, parchment: false });
    const political = styleRaster(style, null, { states: true, parchment: false });
    expect(px(political, 1, 0)).not.toEqual(px(plain, 1, 0));
    expect(px(political, 1, 0)[2]).toBeGreaterThan(0);
  });
  it('agrandi, la côte reste nette : chaque pixel est terre ou eau, sans mélange', () => {
    const r = styleRaster(style, null, { states: false, parchment: false }, 3);
    expect([r.width, r.height]).toEqual([12, 6]);
    // Centre de la case de terre rouge, et la case voisine verte : couleurs intactes.
    expect(px(r, 4, 1)).toEqual([255, 0, 0]);
    expect(px(r, 7, 1)).toEqual([0, 255, 0]);
    // Chaque pixel est terre (rouge et vert mêlés, sans bleu) ou eau : jamais un mélange des deux.
    for (let y = 0; y < 6; y++) for (let x = 0; x < 12; x++) {
      const b = px(r, x, y)[2];
      expect(b === 0 || b > 100, `pixel ${x},${y}`).toBe(true);
    }
    expect(upscaleFactor(267)).toBe(10);
    expect(upscaleFactor(480)).toBe(8);
  });
  it('ombre le relief : versant au soleil plus clair que le versant opposé', () => {
    // Colline au centre d'une grille 9 × 9.
    const hm = { cols: 9, rows: 9, heights: Array.from({ length: 81 }, (_, i) => { const c = i % 9, r = Math.floor(i / 9); return Math.max(60, 250 - 40 * Math.hypot(c - 4, r - 4)); }) };
    expect(hillshade(hm, 0.3, 0.3)).toBeGreaterThan(1);
    expect(hillshade(hm, 0.7, 0.7)).toBeLessThan(1);
    expect(hillshade(null, 0.5, 0.5)).toBe(1);
  });
});

describe('distanceToLand', () => {
  it('distance en cases à la terre la plus proche, diagonale comprise', () => {
    const d = distanceToLand((k) => k === 0, 5, 2);
    expect([...d.slice(0, 5)]).toEqual([0, 1, 2, 3, 4]);
    expect(d[5]).toBe(1);
    expect(d[6]).toBeCloseTo(4 / 3);
  });
});

describe('reliefRaster (cartes sans données Azgaar)', () => {
  it('mer sous le niveau de la terre, terre au-dessus', () => {
    const r = reliefRaster({ cols: 2, rows: 1, heights: [10, 200] }, { parchment: false });
    const [sea, land] = [px(r, 0, 0), px(r, 1, 0)];
    expect(sea[2]).toBeGreaterThan(sea[0]);
    expect(land[1]).toBeGreaterThan(land[2] - 40);
  });
});

describe('rivières', () => {
  it('le lissage garde les extrémités', () => {
    const pts = chaikin([{ x: 0, y: 0 }, { x: 10, y: 0 }, { x: 10, y: 10 }]);
    expect(pts[0]).toEqual({ x: 0, y: 0 });
    expect(pts[pts.length - 1]).toEqual({ x: 10, y: 10 });
    expect(pts.length).toBeGreaterThan(3);
  });
  it('le contour s’élargit vers l’embouchure', () => {
    const d = riverOutline([[0, 0], [100, 0]], [1, 4]);
    const nums = d.match(/-?\d+(\.\d+)?/g)!.map(Number);
    const ys = nums.filter((_, i) => i % 2 === 1);
    // Bord gauche : de +0,5 à +2 ; bord droit : de −2 à −0,5.
    expect(Math.max(...ys)).toBeCloseTo(2, 1);
    expect(Math.min(...ys)).toBeCloseTo(-2, 1);
    expect(d.startsWith('M0.0,0.5')).toBe(true);
    expect(riverOutline([[0, 0]], [1, 2])).toBe('');
  });
});

describe('icônes des bourgs', () => {
  it('capitale > ville > bourg > village', () => {
    const sizes = [{ size: 0, capital: false, port: false }, { size: 1, capital: false, port: false }, { size: 2, capital: false, port: false }, { size: 0, capital: true, port: false }];
    const radii = sizes.map(burgRadius);
    expect([...radii].sort((a, b) => a - b)).toEqual(radii);
    expect(sizes.map(burgFont)).toEqual([...sizes.map(burgFont)].sort((a, b) => a - b));
    expect(burgRadius(undefined)).toBe(7);
    expect(anchorPath(4)).toMatch(/^M0,-4/);
  });
});

describe('mapPath', () => {
  it('demande la vue admin seulement en mode admin', () => {
    expect(mapPath(false)).toBe('/map');
    expect(mapPath(true)).toBe('/map?view=admin');
  });
});

describe('declutterLabels', () => {
  const c = (id: string, x: number, priority: number) => ({ id, x, y: 100, name: 'Abcdef', radius: 5, font: 10, priority });
  it('garde le plus important de deux noms qui se chevauchent', () => {
    expect([...declutterLabels([c('a', 100, 1), c('b', 110, 5)])]).toEqual(['b']);
  });
  it('garde tous les noms éloignés', () => {
    expect(declutterLabels([c('a', 100, 1), c('b', 400, 1), c('c', 800, 0)]).size).toBe(3);
  });
});

describe('landField', () => {
  it('arrondit les côtes sans effacer un îlot', () => {
    const kind = new Uint8Array(9);
    kind[4] = 1; // îlot d'une case au centre
    const f = landField(kind, 3, 3);
    expect(f[4]).toBeGreaterThanOrEqual(0.6);
    expect(f[0]).toBeLessThan(0.5);
    const full = landField(new Uint8Array(9).fill(1), 3, 3);
    expect([...full].every((v) => v === 1)).toBe(true);
  });
});

describe('landAsSea', () => {
  it('paints land cells like the coastal sea, without coastline', async () => {
    const { styleRaster, SURFACE_OCEAN, oceanColor } = await import('./azgaarStyle');
    const runs = (v: number[]) => btoa(String.fromCharCode(...v.flatMap((x) => [x, 1])));
    const style = { cols: 2, rows: 1, surface: runs([4, SURFACE_OCEAN + 1]), biome_colors: Array(13).fill('#00ff00'), rivers: [], exact: true } as never;
    const r = styleRaster(style, null, { states: false, parchment: false, landAsSea: true });
    const sea = oceanColor(0).map(Math.round);
    expect([r.data[0], r.data[1], r.data[2]]).toEqual(sea);
  });
});
