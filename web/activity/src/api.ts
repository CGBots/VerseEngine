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
  /** `burg` : icône du bourg d'origine (import Azgaar avec rendu façon Azgaar). */
  places: { id: string; name: string; x: number; y: number; z: number; burg?: BurgIcon }[];
  roads: { id: string; name: string; from: string; to: string; distance_km: number; secret: boolean; path?: [number, number][] }[];
  terrain: { cols: number; rows: number; heights: number[] } | null;
  zone: { polygon: [number, number][]; outside: 'crop' | 'grey' | 'remove' } | null;
  /** Image exportée par Azgaar (mêmes coordonnées que les lieux). */
  background: { url: string; width: number; height: number } | null;
  /** Rendu façon Azgaar (sauvegarde `.map`) ; absent : carte générée ou calculée. */
  style?: MapStyleView;
  /** Brouillard (E8) : zones à dégager ; absent s'il est désactivé ou en mode admin. */
  fog?: FogView | null;
  server_time: number;
}

/** Taille (0 village, 1 bourg, 2 ville), capitale, port. */
export interface BurgIcon {
  size: number;
  capital: boolean;
  port: boolean;
}

/** Rendu façon Azgaar (`activity/map/azgaar_style.rs`), décodé par `map/azgaarStyle.ts`. */
export interface MapStyleView {
  cols: number;
  rows: number;
  /** Grille de surface, suites (valeur, longueur) en base64 : biome, mer (200 + éloignement de
   * la côte), lac (230), inconnu sous le brouillard (255). */
  surface: string;
  /** État de chaque case, même encodage ; absent sans États. */
  states?: string;
  biome_colors: string[];
  state_colors?: string[];
  /** Rivières : tracé et largeur à la source et à l'embouchure (unités de carte). */
  rivers: { points: [number, number][]; width: [number, number] }[];
  /** Faux : biomes estimés (autre version d'Azgaar), sans États ni rivières. */
  exact: boolean;
  /** Contours vectoriels (absents des anciennes cartes) : le brouillard est déjà appliqué. */
  vector?: {
    land: string;
    lakes: string;
    biomes: [number, string][];
    states: [number, string][];
    borders: string;
  };
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

export interface NearbyGroup {
  leader_id: string;
  members: { user_id: string; name: string }[];
  moving: boolean;
}

export interface GroupInvite {
  from_user_id: string;
  from_name: string;
  expires_at: number;
}

export interface TravelState {
  location: CharacterView['location'];
  travel: TravelView | null;
  group_members: string[];
  /** Noms des membres, dans l'ordre de `group_members`. */
  group_names: string[];
  destinations: { id: string; name: string; distance_km: number }[];
  /** Vitesse du groupe (km/h) : celle du membre le plus lent, effets compris. */
  group_speed_kmh: number | null;
  /** Groupes à la même position (à rejoindre ou à inviter). */
  nearby: NearbyGroup[];
  /** Invitations reçues, encore valides. */
  invites: GroupInvite[];
  result?: string;
  server_time: number;
}

/** Route de la carte : en mode admin, toute la carte (accordée seulement si la session y a droit). */
export function mapPath(adminMode: boolean, spectator = false): string {
  if (adminMode) return '/map?view=admin';
  return spectator ? '/map?view=spectator' : '/map';
}

/** Groupe vu par un spectateur : membres, position et trajet. */
export interface SpectatorGroup {
  members: { user_id: string; name: string }[];
  location: CharacterView['location'];
  travel: TravelView | null;
}

export interface CharacterRequestBody {
  name: string;
  description: string;
  story: string;
  special_request: string;
}

/** Joueur sans personnage : demander un personnage, ou regarder en spectateur. */
export const newcomerApi = {
  requestCharacter: (body: CharacterRequestBody) =>
    api<{ result: string; url: string }>('/me/character_request', { method: 'POST', body: JSON.stringify(body) }),
  spectate: () => api<{ role_given: boolean }>('/me/spectator', { method: 'POST' }),
  players: () => api<{ groups: SpectatorGroup[]; server_time: number }>('/spectator/players'),
};

/** Actions de voyage : mêmes services que les commandes slash. */
export const travelApi = {
  state: () => api<TravelState>('/me/travel'),
  start: (destinationId: string) =>
    api<TravelState>('/me/travel/start', { method: 'POST', body: JSON.stringify({ destination_id: destinationId }) }),
  stop: () => api<TravelState>('/me/travel/stop', { method: 'POST' }),
  join: (targetUserId: string) =>
    api<TravelState>('/me/travel/join', { method: 'POST', body: JSON.stringify({ target_user_id: targetUserId }) }),
  leave: () => api<TravelState>('/me/travel/leave', { method: 'POST' }),
  /** « Créer un groupe » : inviter un joueur à la même position. */
  invite: (userId: string) => api<TravelState>('/me/travel/invite', { method: 'POST', body: JSON.stringify({ user_id: userId }) }),
  acceptInvite: (fromUserId: string) =>
    api<TravelState>('/me/travel/invite/accept', { method: 'POST', body: JSON.stringify({ from_user_id: fromUserId }) }),
  declineInvite: (fromUserId: string) =>
    api<TravelState>('/me/travel/invite/decline', { method: 'POST', body: JSON.stringify({ from_user_id: fromUserId }) }),
};

// ---------------------------------------------------------------------------
// Inventaire, recettes, craft et farm (mêmes services que `/recipe` et `/loot`)
// ---------------------------------------------------------------------------

export type ItemUsage = 'consumable' | 'usable' | 'placeable' | 'wearable' | 'none';

export interface ItemCard {
  inventory_id: string;
  item_id: string;
  name: string;
  description: string | null;
  /** Informations réservées au détenteur. */
  secret: string | null;
  usage: ItemUsage;
  effects: { stat: string; value: unknown; modifier_type: 'base' | 'multiplier' | 'flat' }[];
  quantity: number;
}

export interface InventoryView {
  items: ItemCard[];
  server_time: number;
}

export interface RecipeView {
  id: string;
  name: string;
  /** Durée d'un exemplaire (s). */
  delay: number;
  ingredients: { item_id: string; name: string; quantity: number; owned: number }[];
  results: { name: string; quantity: number }[];
  tools: { name: string; available: boolean }[];
  /** Exemplaires faisables maintenant, calculé par le serveur (0 : non réalisable). */
  max_craftable: number;
}

export interface RecipeBook {
  /** Réalisables d'abord. */
  recipes: RecipeView[];
  max_quantity: number;
  server_time: number;
}

export interface CraftState {
  craft: {
    recipe_id: string;
    recipe_name: string | null;
    quantity: number;
    completed: number;
    start_timestamp: number;
    end_timestamp: number;
  } | null;
  result?: string;
  /** Arrêt : exemplaires livrés, et exemplaires dont les ingrédients ont été rendus. */
  delivered?: number;
  refunded?: number;
  server_time: number;
}

export interface FarmState {
  farm: { start_timestamp: number; end_timestamp: number } | null;
  result?: string;
  /** Fouille instantanée : butin obtenu. */
  looted?: { name: string; quantity: number; unknown: boolean }[];
  /** Dernière fouille terminée (il y a peu) : objets obtenus. */
  finished?: { at: number; items: { name: string; quantity: number; unknown: boolean }[] };
  /** Fouille refusée : recharge restante (s). */
  cooldown_secs?: number;
  server_time: number;
}

export const craftApi = {
  inventory: () => api<InventoryView>('/me/inventory'),
  /** Consommer un objet : effets appliqués tout de suite (service de `/consume`). */
  useItem: (itemId: string) =>
    api<{ inventory: InventoryView; result: string; item_name: string }>('/me/inventory/use', { method: 'POST', body: JSON.stringify({ item_id: itemId }) }),
  recipes: () => api<RecipeBook>('/me/recipes'),
  state: () => api<CraftState>('/me/craft'),
  start: (recipeId: string, quantity: number) =>
    api<CraftState>('/me/craft/start', { method: 'POST', body: JSON.stringify({ recipe_id: recipeId, quantity }) }),
  stop: () => api<CraftState>('/me/craft/stop', { method: 'POST' }),
};

export const farmApi = {
  state: () => api<FarmState>('/me/farm'),
  start: () => api<FarmState>('/me/farm/start', { method: 'POST' }),
  stop: () => api<FarmState>('/me/farm/stop', { method: 'POST' }),
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
/** Taille d'une partie d'envoi de musique (côté serveur : `UPLOAD_PART_BYTES`). */
export const UPLOAD_PART_BYTES = 16 * 1024 * 1024;

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
  /** Durée estimée (s), attentes de Discord comprises (mesurée sur des imports réels). */
  estimated_secs: number;
  /** Discord bloque déjà des créations (limite de débit longue) : secondes restantes. */
  discord_wait_secs?: number | null;
  universe_name: string | null;
}

export interface PlanRequest {
  mode: ImportMode;
  include_sea: boolean;
  secret: [number, number][];
  attach: { burg_id: number; place_id: string }[];
  servers: string[];
  setup: SetupType;
  /** Zone à importer (polygone, coordonnées de la carte) ; absente : toute la carte. */
  zone?: [number, number][];
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
  progress: { phase: 'Creating' | 'Waiting' | 'Deleting' | 'RollingBack' | 'Done' | 'RolledBack' | 'RollbackIncomplete'; done: number; total: number };
  outcome: { status: 'running' } | { status: 'done'; universe_id: string } | { status: 'failed'; error: string; rolled_back: boolean };
  /** Attente imposée par Discord en cours (limite de débit), en secondes. */
  discord_wait_secs?: number | null;
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

/** Champs de session relus sans réauthentification (`GET /api/me/session`), ex. après un import. */
export type SessionRefresh = Omit<SessionInfo, 'access_token' | 'session_token' | 'expires_in' | 'user'>;
export const refreshSession = () => api<SessionRefresh>('/me/session');

export const universeApi = {
  setFog: (enabled: boolean) => api<{ fog_enabled: boolean }>('/admin/universe/fog', { method: 'POST', body: JSON.stringify({ enabled }) }),
};

/** Sauvegarde .map gardée avec l'univers. */
export interface MapSourceInfo {
  seed: string | null;
  name: string | null;
  version: string | null;
  bytes: number;
  saved_at: number;
}

export const importApi = {
  generate: (seed: number | null, count: number) =>
    api<GeneratedMap>('/admin/import/generate', { method: 'POST', body: JSON.stringify(seed === null ? { count } : { seed, count }) }),
  useGenerated: (map: GeneratedMap) => api<UploadedMap>('/admin/import/generated', { method: 'POST', body: JSON.stringify(map) }),
  servers: () => api<ServerList>('/admin/import/servers'),
  plan: (req: PlanRequest) => api<PlanResponse>('/admin/import/plan', { method: 'POST', body: JSON.stringify(req) }),
  execute: (req: ExecuteRequest) => api<{ job_id: string; total: number }>('/admin/import/execute', { method: 'POST', body: JSON.stringify(req) }),
  job: (id: string) => api<JobState>(`/admin/import/jobs/${encodeURIComponent(id)}`),
  /** Même carte, déjà importée : rendu recalculé (carte vectorielle) sans rien changer sur Discord. */
  renderOnly: () => api<{ result: string; vector: boolean }>('/admin/import/render', { method: 'POST' }),
  /** Sauvegardes .map gardées pour l'univers (la plus récente d'abord). */
  sources: () => api<{ sources: MapSourceInfo[] }>('/admin/import/source'),
  /** Renvoie la dernière sauvegarde en message privé. */
  sendSource: () => api<{ result: string }>('/admin/import/source/send', { method: 'POST' }),
};

// ---------------------------------------------------------------------------
// Musique (MUSIC_SPEC.md)
// ---------------------------------------------------------------------------

export interface MusicHere {
  spot: { kind: 'place' | 'road'; id: string } | null;
  tracks: { id: string; title: string; duration_ms: number; kind: 'file' | 'youtube'; video_id?: string }[];
}

export interface AdminTrack {
  id: string;
  title: string;
  duration_ms: number;
  bytes: number;
  origin: 'file' | 'youtube';
  places: string[];
  roads: string[];
  video_id?: string;
}

export interface MusicLibrary {
  tracks: AdminTrack[];
  used_bytes: number;
  max_bytes: number;
  max_tracks: number;
  max_file_bytes: number;
  places: { id: string; name: string }[];
  roads: { id: string; name: string; from: string; to: string }[];
}

export const musicApi = {
  /** Vidéo YouTube : seul son identifiant est gardé (lecteur officiel, visible). */
  youtube: (url: string, title: string, rights: boolean) =>
    api<AdminTrack>('/admin/music/youtube', { method: 'POST', body: JSON.stringify({ url, title: title.trim(), rights }) }),
  here: () => api<MusicHere>('/music/here'),
  /** Fichier d'un morceau, téléchargé avec le jeton, en URL `blob:` locale. */
  /** Adresse de lecture en continu (par plages) : la musique démarre sans tout télécharger,
   *  même pour une ambiance de plusieurs heures. */
  trackUrl: (id: string) => api<{ url: string }>(`/music/tracks/${encodeURIComponent(id)}/stream`, { method: 'POST' }).then((r) => r.url),
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
  /**
   * Envoi d'un fichier en parties de `UPLOAD_PART_BYTES` (le proxy de Discord refuse les requêtes
   * de plus de ~128 Mio), puis vérification en tâche de fond, suivie jusqu'au bout.
   * `onProgress(fraction envoyée, en vérification)`.
   */
  upload: async (bytes: ArrayBuffer, title: string, rights: boolean, onProgress?: (sent: number, checking: boolean) => void): Promise<AdminTrack> => {
    const send = async (url: string, init: RequestInit) => {
      const headers = new Headers(init.headers);
      headers.set('Accept', 'application/json');
      if (sessionToken) headers.set('Authorization', `Bearer ${sessionToken}`);
      const res = await fetch(url, { ...init, headers });
      const body = await res.json().catch(() => ({}));
      if (!res.ok) throw new ApiError(res.status, typeof body.error === 'string' ? body.error : 'activity__internal_error');
      return body;
    };
    let uploadId: string | null = null;
    for (let offset = 0; offset < bytes.byteLength; offset += UPLOAD_PART_BYTES) {
      const part = bytes.slice(offset, offset + UPLOAD_PART_BYTES);
      const q = new URLSearchParams({ offset: String(offset), total: String(bytes.byteLength) });
      if (uploadId) q.set('upload_id', uploadId);
      const r = await send(`/api/admin/music/upload/part?${q}`, { method: 'POST', headers: { 'Content-Type': 'application/octet-stream' }, body: part });
      uploadId = r.upload_id as string;
      onProgress?.((offset + part.byteLength) / bytes.byteLength, false);
    }
    if (!uploadId) throw new ApiError(400, 'activity__music_unknown_format');
    const q = new URLSearchParams({ upload_id: uploadId, title, rights: String(rights) });
    await send(`/api/admin/music/upload/finish?${q}`, { method: 'POST' });
    onProgress?.(1, true);
    // Vérification complète du fichier : quelques secondes à quelques minutes.
    for (;;) {
      await new Promise((r) => setTimeout(r, 2000));
      const st = await send(`/api/admin/music/upload/status?${new URLSearchParams({ upload_id: uploadId })}`, {});
      if (st.status === 'done') return st.track as AdminTrack;
      if (st.status === 'failed') throw new ApiError(422, typeof st.error === 'string' ? st.error : 'activity__internal_error');
    }
  },
};

// ---------------------------------------------------------------------------
// Panneau du mode admin : téléportation, objets, recettes (`activity/admin_panel.rs`).
// Le serveur revérifie les droits à chaque requête (D3 : modérateurs en lecture seule ;
// administrateurs en écriture et téléportation) ; `can_edit` ne sert qu'à griser l'interface.
// ---------------------------------------------------------------------------

export interface AdminPlayer {
  user_id: string;
  name: string;
  location: { kind: 'place' | 'road'; id: string; name: string | null } | null;
  moving: boolean;
  group_size: number;
}

export interface AdminPlayers {
  players: AdminPlayer[];
  places: { id: string; name: string }[];
  server_time: number;
}

export interface TeleportResponse {
  results: { user_id: string; ok: boolean; key: string }[];
  place: string;
  server_time: number;
}

export interface AdminItem {
  id: string;
  name: string;
  usage: ItemUsage;
  description: string | null;
  secret: string | null;
  image: string | null;
  inventory_size: number;
  /** Effets dans la syntaxe de `/item create` (« Stat: valeur durée niveau »). */
  effects_text: string;
  effects: ItemCard['effects'];
}

export interface AdminItems {
  items: AdminItem[];
  stats: string[];
  can_edit: boolean;
  server_time: number;
}

export type WikiAction = 'keep' | 'publish' | 'remove';

export interface ItemBody {
  /** Création seulement : le nom d'un objet ne change pas. */
  name?: string;
  usage: ItemUsage;
  description: string;
  secret: string;
  image: string;
  inventory_size: number;
  effects: string;
  wiki: WikiAction;
}

export interface AdminRecipeLine {
  item_id: string;
  name: string;
  quantity: number;
}

export interface AdminRecipe {
  id: string;
  name: string;
  /** Durée d'un exemplaire (s). */
  delay: number;
  ingredients: AdminRecipeLine[];
  results: AdminRecipeLine[];
  tools: AdminRecipeLine[];
  in_wiki: boolean;
}

export interface AdminRecipes {
  recipes: AdminRecipe[];
  items: { id: string; name: string }[];
  can_edit: boolean;
  server_time: number;
}

export interface RecipeBody {
  name: string;
  delay: number;
  ingredients: { item_id: string; quantity: number }[];
  results: { item_id: string; quantity: number }[];
  tools: string[];
  into_wiki: boolean;
}

const json = (method: string, body?: unknown): RequestInit => ({ method, body: body === undefined ? undefined : JSON.stringify(body) });

export const adminApi = {
  players: () => api<AdminPlayers>('/admin/players'),
  teleport: (userIds: string[], destinationId: string) =>
    api<TeleportResponse>('/admin/teleport', json('POST', { user_ids: userIds, destination_id: destinationId })),
  items: () => api<AdminItems>('/admin/items'),
  createItem: (body: ItemBody) => api<{ id: string; result: string }>('/admin/items', json('POST', body)),
  updateItem: (id: string, body: ItemBody) => api<{ id: string; result: string }>(`/admin/items/${encodeURIComponent(id)}`, json('PUT', body)),
  deleteItem: (id: string) => api<{ name: string; affected_recipes: string[] }>(`/admin/items/${encodeURIComponent(id)}`, json('DELETE')),
  recipes: () => api<AdminRecipes>('/admin/recipes'),
  createRecipe: (body: RecipeBody) => api<{ id: string; result: string }>('/admin/recipes', json('POST', body)),
  updateRecipe: (id: string, body: RecipeBody) => api<{ id: string; result: string }>(`/admin/recipes/${encodeURIComponent(id)}`, json('PUT', body)),
  deleteRecipe: (id: string) => api<{ name: string }>(`/admin/recipes/${encodeURIComponent(id)}`, json('DELETE')),
};

// ---------------------------------------------------------------------------
// Fiche d'un lieu (mode admin) et texte publié (joueurs) : `src/activity/place_admin.rs`
// ---------------------------------------------------------------------------

export interface BurgInfo {
  name: string | null;
  size: number;
  capital: boolean;
  port: boolean;
  population: number | null;
  state: string | null;
  culture: string | null;
  kind: string | null;
  group: string | null;
  features: string[];
}

export interface PlaceSheetView {
  id: string;
  name: string;
  server_id: string;
  role_id: string;
  central_channel_id: string | null;
  /** Texte enregistré (brouillon). */
  description: string | null;
  /** Dernier texte publié (celui que voient les joueurs). */
  published: { text: string; at: number } | null;
  posts: { server_id: string; thread_id: string; url: string }[];
  burg: BurgInfo | null;
  roads: { id: string; name: string; to_id: string; to_name: string | null; distance_km: number; secret: boolean }[];
  players: { user_id: string; name: string | null }[];
  players_total: number;
  loot: { category: boolean; central: boolean };
  /** Administrateur : modifier et publier (un modérateur ne fait que lire, D3). */
  can_edit: boolean;
  max_description: number;
  server_time: number;
}

export type PublishStatus = 'created' | 'updated' | 'recreated' | 'no_forum' | 'failed';

export interface PublishReport {
  server_id: string;
  server_name: string | null;
  status: PublishStatus;
  thread_id: string | null;
}

export interface LootItem {
  name: string;
  probability: number;
  min: number;
  max: number;
  stock: number | null;
  secret: boolean;
}

export type LootEntry = ({ kind: 'item' } & LootItem) | ({ kind: 'set'; items: LootItem[] } & LootItem);
export type LootTarget = 'category' | 'central';

export interface LootTableBody {
  entries: LootEntry[];
  rate_limit: number | null;
  delay: number | null;
}

export interface LootState {
  target: LootTarget;
  channel_id: string;
  central_available: boolean;
  exists: boolean;
  entries: LootEntry[];
  rate_limit: number | null;
  delay: number | null;
  raw_text: string;
  /** Objets de l'univers, triés : la liste où l'on choisit. */
  items: string[];
  can_edit: boolean;
  server_time: number;
}

export interface PlaceLore {
  id: string;
  name: string;
  text: string | null;
  published_at: number | null;
  burg: BurgInfo | null;
  /** Routes connues du joueur : non secrètes, vers un lieu de sa carte. */
  roads: { to_id: string; to_name: string; distance_km: number }[];
}

export const placeApi = {
  sheet: (id: string) => api<PlaceSheetView>(`/admin/places/${encodeURIComponent(id)}`),
  saveDescription: (id: string, description: string) =>
    api<PlaceSheetView>(`/admin/places/${encodeURIComponent(id)}/description`, { method: 'PUT', body: JSON.stringify({ description }) }),
  publish: (id: string) => api<{ sheet: PlaceSheetView; report: PublishReport[] }>(`/admin/places/${encodeURIComponent(id)}/publish`, { method: 'POST' }),
  loot: (id: string, target: LootTarget) => api<LootState>(`/admin/places/${encodeURIComponent(id)}/loot?target=${target}`),
  saveLoot: (id: string, target: LootTarget, body: LootTableBody) =>
    api<LootState>(`/admin/places/${encodeURIComponent(id)}/loot?target=${target}`, { method: 'PUT', body: JSON.stringify(body) }),
  lore: (id: string) => api<PlaceLore>(`/places/${encodeURIComponent(id)}/lore`),
};
