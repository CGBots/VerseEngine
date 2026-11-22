import { ServerClock } from './time';

export interface TravelView {
  in_move: boolean;
  step_end_timestamp: number | null;
  eta_timestamp: number | null;
  remaining_km: number | null;
  traveled_km: number;
  speed_kmh: number;
  final_step: boolean;
}

export interface StatusView {
  stat: string;
  value: unknown;
  modifier_type: 'base' | 'multiplier' | 'flat';
  level: 'player' | 'place' | 'area' | 'universe';
  end_timestamp: number | null;
  source: string;
}

export interface CharacterView {
  id: string;
  name: string;
  description: string | null;
  image_url: string | null;
  speed: number | null;
  stats: { id: string; name: string; value: unknown }[];
  statuses: StatusView[];
  location: {
    kind: 'place' | 'road';
    id: string;
    name: string | null;
    source_id: string | null;
    destination_id: string | null;
    destination_name: string | null;
  } | null;
  travel: TravelView | null;
  group_members: string[];
  server_time: number;
}

export interface SessionInfo {
  access_token: string;
  session_token: string;
  expires_in: number;
  user: { id: string; username: string; global_name: string | null; avatar: string | null };
  guild_id: string;
  /** `null` : serveur relié à aucun univers (seul un administrateur peut alors ouvrir l'activité, pour l'import). */
  universe: { id: string; name: string } | null;
  access: 'player' | 'moderator' | 'admin';
  /** Import et génération de carte permis (créateur de l'univers ou administrateur). */
  can_import: boolean;
  /** Créateur de l'univers : lui seul règle le brouillard (I17). */
  is_creator: boolean;
  fog_enabled: boolean;
  admin_mode_available: boolean;
  /** `null` si le temps de l'univers est à l'arrêt. */
  universe_clock: { origin: number; phase_duration_secs: number } | null;
  server_time: number;
}

export interface MapView {
  source: 'Azgaar' | 'Computed' | 'Manual';
  width: number;
  height: number;
  places: { id: string; name: string; x: number; y: number; z: number }[];
  roads: { id: string; name: string; from: string; to: string; distance_km: number; secret: boolean; path?: [number, number][] }[];
  terrain: { cols: number; rows: number; heights: number[] } | null;
  zone: { polygon: [number, number][]; outside: 'crop' | 'grey' | 'remove' } | null;
  /** Image exportée par Azgaar (mêmes coordonnées que les lieux). */
  background: { url: string; width: number; height: number } | null;
  /** Brouillard (E8) : zones à dégager ; absent s'il est désactivé ou pour un modérateur. */
  fog?: FogView | null;
  server_time: number;
}

export interface FogView {
  /** Rayon d'une zone découverte (unités de carte). */
  radius: number;
  discovered: string[];
  /** Destinations possibles pas encore découvertes : zone plus petite. */
  glimpsed: string[];
  /** Routes empruntées : couloir dégagé. */
  roads: string[];
}

export interface TravelState {
  location: CharacterView['location'];
  travel: TravelView | null;
  group_members: string[];
  destinations: { id: string; name: string; distance_km: number }[];
  result?: string;
  server_time: number;
}

/** Actions de voyage : mêmes services que les commandes slash. */
export const travelApi = {
  state: () => api<TravelState>('/me/travel'),
  start: (destinationId: string) =>
    api<TravelState>('/me/travel/start', { method: 'POST', body: JSON.stringify({ destination_id: destinationId }) }),
  stop: () => api<TravelState>('/me/travel/stop', { method: 'POST' }),
  join: (targetUserId: string) =>
    api<TravelState>('/me/travel/join', { method: 'POST', body: JSON.stringify({ target_user_id: targetUserId }) }),
  leave: () => api<TravelState>('/me/travel/leave', { method: 'POST' }),
};

/** Erreur portant une clé Fluent renvoyée par le serveur. */
export class ApiError extends Error {
  constructor(public status: number, public key: string) {
    super(key);
  }
}

export const clock = new ServerClock();

let sessionToken: string | null = null; // en mémoire uniquement (jamais de stockage persistant)

export function setSessionToken(token: string | null): void {
  sessionToken = token;
}

export function getSessionToken(): string | null {
  return sessionToken;
}

