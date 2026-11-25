import { describe, expect, it, vi } from 'vitest';
import { backoffMs, coalesce, type ServerEvent } from './realtime';

describe('coalesce', () => {
  it("transmet une rafale en un seul lot, sans doublon", () => {
    vi.useFakeTimers();
    const batches: ServerEvent[][] = [];
    const c = coalesce((b) => batches.push(b), 400);
    for (let i = 0; i < 200; i++) c.push(i % 2 ? 'map_updated' : 'travel_updated');
    expect(batches).toEqual([]);
    vi.advanceTimersByTime(400);
    expect(batches).toEqual([['travel_updated', 'map_updated']]);
    c.push('character_updated');
    vi.advanceTimersByTime(400);
    expect(batches[1]).toEqual(['character_updated']);
    vi.useRealTimers();
  });

  it('resync remplace les autres types du lot', () => {
    vi.useFakeTimers();
    const batches: ServerEvent[][] = [];
    const c = coalesce((b) => batches.push(b), 400);
    c.push('map_updated');
    c.push('resync');
    c.push('travel_updated');
    vi.advanceTimersByTime(400);
    expect(batches).toEqual([['resync']]);
    vi.useRealTimers();
  });

  it("cancel abandonne le lot en attente", () => {
    vi.useFakeTimers();
    const batches: ServerEvent[][] = [];
    const c = coalesce((b) => batches.push(b), 400);
    c.push('map_updated');
    c.cancel();
    vi.advanceTimersByTime(1000);
    expect(batches).toEqual([]);
    vi.useRealTimers();
  });
});

describe('backoffMs', () => {
  it('double à chaque tentative et plafonne à 30 s', () => {
    expect([0, 1, 2, 3, 4, 5, 6, 10].map(backoffMs)).toEqual([1000, 2000, 4000, 8000, 16000, 30000, 30000, 30000]);
  });
});
