import { describe, expect, it } from 'vitest';
import { PULL_THRESHOLD, pullDecision } from './sheet';

describe('pullDecision', () => {
  it('ouvre la fiche quand on tire vers le bas', () => {
    expect(pullDecision(10, 10 + PULL_THRESHOLD, false)).toBe('open');
    expect(pullDecision(10, 10 + PULL_THRESHOLD - 1, false)).toBeNull();
  });
  it('la ferme quand on la remonte', () => {
    expect(pullDecision(300, 300 - PULL_THRESHOLD, true)).toBe('close');
    expect(pullDecision(300, 290, true)).toBeNull();
  });
  it('ignore le geste dans le mauvais sens', () => {
    expect(pullDecision(10, 200, true)).toBeNull();
    expect(pullDecision(300, 0, false)).toBeNull();
  });
});
