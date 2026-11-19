import { describe, expect, it } from 'vitest';
import { initials, pinPath, playerPictureUrl } from './player';

const user = { id: '42', username: 'a', global_name: null, avatar: 'abc' };
const character = { image_url: 'https://cdn.discordapp.com/attachments/1/2/p.png' } as never;

describe('playerPictureUrl', () => {
  it("prend l'avatar Discord en priorité", () => {
    expect(playerPictureUrl(user, character)).toBe('https://cdn.discordapp.com/avatars/42/abc.png?size=128');
  });
  it('se rabat sur le portrait du personnage', () => {
    expect(playerPictureUrl({ ...user, avatar: null }, character)).toBe('https://cdn.discordapp.com/attachments/1/2/p.png');
  });
  it('refuse une URL hors des exceptions de la CSP', () => {
    expect(playerPictureUrl({ ...user, avatar: null }, { image_url: 'https://evil.example/p.png' } as never)).toBeNull();
    expect(playerPictureUrl({ ...user, avatar: null }, null)).toBeNull();
  });
});

describe('initials', () => {
  it('garde la première lettre du prénom et du nom', () => {
    expect(initials('Aelys Ventbrume')).toBe('AV');
    expect(initials('Élise')).toBe('É');
    expect(initials('  ')).toBe('?');
  });
});

describe('pinPath', () => {
  it('part de la pointe et décrit un arc', () => {
    const d = pinPath(10, 30);
    expect(d.startsWith('M0 0 L')).toBe(true);
    expect(d).toContain('A10 10 0 1 1');
    expect(d.endsWith('Z')).toBe(true);
  });
});
