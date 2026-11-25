import { describe, expect, it } from 'vitest';
import { isoBands } from './seaBands';

const nums = (d: string) => (d.match(/-?\d+(\.\d+)?/g) ?? []).map(Number);
const count = (d: string, ch: string) => d.split(ch).length - 1;

describe('isoBands', () => {
  it('gives one closed loop around a single high cell', () => {
    const f = new Array(25).fill(0);
    f[12] = 1;
    const res = isoBands(f, 5, 5, [0.5], 50, 50);
    expect(res).toHaveLength(1);
    const d = res[0].d;
    expect(d.startsWith('M')).toBe(true);
    expect(d.endsWith('Z')).toBe(true);
    expect(count(d, 'M')).toBe(1);
    const n = nums(d);
    for (let i = 0; i < n.length; i++) expect(Math.abs(n[i] - 25)).toBeLessThanOrEqual(10);
  });

  it('gives two loops for two separate blobs', () => {
    const f = new Array(7 * 3).fill(0);
    f[1 * 7 + 1] = 1;
    f[1 * 7 + 5] = 1;
    const res = isoBands(f, 7, 3, [0.5], 70, 30);
    expect(count(res[0].d, 'M')).toBe(2);
  });

  it('gives an outer loop and a hole for a ring', () => {
    const n = 7;
    const f = new Array(n * n).fill(0);
    for (let r = 1; r <= 5; r++) for (let c = 1; c <= 5; c++) f[r * n + c] = 1;
    f[3 * n + 3] = 0;
    const res = isoBands(f, n, n, [0.5], 70, 70);
    expect(count(res[0].d, 'M')).toBe(2);
  });

  it('keeps points inside the map for a region touching the border', () => {
    const f = new Array(16).fill(0);
    for (const i of [0, 1, 4, 5]) f[i] = 1;
    const res = isoBands(f, 4, 4, [0.5], 40, 20);
    expect(res).toHaveLength(1);
    const n = nums(res[0].d);
    for (let i = 0; i < n.length; i += 2) {
      expect(n[i]).toBeGreaterThanOrEqual(0);
      expect(n[i]).toBeLessThanOrEqual(40);
      expect(n[i + 1]).toBeGreaterThanOrEqual(0);
      expect(n[i + 1]).toBeLessThanOrEqual(20);
    }
  });

  it('omits empty levels and follows the order of levels', () => {
    const f = new Array(25).fill(0);
    f[12] = 3;
    const res = isoBands(f, 5, 5, [2, 10, 1], 50, 50);
    expect(res.map((r) => r.level)).toEqual([2, 1]);
  });

  it('handles a 480x170 field with 6 levels quickly', () => {
    const cols = 480, rows = 170;
    const f = new Float32Array(cols * rows);
    for (let r = 0; r < rows; r++) {
      for (let c = 0; c < cols; c++) {
        f[r * cols + c] = 3.5 + 1.5 * Math.sin(c * 0.05) * Math.cos(r * 0.07) + Math.sin((c + r) * 0.11);
      }
    }
    const t0 = performance.now();
    const res = isoBands(f, cols, rows, [1, 2, 3, 4, 5, 6], 960, 340);
    expect(performance.now() - t0).toBeLessThan(500);
    expect(res.length).toBeGreaterThan(0);
  });
});
