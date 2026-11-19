import { describe, expect, it } from 'vitest';
import { ServerClock, formatDuration } from './time';

describe('formatDuration', () => {
  it('formate les durées', () => {
    expect(formatDuration(9)).toBe('9 s');
    expect(formatDuration(65)).toBe('1 min 05 s');
    expect(formatDuration(3725)).toBe('1 h 02 min 05 s');
    expect(formatDuration(90061)).toBe('1 j 01 h 01 min');
    expect(formatDuration(-4)).toBe('0 s');
  });
});

describe('ServerClock', () => {
  it('corrige le décalage client/serveur', () => {
    const c = new ServerClock();
    // Le client a 30 s de retard sur le serveur.
    c.sync(1_000_030, 1_000_000_000);
    expect(c.now(1_000_000_000)).toBe(1_000_030);
    expect(c.remaining(1_000_090, 1_000_000_000)).toBe(60);
    expect(c.remaining(1_000_000, 1_000_000_000)).toBe(0);
  });
});
