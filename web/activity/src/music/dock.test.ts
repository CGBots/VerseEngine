import { describe, expect, it } from 'vitest';
import { clampDock, DOCK_MARGIN, snapDock } from './dock';

describe('music dock', () => {
  it('stays inside the map', () => {
    expect(clampDock({ x: -50, y: 900 }, 200, 100, 800, 600)).toEqual({ x: DOCK_MARGIN, y: 600 - 100 - DOCK_MARGIN });
  });
  it('sticks to the nearest edges when released close to them', () => {
    expect(snapDock({ x: 30, y: 300 }, 200, 100, 800, 600)).toEqual({ x: DOCK_MARGIN, y: 300 });
    expect(snapDock({ x: 570, y: 470 }, 200, 100, 800, 600)).toEqual({ x: 592, y: 492 });
    expect(snapDock({ x: 300, y: 250 }, 200, 100, 800, 600)).toEqual({ x: 300, y: 250 });
  });
});
