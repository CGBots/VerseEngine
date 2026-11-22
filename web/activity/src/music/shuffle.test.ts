import { describe, expect, it } from 'vitest';
import { ShuffleQueue } from './shuffle';

/** Générateur déterministe pour les tests. */
function seeded(seed: number) {
  let s = seed;
  return () => {
    s = (s * 1103515245 + 12345) % 2 ** 31;
    return s / 2 ** 31;
  };
}

describe('ShuffleQueue', () => {
  it('plays every track once per round, in a shuffled order', () => {
    const q = new ShuffleQueue(seeded(7));
    q.setTracks(['a', 'b', 'c', 'd']);
    for (let round = 0; round < 5; round++) {
      const played = [q.next(), q.next(), q.next(), q.next()];
      expect([...played].sort()).toEqual(['a', 'b', 'c', 'd']);
    }
  });

  it('never repeats a track across the boundary between two rounds', () => {
    for (let seed = 1; seed < 200; seed++) {
      const q = new ShuffleQueue(seeded(seed));
      q.setTracks(['a', 'b', 'c']);
      let previous = q.next();
      for (let i = 0; i < 30; i++) {
        const n = q.next();
        expect(n).not.toBe(previous);
        previous = n;
      }
    }
  });

  it('loops on a single track and is silent when empty', () => {
    const q = new ShuffleQueue();
    expect(q.next()).toBeNull();
    q.setTracks(['solo']);
    expect([q.next(), q.next(), q.next()]).toEqual(['solo', 'solo', 'solo']);
  });

  it('follows library changes without restarting the round', () => {
    const q = new ShuffleQueue(seeded(3));
    q.setTracks(['a', 'b', 'c']);
    const first = q.next()!;
    const rest = ['a', 'b', 'c'].filter((x) => x !== first);
    q.setTracks([first, rest[0], 'd']); // rest[1] retiré, d ajouté
    const round = [q.next(), q.next()];
    expect([...round].sort()).toEqual([rest[0], 'd'].sort());
    expect(q.has(rest[1])).toBe(false);
    expect(q.size).toBe(3);
  });

  it('peeks at the next track without consuming it', () => {
    const q = new ShuffleQueue(seeded(5));
    q.setTracks(['a', 'b']);
    q.next();
    const peeked = q.peek();
    expect(q.next()).toBe(peeked);
  });
});