/** Ressource binaire authentifiée (fond de carte) → URL locale `blob:` utilisable par <img>. */
export async function apiBlobUrl(path: string): Promise<string> {
  const headers = new Headers();
  if (sessionToken) headers.set('Authorization', `Bearer ${sessionToken}`);
  const res = await fetch(path.startsWith('/api') ? path : `/api${path}`, { headers });
  if (!res.ok) throw new ApiError(res.status, 'activity__map_not_found');
  return URL.createObjectURL(await res.blob());
}

export async function api<T>(path: string, init: RequestInit = {}): Promise<T> {
  const headers = new Headers(init.headers);
  headers.set('Accept', 'application/json');
  if (init.body) headers.set('Content-Type', 'application/json');
  if (sessionToken) headers.set('Authorization', `Bearer ${sessionToken}`);
  const res = await fetch(`/api${path}`, { ...init, headers });
  const body = await res.json().catch(() => ({}));
  if (!res.ok) throw new ApiError(res.status, typeof body.error === 'string' ? body.error : 'activity__internal_error');
  if (typeof body.server_time === 'number') clock.sync(body.server_time);
  return body as T;
}

// ---------------------------------------------------------------------------
// Import de carte (vue admin)
// ---------------------------------------------------------------------------

export type ImportMode = 'replace' | 'merge' | 'reimport';
export type SetupType = 'FullSetup' | 'PartialSetup';

export interface ServerChoice {
  guild_id: string;
  name: string;
  linked: 'none' | 'this' | 'other';
  eligible: boolean;
  current: boolean;
}

export interface ServerList {
  servers: ServerChoice[];
  max_places_per_server: number;
  invite_url: string | null;
}

export interface UploadedMap {
  name: string | null;
  version: string;
  width: number;
  height: number;
  distance_unit: string | null;
  route_count: number;
  burgs: { id: number; name: string; x: number; y: number; capital: boolean }[];
  terrain: { cols: number; rows: number; heights: number[] } | null;
  suggestions: { place_id: string; place_name: string; status: 'exact' | 'ambiguous' | 'partial' | 'none'; burgs: number[] }[];
  has_universe: boolean;
  /** Carte générée par l'activité (« Générer une map »). */
  generated?: boolean;
}

/** Carte générée (E7) : mêmes champs que côté serveur (`activity/map/generate.rs`). */
export interface GeneratedMap {
  seed: number;
  width: number;
  height: number;
  km_per_unit: number;
  places: { id: number; name: string; x: number; y: number; capital: boolean }[];
  roads: { a: number; b: number; control: [number, number] }[];
}

export type PlaceAction = { kind: 'create' } | { kind: 'keep'; category_id: string } | { kind: 'attach'; category_id: string };
export type RoadAction = { kind: 'create' } | { kind: 'keep'; channel_id: string } | { kind: 'update'; channel_id: string };

export interface ServerBudget {
  places: number;
  roads_hosted: number;
  road_categories: number;
  channels: number;
  roles: number;
}

export interface ImportPlan {
  places: { burg_id: number; name: string; x: number; y: number; capital: boolean; action: PlaceAction; server: number }[];
  roads: { burg_a: number; burg_b: number; distance: number; uses_sea: boolean; secret: boolean; path: [number, number][]; action: RoadAction; host: number }[];
  remove_places: string[];
  remove_roads: string[];
  servers: { guild_id: string | null; budget: ServerBudget }[];
  cut_roads: number;
  isolated: number[];
}

export interface RemovalInfo {
  characters: number;
  travelling: number;
  items: { tool_id: string; name: string; place_name: string; owner: string | null; content: number }[];
}

export interface PlanResponse {
  /** Ce que la suppression des lieux existants toucherait (I15), `null` s'il n'y a rien à supprimer. */
  removal: RemovalInfo | null;
  plan: ImportPlan;
  servers_to_create: number;
  discord_objects: number;
  universe_name: string | null;
}

export interface PlanRequest {
  mode: ImportMode;
  include_sea: boolean;
  secret: [number, number][];
  attach: { burg_id: number; place_id: string }[];
  servers: string[];
  setup: SetupType;
}

export interface ExecuteRequest {
  plan: PlanRequest;
  confirm_name: string;
  new_universe?: { name: string; time_modifier?: number; fog?: boolean };
  removal?: RemovalChoices;
}

export interface RemovalChoices {
  start_burg?: number;
  items: { tool_id: string; action: 'destroy' | 'return' }[];
  confirm_destroy_unowned: boolean;
}

