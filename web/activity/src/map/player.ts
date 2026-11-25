import type { CharacterView, SessionInfo } from '../api';

/** Exceptions documentées de la CSP des activités : chargeables sans URL mapping. */
const ALLOWED = ['https://cdn.discordapp.com/avatars/', 'https://cdn.discordapp.com/attachments/', 'https://media.discordapp.net/attachments/'];

/**
 * Photo de l'épingle du joueur : son avatar Discord, sinon le portrait de son personnage. `null`
 * s'il n'a ni l'un ni l'autre (l'épingle affiche alors ses initiales). L'avatar par défaut de
 * Discord (`/embed/avatars/`) n'est pas une exception de la CSP : il n'est pas utilisé.
 */
export function playerPictureUrl(user: SessionInfo['user'], character: CharacterView | null): string | null {
  const candidates = [
    user.avatar ? `https://cdn.discordapp.com/avatars/${user.id}/${user.avatar}.png?size=128` : null,
    character?.image_url ?? null,
  ];
  return candidates.find((u): u is string => !!u && ALLOWED.some((p) => u.startsWith(p))) ?? null;
}

/** Initiales affichées dans l'épingle sans photo : « Aelys Ventbrume » → « AV ». */
export function initials(name: string): string {
  const parts = name.trim().split(/\s+/).filter(Boolean);
  const letters = parts.length > 1 ? [parts[0], parts[parts.length - 1]] : parts.slice(0, 1);
  return letters.map((p) => [...p][0]?.toUpperCase() ?? '').join('') || '?';
}

/**
 * Contour de l'épingle « goutte inversée » dont la pointe est en (0, 0) et le centre du cercle en
 * (0, −h + r). Repère SVG : y vers le bas.
 */
export function pinPath(r: number, h: number): string {
  const cy = -h + r;
  // Tangentes depuis la pointe jusqu'au cercle.
  const d = h - r;
  const a = Math.asin(Math.min(1, r / d));
  const tx = r * Math.cos(a);
  const ty = cy + r * Math.sin(a);
  return `M0 0 L${-tx} ${ty} A${r} ${r} 0 1 1 ${tx} ${ty} Z`;
}
