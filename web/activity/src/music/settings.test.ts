import { describe, expect, it } from 'vitest';
import { DEFAULT_SETTINGS, effectiveVolume, parseSettings } from './settings';

describe('music settings', () => {
  it('falls back to defaults on missing or broken values', () => {
    expect(parseSettings(null)).toEqual(DEFAULT_SETTINGS);
    expect(parseSettings('{oops')).toEqual(DEFAULT_SETTINGS);
    expect(parseSettings('{"volume": "loud"}')).toEqual(DEFAULT_SETTINGS);
  });
  it('keeps valid values and bounds the volume', () => {
    expect(parseSettings('{"volume":0.3,"muted":true,"videoHidden":true}')).toEqual({ volume: 0.3, muted: true, videoHidden: true });
    expect(parseSettings('{"volume":7}').volume).toBe(1);
    expect(parseSettings('{"volume":-1}').volume).toBe(0);
  });
  it('applies mute and fade', () => {
    expect(effectiveVolume({ volume: 1, muted: false, videoHidden: false }, 1)).toBe(1);
    expect(effectiveVolume({ volume: 0.5, muted: false, videoHidden: false }, 1)).toBeCloseTo(0.25);
    expect(effectiveVolume({ volume: 1, muted: false, videoHidden: false }, 0.5)).toBeCloseTo(0.5);
    expect(effectiveVolume({ volume: 1, muted: true, videoHidden: false }, 1)).toBe(0);
  });
});
