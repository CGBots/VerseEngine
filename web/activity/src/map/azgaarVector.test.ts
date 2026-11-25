import { describe, expect, it } from 'vitest';
import type { MapStyleView } from '../api';
import { vectorLayers } from './azgaarVector';

const style = (vector: MapStyleView['vector']): MapStyleView => ({
  cols: 2, rows: 2, surface: '', biome_colors: ['#102030', '#ff0000'], state_colors: ['', '#00ff00', ''], rivers: [], exact: true, vector,
});
const full = { land: 'M0,0Z', lakes: 'M1,1Z', biomes: [[1, 'M2,2Z']] as [number, string][], states: [[1, 'M3,3Z'], [2, 'M4,4Z'], [0, 'M5,5Z']] as [number, string][], borders: 'M6,6L7,7' };
const opts = { states: false, parchment: false };

describe('vectorLayers', () => {
  it('returns null without vector data', () => {
    expect(vectorLayers(style(undefined), opts)).toBeNull();
  });

  it('takes biome colors from biome_colors', () => {
    const l = vectorLayers(style(full), opts)!;
    expect(l.biomes).toEqual([{ d: 'M2,2Z', fill: 'rgb(255,0,0)' }]);
    expect(l.landFill).toBe('rgb(255,0,0)');
    expect(l.states).toEqual([]);
  });

  it('adds state fills and pales biomes in political mode, skipping empty colors', () => {
    const l = vectorLayers(style(full), { ...opts, states: true })!;
    expect(l.states).toEqual([{ d: 'M3,3Z', fill: 'rgb(0,255,0)', opacity: 0.55 }]);
    expect(l.biomes[0].fill).not.toBe('rgb(255,0,0)');
    expect(l.biomes[0].fill).toBe('rgb(255,89,89)');
  });

  it('gives null layers for empty strings', () => {
    const l = vectorLayers(style({ land: '', lakes: '', biomes: [], states: [], borders: '' }), opts)!;
    expect(l.lakes).toBeNull();
    expect(l.coast).toBeNull();
    expect(l.borders).toBeNull();
    expect(l.biomes).toEqual([]);
  });

  it('draws lighter borders outside political mode', () => {
    const a = vectorLayers(style(full), opts)!.borders!.stroke;
    const b = vectorLayers(style(full), { ...opts, states: true })!.borders!.stroke;
    expect(a).not.toBe(b);
  });

  it('changes colors in parchment mode', () => {
    const a = vectorLayers(style(full), opts)!;
    const b = vectorLayers(style(full), { ...opts, parchment: true })!;
    expect(b.biomes[0].fill).not.toBe(a.biomes[0].fill);
    expect(b.coast!.stroke).not.toBe(a.coast!.stroke);
  });
});
