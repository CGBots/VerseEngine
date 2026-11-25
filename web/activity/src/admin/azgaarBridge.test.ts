import { describe, expect, it } from 'vitest';
import { exportRequest, mapBytes, MAX_MAP_BYTES, parseAzgaarMessage } from './azgaarBridge';

const frame = { name: 'azgaar' };

describe('Azgaar messages', () => {
  it('only accepts messages from the Azgaar frame', () => {
    expect(parseAzgaarMessage({ source: frame, data: { type: 'verseengine:ready' } }, frame)).toEqual({ kind: 'ready' });
    expect(parseAzgaarMessage({ source: {}, data: { type: 'verseengine:ready' } }, frame)).toBeNull();
    expect(parseAzgaarMessage({ source: null, data: { type: 'verseengine:ready' } }, null)).toBeNull();
  });

  it('reads a map or an error answering a request', () => {
    expect(parseAzgaarMessage({ source: frame, data: { type: 'verseengine:map', id: 3, data: '1.158|x' } }, frame)).toEqual({ kind: 'map', id: 3, data: '1.158|x' });
    const long = 'x'.repeat(500);
    expect(parseAzgaarMessage({ source: frame, data: { type: 'verseengine:error', id: 3, message: long } }, frame)).toEqual({ kind: 'error', id: 3, message: long.slice(0, 200) });
  });

  it('ignores anything else', () => {
    for (const data of [null, 'verseengine:map', 42, { type: 'verseengine:map', id: '3', data: 'x' }, { type: 'verseengine:map', id: 1.5, data: 'x' }, { type: 'verseengine:map', id: 3, data: {} }, { type: 'session', id: 1 }]) {
      expect(parseAzgaarMessage({ source: frame, data }, frame)).toBeNull();
    }
  });

  it('builds the request', () => {
    expect(exportRequest(7)).toEqual({ type: 'verseengine:export-request', id: 7 });
  });
});

describe('map bytes', () => {
  it('encodes the map as UTF-8', () => {
    expect(Array.from(mapBytes('é')!)).toEqual([0xc3, 0xa9]);
  });
  it('refuses an empty or oversized map', () => {
    expect(mapBytes('')).toBeNull();
    expect(mapBytes('x'.repeat(MAX_MAP_BYTES + 1))).toBeNull();
    expect(mapBytes('é'.repeat(MAX_MAP_BYTES / 2 + 1))).toBeNull();
  });
});
