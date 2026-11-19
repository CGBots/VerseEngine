import { describe, expect, it } from 'vitest';
import { backoffMs } from './realtime';

describe('backoffMs', () => {
  it('double à chaque tentative et plafonne à 30 s', () => {
    expect([0, 1, 2, 3, 4, 5, 6, 10].map(backoffMs)).toEqual([1000, 2000, 4000, 8000, 16000, 30000, 30000, 30000]);
  });
});
