import { describe, expect, it } from 'vitest';
import { isNegative } from './CharacterPanel';

describe('isNegative', () => {
  it('repère les effets qui réduisent une statistique', () => {
    expect(isNegative('multiplier', 0.8)).toBe(true);
    expect(isNegative('multiplier', 1.15)).toBe(false);
    expect(isNegative('flat', -2)).toBe(true);
    expect(isNegative('flat', 3)).toBe(false);
    expect(isNegative('base', 'texte')).toBe(false);
  });
});
