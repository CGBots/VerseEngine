import type { CharacterView, MapView, TravelView } from '../api';
import { roadPath, type PlayerMotion } from './geometry';

/** Mouvement d'un groupe à partir de sa position (lieu ou route) et de son trajet, tels que le
 *  serveur les décrit à `serverTime`. `null` si un lieu manque sur la carte. */
export function motionOf(map: MapView, loc: CharacterView['location'], travel: TravelView | null, serverTime: number): PlayerMotion | null {
  if (!loc) return null;
  if (loc.kind === 'place') {
    const p = map.places.find((x) => x.id === loc.id);
    return p ? { kind: 'place', at: { x: p.x, y: p.y } } : null;
  }
  const from = map.places.find((x) => x.id === loc.source_id);
  const to = map.places.find((x) => x.id === loc.destination_id);
  if (!travel || !from || !to) return null;
  return {
    kind: 'road',
    from: { x: from.x, y: from.y },
    to: { x: to.x, y: to.y },
    // Tracé réel de la route (import Azgaar), sinon ligne droite.
    path: roadPath(map, from.id, to.id),
    traveledKm: travel.traveled_km,
    roadKm: travel.traveled_km + (travel.remaining_km ?? 0),
    speedKmh: travel.speed_kmh,
    refTime: serverTime,
    moving: travel.in_move,
  };
}

/** Autre groupe affiché sur la carte (mode spectateur). */
export interface OtherGroup {
  key: string;
  name: string;
  motion: PlayerMotion;
}
