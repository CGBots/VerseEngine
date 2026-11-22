import { describe, expect, it } from 'vitest';
import type { AdminTrack } from '../api';
import { draftOf, filterSpots, formatBytes, formatDuration, isDirty, spots, titleFromFileName, toggleSpot, uploadBlocker } from './musicModel';

const lib = {
  places: [{ id: '1', name: 'Kérival' }, { id: '2', name: 'Lorroc' }],
  roads: [{ id: '9', name: '', from: 'Kérival', to: 'Lorroc' }, { id: '8', name: 'Voie royale', from: 'Lorroc', to: 'Calval' }],
};
const track: AdminTrack = { id: 't', title: 'Taverne', duration_ms: 125_000, bytes: 3_000_000, origin: 'file', places: ['1'], roads: [] };

describe('music admin model', () => {
  it('proposes a title from the file name', () => {
    expect(titleFromFileName('la_taverne-du-port.final.mp3')).toBe('la taverne du port.final');
    expect(titleFromFileName('Forêt.ogg')).toBe('Forêt');
  });

  it('formats durations and sizes', () => {
    expect(formatDuration(125_000)).toBe('2:05');
    expect(formatDuration(0)).toBe('0:00');
    expect(formatBytes(3 * 1024 * 1024, 'fr')).toBe('3,0 Mo');
    expect(formatBytes(2048, 'en')).toBe('2 Ko');
  });

  it('lists places then roads, and searches without accents', () => {
    const all = spots(lib);
    expect(all.map((s) => s.label)).toEqual(['Kérival', 'Lorroc', 'Kérival → Lorroc', 'Voie royale (Lorroc → Calval)']);
    expect(filterSpots(all, 'kerival').map((s) => s.id)).toEqual(['1', '9']);
    expect(filterSpots(all, '  ')).toHaveLength(4);
  });

  it('tracks edits', () => {
    const d = draftOf(track);
    expect(isDirty(track, d)).toBe(false);
    const withRoad = toggleSpot(d, { kind: 'road', id: '9' });
    expect(withRoad.roads).toEqual(['9']);
    expect(isDirty(track, withRoad)).toBe(true);
    expect(isDirty(track, toggleSpot(withRoad, { kind: 'road', id: '9' }))).toBe(false);
    expect(isDirty(track, { ...d, title: ' Taverne ' })).toBe(false);
    expect(toggleSpot(d, { kind: 'place', id: '1' }).places).toEqual([]);
  });

  it('explains why an upload cannot start', () => {
    const l = { used_bytes: 0, max_bytes: 100, max_file_bytes: 50, tracks: [], max_tracks: 2 };
    expect(uploadBlocker(null, 'x', true, l)).toBe('activity_ui__music_choose_file');
    expect(uploadBlocker({ size: 60 }, 'x', true, l)).toBe('activity__payload_too_large');
    expect(uploadBlocker({ size: 40 }, 'x', true, { ...l, used_bytes: 70 })).toBe('activity__music_quota');
    expect(uploadBlocker({ size: 40 }, ' ', true, l)).toBe('activity__music_title_required');
    expect(uploadBlocker({ size: 40 }, 'x', false, l)).toBe('activity__music_rights_required');
    expect(uploadBlocker({ size: 40 }, 'x', true, l)).toBeNull();
  });
});
