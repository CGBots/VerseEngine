import { describe, expect, it } from 'vitest';
import type { GeneratedMap } from '../api';
import { addPlace, addRoad, bezier, deletePlace, deleteRoad, mapProblem, renamePlace, reshapeRoad, roadKm, roadMiddle, toMapView } from './generateModel';

const base = (): GeneratedMap => ({
  seed: 1, width: 1000, height: 1000, km_per_unit: 0.1,
  places: [
    { id: 1, name: 'A', x: 0, y: 0, capital: true },
    { id: 2, name: 'B', x: 100, y: 0, capital: false },
    { id: 3, name: 'C', x: 100, y: 100, capital: false },
  ],
  roads: [{ a: 1, b: 2, control: [50, 0] }, { a: 2, b: 3, control: [100, 50] }],
});

describe('generated map editing', () => {
  it('follows the same curve as the server', () => {
    const p = bezier([0, 0], [10, 0], [5, 4]);
    expect(p).toHaveLength(12);
    expect(p[0]).toEqual([0, 0]);
    expect(p[11]).toEqual([10, 0]);
  });

  it('measures roads in km', () => {
    expect(roadKm(base(), base().roads[0])).toBeCloseTo(10, 6);
  });

  it('renames, adds and deletes places with their roads', () => {
    let m = renamePlace(base(), 2, 'Bree');
    expect(m.places[1].name).toBe('Bree');
    m = addPlace(m, 2000, -5, 'Nouveau');
    expect(m.places.at(-1)).toEqual({ id: 4, name: 'Nouveau', x: 1000, y: 0, capital: false });
    m = deletePlace(m, 2);
    expect(m.places.map((p) => p.id)).toEqual([1, 3, 4]);
    expect(m.roads).toEqual([]); // les routes du lieu supprimé partent aussi
  });

  it('never goes below two places', () => {
    let m = deletePlace(base(), 3);
    m = deletePlace(m, 2);
    expect(m.places).toHaveLength(2);
  });

  it('adds a road once, and deletes it whatever the order', () => {
    let m = addRoad(base(), 3, 1);
    expect(m.roads).toHaveLength(3);
    expect(addRoad(m, 1, 3).roads).toHaveLength(3);
    expect(addRoad(m, 1, 1).roads).toHaveLength(3);
    m = deleteRoad(m, 1, 3);
    expect(m.roads).toHaveLength(2);
  });

  it('reshapes a road so that its middle follows the pointer', () => {
    const m = reshapeRoad(base(), 2, 1, [50, 30]);
    expect(roadMiddle(m, m.roads[0])).toEqual([50, 30]);
  });

  it('reports what prevents using the map', () => {
    expect(mapProblem(base())).toBeNull();
    expect(mapProblem(renamePlace(base(), 1, '  '))).toBe('activity_ui__generate_empty_name');
  });

  it('builds a map view for the 3D scene', () => {
    const v = toMapView(base());
    expect(v.terrain).toBeNull();
    expect(v.places[0]).toEqual({ id: '1', name: 'A', x: 0, y: 0, z: 0 });
    expect(v.roads[0].path).toHaveLength(12);
  });
});
