import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { FADE_MS, MusicPlayer, type AudioLike, type PlayerState } from './player';

class FakeAudio implements AudioLike {
  src = '';
  volume = 1;
  playing = false;
  allowed = true;
  private listeners: Record<string, (() => void)[]> = {};
  play(): Promise<void> {
    if (!this.allowed) return Promise.reject(new DOMException('blocked', 'NotAllowedError'));
    this.playing = true;
    return Promise.resolve();
  }
  pause(): void {
    this.playing = false;
  }
  addEventListener(type: string, l: () => void): void {
    (this.listeners[type] ??= []).push(l);
  }
  emit(type: string): void {
    for (const l of this.listeners[type] ?? []) l();
  }
}

const flush = async () => {
  for (let i = 0; i < 10; i++) await Promise.resolve();
};

function setup() {
  const audio = new FakeAudio();
  const loaded: string[] = [];
  const revoked: string[] = [];
  const states: PlayerState[] = [];
  const player = new MusicPlayer(
    () => audio,
    async (id) => {
      loaded.push(id);
      if (id === 'gone') throw new Error('404');
      return `blob:${id}`;
    },
    (s) => states.push(s),
    (u) => revoked.push(u),
  );
  player.setSettings({ volume: 1, muted: false });
  return { audio, loaded, revoked, states, player };
}

describe('MusicPlayer', () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => vi.useRealTimers());

  it('starts the playlist of the place with a fade-in', async () => {
    const { audio, player, states } = setup();
    player.setPlaylist({ spot: 'place:1', tracks: [{ id: 'a', title: 'Taverne' }] });
    await flush();
    expect(audio.src).toBe('blob:a');
    expect(audio.playing).toBe(true);
    expect(audio.volume).toBe(0);
    vi.advanceTimersByTime(FADE_MS);
    expect(audio.volume).toBe(1);
    expect(states.at(-1)).toEqual({ title: 'Taverne', blocked: false });
  });

  it('chains tracks and loops', async () => {
    const { audio, player } = setup();
    player.setPlaylist({ spot: 'place:1', tracks: [{ id: 'a', title: 'A' }, { id: 'b', title: 'B' }] });
    await flush();
    const first = audio.src;
    audio.emit('ended');
    await flush();
    expect(audio.src).not.toBe(first);
    audio.emit('ended');
    await flush();
    expect(['blob:a', 'blob:b']).toContain(audio.src);
  });

  it('fades out then switches when the player moves', async () => {
    const { audio, player, revoked } = setup();
    player.setPlaylist({ spot: 'place:1', tracks: [{ id: 'a', title: 'A' }] });
    await flush();
    vi.advanceTimersByTime(FADE_MS);
    player.setPlaylist({ spot: 'road:9', tracks: [{ id: 'r', title: 'Route' }] });
    vi.advanceTimersByTime(FADE_MS / 2);
    expect(audio.src).toBe('blob:a');
    expect(audio.volume).toBeGreaterThan(0);
    expect(audio.volume).toBeLessThan(1);
    vi.advanceTimersByTime(FADE_MS / 2 + 100);
    await flush();
    expect(audio.src).toBe('blob:r');
    expect(revoked).toContain('blob:a');
  });

  it('goes silent where there is no music', async () => {
    const { audio, player, states } = setup();
    player.setPlaylist({ spot: 'place:1', tracks: [{ id: 'a', title: 'A' }] });
    await flush();
    player.setPlaylist({ spot: 'place:2', tracks: [] });
    vi.advanceTimersByTime(FADE_MS + 100);
    await flush();
    expect(audio.playing).toBe(false);
    expect(states.at(-1)?.title).toBeNull();
  });

  it('keeps playing when the library changes but the track stays', async () => {
    const { audio, player } = setup();
    player.setPlaylist({ spot: 'place:1', tracks: [{ id: 'a', title: 'A' }] });
    await flush();
    player.setPlaylist({ spot: 'place:1', tracks: [{ id: 'a', title: 'A' }, { id: 'b', title: 'B' }] });
    await flush();
    expect(audio.src).toBe('blob:a');
  });

  it('skips a track that cannot be loaded, without looping forever', async () => {
    const { audio, player, states } = setup();
    player.setPlaylist({ spot: 'place:1', tracks: [{ id: 'gone', title: 'X' }, { id: 'a', title: 'A' }] });
    await flush();
    await flush();
    expect(audio.src).toBe('blob:a');
    player.setPlaylist({ spot: 'place:2', tracks: [{ id: 'gone', title: 'X' }] });
    vi.advanceTimersByTime(FADE_MS + 100);
    await flush();
    await flush();
    expect(states.at(-1)?.title).toBeNull();
  });

  it('reports a blocked autoplay, and unlocks on a user gesture', async () => {
    const { audio, player, states } = setup();
    audio.allowed = false;
    player.setPlaylist({ spot: 'place:1', tracks: [{ id: 'a', title: 'A' }] });
    await flush();
    expect(states.at(-1)?.blocked).toBe(true);
    audio.allowed = true;
    player.unlock();
    await flush();
    expect(states.at(-1)?.blocked).toBe(false);
  });

  it('mutes without stopping', async () => {
    const { audio, player } = setup();
    player.setPlaylist({ spot: 'place:1', tracks: [{ id: 'a', title: 'A' }] });
    await flush();
    vi.advanceTimersByTime(FADE_MS);
    player.setSettings({ volume: 1, muted: true });
    expect(audio.volume).toBe(0);
    expect(audio.playing).toBe(true);
  });
});
