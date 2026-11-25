import { describe, expect, it } from 'vitest';
import { formatModifier, isNegative } from './CharacterPanel';

describe('isNegative', () => {
  it('repère les effets qui réduisent une statistique', () => {
    expect(isNegative('multiplier', 0.8)).toBe(true);
    expect(isNegative('multiplier', 1.15)).toBe(false);
    expect(isNegative('flat', -2)).toBe(true);
    expect(isNegative('flat', 3)).toBe(false);
    expect(isNegative('base', 'texte')).toBe(false);
  });
});

describe('formatModifier', () => {
  it('affiche un multiplicateur ou un ajout signé', () => {
    expect(formatModifier('multiplier', 1.5)).toBe('×1.50');
    expect(formatModifier('flat', 3)).toBe('+3');
    expect(formatModifier('flat', -2)).toBe('-2');
    expect(formatModifier('base', 0)).toBe('+0');
  });
});
