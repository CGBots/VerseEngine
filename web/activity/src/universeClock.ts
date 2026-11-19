/** Paramètres d'horloge envoyés par le serveur (`universe::time::universe_time_params`). */
export interface UniverseClockParams {
  origin: number;
  phase_duration_secs: number;
}

export type Phase = 'midnight' | 'sunrise' | 'noon' | 'sunset';
const PHASES: Phase[] = ['midnight', 'sunrise', 'noon', 'sunset'];

/**
 * Heure de l'univers à l'instant `now` (secondes Unix). Même calcul que le bot
 * (`universe::time::time_of_day`, commande `/universe_time`).
 */
export function timeOfDay(now: number, p: UniverseClockParams): { hours: number; minutes: number; phase: Phase } {
  const phaseSecs = Math.max(1, Math.floor(p.phase_duration_secs));
  const cycle = phaseSecs * 4;
  const elapsed = Math.max(0, Math.floor(now) - p.origin);
  const position = elapsed % cycle;
  const phase = PHASES[Math.floor(position / phaseSecs) % 4];
  const daySeconds = (position / cycle) * 86_400;
  return { hours: Math.floor(daySeconds / 3600), minutes: Math.floor((daySeconds % 3600) / 60), phase };
}

export function formatClock(hours: number, minutes: number): string {
  return `${String(hours).padStart(2, '0')}:${String(minutes).padStart(2, '0')}`;
}
