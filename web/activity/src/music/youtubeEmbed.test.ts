import { describe, expect, it } from 'vitest';
import { command, embedBase, embedUrl, LISTENING, parseEvent, sandboxFor } from './youtubeEmbed';

describe('YouTube embed', () => {
  it('goes through the Discord proxy inside Discord, directly elsewhere', () => {
    expect(embedBase('123.discordsays.com')).toBe('/.proxy/youtube');
    expect(embedBase('127.0.0.1')).toBe('https://www.youtube-nocookie.com');
  });

  it('builds the official embed URL', () => {
    const url = new URL(embedUrl('https://www.youtube-nocookie.com', 'dQw4w9WgXcQ', 'https://x.test'));
    expect(url.pathname).toBe('/embed/dQw4w9WgXcQ');
    expect(url.searchParams.get('enablejsapi')).toBe('1');
    expect(url.searchParams.get('origin')).toBe('https://x.test');
    expect(embedUrl('/.proxy/youtube', 'a/../b', 'o')).toContain('/embed/a%2F..%2Fb?');
  });

  it('never gives the proxied player the activity origin', () => {
    expect(sandboxFor('/.proxy/youtube')).not.toContain('allow-same-origin');
    expect(sandboxFor('https://www.youtube-nocookie.com')).toContain('allow-same-origin');
  });

  it('speaks the widget protocol', () => {
    expect(JSON.parse(command('setVolume', [40]))).toEqual({ event: 'command', func: 'setVolume', args: [40], id: 1, channel: 'widget' });
    expect(JSON.parse(LISTENING).event).toBe('listening');
  });

  it('reads player events', () => {
    expect(parseEvent('{"event":"onReady"}')).toEqual({ kind: 'ready' });
    expect(parseEvent('{"event":"onStateChange","info":0}')).toEqual({ kind: 'ended' });
    expect(parseEvent('{"event":"infoDelivery","info":{"playerState":1}}')).toEqual({ kind: 'playing' });
    expect(parseEvent('{"event":"onError","info":150}')).toEqual({ kind: 'error', code: 150 });
    expect(parseEvent('{"event":"infoDelivery","info":{"currentTime":3}}')).toBeNull();
    expect(parseEvent('not json')).toBeNull();
    expect(parseEvent({ event: 'onReady' })).toEqual({ kind: 'ready' });
    expect(parseEvent(null)).toBeNull();
  });
});
