import type { CraftState, FarmState, TravelState } from './api';

/**
 * Ce que fait le joueur. Voyage, craft et farm s'excluent : le serveur le garantit
 * (`src/occupation.rs`), l'interface ne fait que refléter son état relu par l'API.
 */
export type Occupation = 'travel' | 'craft' | 'farm' | null;

/** Occupation en cours d'après l'état serveur. Un groupe arrêté sur une route ne voyage pas. */
export function currentOccupation(travel: TravelState | null, craft: CraftState | null, farm: FarmState | null): Occupation {
  if (travel?.travel?.in_move) return 'travel';
  if (craft?.craft) return 'craft';
  if (farm?.farm) return 'farm';
  return null;
}

export interface Availability {
  /** Ouvrir la fenêtre de voyage (grisé pendant le voyage, le craft et le farm). */
  travel: boolean;
  /** Ouvrir le livre de recettes (reste ouvert pendant un craft, pour le suivre). */
  craft: boolean;
  /** Consulter l'inventaire : toujours permis (simple lecture, même en voyage, craft ou farm). */
  inventory: boolean;
  /** Lancer un farm. */
  farm: boolean;
  /** Lancer une recette depuis le livre. */
  startCraft: boolean;
}

/** Boutons utilisables. `busy` : une requête est en cours (pas de double clic). */
export function availability(occupation: Occupation, busy: boolean): Availability {
  const free = occupation === null && !busy;
  return {
    travel: free,
    craft: occupation === null || occupation === 'craft',
    inventory: true,
    farm: free,
    startCraft: free,
  };
}

/** Avancement entre `start` et `end` (secondes serveur), borné à [0, 1]. */
export function progress(start: number, end: number, now: number): number {
  if (!(end > start)) return now >= end ? 1 : 0;
  return Math.min(1, Math.max(0, (now - start) / (end - start)));
}

/** Quantité saisie ramenée dans [1, max] (entier). `max` ≤ 0 : rien n'est faisable. */
export function clampQuantity(value: number, max: number): number {
  if (max < 1) return 0;
  if (!Number.isFinite(value)) return 1;
  return Math.min(max, Math.max(1, Math.floor(value)));
}