export interface JobState {
  progress: { phase: 'Creating' | 'Waiting' | 'RollingBack' | 'Done' | 'RolledBack' | 'RollbackIncomplete'; done: number; total: number };
  outcome: { status: 'running' } | { status: 'done'; universe_id: string } | { status: 'failed'; error: string; rolled_back: boolean };
}

/** Envoi brut d'une sauvegarde `.map` (le serveur la lit lui-même, 32 Mio au plus). */
export async function uploadMapFile(bytes: ArrayBuffer): Promise<UploadedMap> {
  const headers = new Headers({ Accept: 'application/json', 'Content-Type': 'application/octet-stream' });
  if (sessionToken) headers.set('Authorization', `Bearer ${sessionToken}`);
  const res = await fetch('/api/admin/import/map', { method: 'POST', headers, body: bytes });
  const body = await res.json().catch(() => ({}));
  if (!res.ok) throw new ApiError(res.status, typeof body.error === 'string' ? body.error : 'activity__internal_error');
  return body as UploadedMap;
}

export const universeApi = {
  setFog: (enabled: boolean) => api<{ fog_enabled: boolean }>('/admin/universe/fog', { method: 'POST', body: JSON.stringify({ enabled }) }),
};

export const importApi = {
  generate: (seed: number | null, count: number) =>
    api<GeneratedMap>('/admin/import/generate', { method: 'POST', body: JSON.stringify(seed === null ? { count } : { seed, count }) }),
  useGenerated: (map: GeneratedMap) => api<UploadedMap>('/admin/import/generated', { method: 'POST', body: JSON.stringify(map) }),
  servers: () => api<ServerList>('/admin/import/servers'),
  plan: (req: PlanRequest) => api<PlanResponse>('/admin/import/plan', { method: 'POST', body: JSON.stringify(req) }),
  execute: (req: ExecuteRequest) => api<{ job_id: string; total: number }>('/admin/import/execute', { method: 'POST', body: JSON.stringify(req) }),
  job: (id: string) => api<JobState>(`/admin/import/jobs/${encodeURIComponent(id)}`),
};

// ---------------------------------------------------------------------------
// Musique (MUSIC_SPEC.md)
// ---------------------------------------------------------------------------

export interface MusicHere {
  spot: { kind: 'place' | 'road'; id: string } | null;
  tracks: { id: string; title: string; duration_ms: number }[];
}

export interface AdminTrack {
  id: string;
  title: string;
  duration_ms: number;
  bytes: number;
  origin: 'file' | 'youtube';
  places: string[];
  roads: string[];
}

export interface MusicLibrary {
  tracks: AdminTrack[];
  used_bytes: number;
  max_bytes: number;
  max_tracks: number;
  max_file_bytes: number;
  places: { id: string; name: string }[];
  roads: { id: string; name: string; from: string; to: string }[];
  youtube_enabled: boolean;
}

export const musicApi = {
  here: () => api<MusicHere>('/music/here'),
  /** Fichier d'un morceau, téléchargé avec le jeton, en URL `blob:` locale. */
  trackUrl: (id: string) => apiBlobUrl(`/music/tracks/${encodeURIComponent(id)}`),
  library: () => api<MusicLibrary>('/admin/music'),
  update: (id: string, body: { title: string; places: string[]; roads: string[] }) =>
    api<AdminTrack>(`/admin/music/${encodeURIComponent(id)}`, { method: 'PUT', body: JSON.stringify(body) }),
  remove: async (id: string) => {
    const headers = new Headers();
    if (sessionToken) headers.set('Authorization', `Bearer ${sessionToken}`);
    const res = await fetch(`/api/admin/music/${encodeURIComponent(id)}`, { method: 'DELETE', headers });
    if (!res.ok) {
      const body = await res.json().catch(() => ({}));
      throw new ApiError(res.status, typeof body.error === 'string' ? body.error : 'activity__internal_error');
    }
  },
  upload: async (bytes: ArrayBuffer, title: string, rights: boolean): Promise<AdminTrack> => {
    const headers = new Headers({ Accept: 'application/json', 'Content-Type': 'application/octet-stream' });
    if (sessionToken) headers.set('Authorization', `Bearer ${sessionToken}`);
    const q = new URLSearchParams({ title, rights: String(rights) });
    const res = await fetch(`/api/admin/music/upload?${q}`, { method: 'POST', headers, body: bytes });
    const body = await res.json().catch(() => ({}));
    if (!res.ok) throw new ApiError(res.status, typeof body.error === 'string' ? body.error : 'activity__internal_error');
    return body as AdminTrack;
  },
};
