import type { AdminTrack, MusicLibrary } from '../api';

/** Titre proposé d'après le nom du fichier : sans extension, tirets et soulignés en espaces. */
export function titleFromFileName(name: string): string {
  return name.replace(/\.[^.]+$/, '').replace(/[_-]+/g, ' ').replace(/\s+/g, ' ').trim().slice(0, 100);
}

export function formatDuration(ms: number): string {
  const total = Math.round(ms / 1000);
  return `${Math.floor(total / 60)}:${String(total % 60).padStart(2, '0')}`;
}

/** Taille lisible (Mo avec une décimale, Ko en dessous) ; unités traduites par l'appelant. */
export function formatBytes(bytes: number, locale: string, units = { mb: 'Mo', kb: 'Ko' }): string {
  const mb = bytes / (1024 * 1024);
  const f = (n: number, d: number) => n.toLocaleString(locale, { maximumFractionDigits: d, minimumFractionDigits: d });
  return mb >= 1 ? `${f(mb, 1)} ${units.mb}` : `${f(Math.max(1, Math.round(bytes / 1024)), 0)} ${units.kb}`;
}

export interface Spot {
  kind: 'place' | 'road';
  id: string;
  label: string;
}

/** Lieux puis routes, avec un libellé lisible (« Départ → Arrivée » pour une route sans nom). */
export function spots(lib: Pick<MusicLibrary, 'places' | 'roads'>): Spot[] {
  return [
    ...lib.places.map((p) => ({ kind: 'place' as const, id: p.id, label: p.name })),
    ...lib.roads.map((r) => ({ kind: 'road' as const, id: r.id, label: r.name ? `${r.name} (${r.from} → ${r.to})` : `${r.from} → ${r.to}` })),
  ];
}

const fold = (s: string) => s.normalize('NFD').replace(/\p{Diacritic}/gu, '').toLowerCase();

/** Recherche sans accents ni casse ; vide : tout. */
export function filterSpots(all: Spot[], query: string): Spot[] {
  const q = fold(query.trim());
  return q ? all.filter((s) => fold(s.label).includes(q)) : all;
}

/** Modification en cours d'un morceau. */
export interface TrackDraft {
  title: string;
  places: string[];
  roads: string[];
}

export const draftOf = (t: AdminTrack): TrackDraft => ({ title: t.title, places: [...t.places], roads: [...t.roads] });

const sameSet = (a: string[], b: string[]) => a.length === b.length && a.every((x) => b.includes(x));

export function isDirty(t: AdminTrack, d: TrackDraft): boolean {
  return d.title.trim() !== t.title || !sameSet(d.places, t.places) || !sameSet(d.roads, t.roads);
}

export function toggleSpot(d: TrackDraft, spot: Pick<Spot, 'kind' | 'id'>): TrackDraft {
  const key = spot.kind === 'place' ? 'places' : 'roads';
  const list = d[key];
  return { ...d, [key]: list.includes(spot.id) ? list.filter((x) => x !== spot.id) : [...list, spot.id] };
}

/** Raison qui empêche l'envoi (clé de traduction), ou `null`. */
export function uploadBlocker(file: { size: number } | null, title: string, rights: boolean, lib: Pick<MusicLibrary, 'used_bytes' | 'max_bytes' | 'max_file_bytes' | 'tracks' | 'max_tracks'>): string | null {
  if (!file) return 'activity_ui__music_choose_file';
  if (file.size > lib.max_file_bytes) return 'activity__payload_too_large';
  if (lib.tracks.length >= lib.max_tracks || lib.used_bytes + file.size > lib.max_bytes) return 'activity__music_quota';
  if (!title.trim()) return 'activity__music_title_required';
  if (!rights) return 'activity__music_rights_required';
  return null;
}
