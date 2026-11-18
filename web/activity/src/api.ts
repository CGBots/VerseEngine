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
  universe: { id: string; name: string };
  access: 'player' | 'moderator' | 'admin';
  admin_mode_available: boolean;
  server_time: number;
}

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
