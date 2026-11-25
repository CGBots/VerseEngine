import { describe, expect, it } from 'vitest';
import { layoutFor } from './layout';

describe('layoutFor', () => {
  it('keeps the character column where there is room', () => {
    expect(layoutFor(1280, 800)).toBe('wide'); // ordinateur
    expect(layoutFor(1000, 640)).toBe('wide'); // fenêtre de Discord sur PC
    expect(layoutFor(1028, 876)).toBe('wide'); // Z Fold 6 déplié, paysage
    expect(layoutFor(1024, 768)).toBe('wide'); // tablette, paysage
  });

  it('uses the drawer elsewhere', () => {
    expect(layoutFor(928, 1004)).toBe('compact'); // Z Fold 6 déplié, portrait
    expect(layoutFor(484, 1112)).toBe('compact'); // Z Fold 6 plié
    expect(layoutFor(1136, 432)).toBe('compact'); // Z Fold 6 plié, paysage : trop bas
    expect(layoutFor(430, 739)).toBe('compact'); // téléphone
    expect(layoutFor(768, 1024)).toBe('compact'); // tablette, portrait
  });
});
