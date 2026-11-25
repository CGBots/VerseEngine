import { describe, expect, it } from 'vitest';
import type { MapView } from '../api';
import { motionOf } from './motion';

const map = { places: [{ id: 'a', name: 'A', x: 0, y: 0 }, { id: 'b', name: 'B', x: 10, y: 0 }], roads: [] } as unknown as MapView;
const travel = { in_move: true, step_end_timestamp: null, eta_timestamp: null, remaining_km: 6, traveled_km: 4, speed_kmh: 5, final_step: false };

describe('motionOf', () => {
  it('places a group at its place, or on its road between both ends', () => {
    expect(motionOf(map, { kind: 'place', id: 'b', name: 'B', source_id: null, destination_id: null, destination_name: null }, null, 0)).toEqual({ kind: 'place', at: { x: 10, y: 0 } });
    const road = motionOf(map, { kind: 'road', id: 'r', name: 'R', source_id: 'a', destination_id: 'b', destination_name: 'B' }, travel, 100);
    expect(road).toMatchObject({ kind: 'road', traveledKm: 4, roadKm: 10, refTime: 100, moving: true });
  });
  it('gives nothing when a place is missing from the map', () => {
    expect(motionOf(map, { kind: 'place', id: 'zz', name: null, source_id: null, destination_id: null, destination_name: null }, null, 0)).toBeNull();
    expect(motionOf(map, null, null, 0)).toBeNull();
  });
});
