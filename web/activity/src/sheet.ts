import { useEffect, useState } from 'react';

/** Distance (px) à tirer pour ouvrir ou fermer la fiche. */
export const PULL_THRESHOLD = 40;

/**
 * Geste sur la fiche du personnage (téléphone) : tirer vers le bas l'ouvre, la remonter la
 * ferme. `null` tant que le doigt n'a pas parcouru assez de chemin dans le bon sens.
 */
export function pullDecision(startY: number, currentY: number, open: boolean): 'open' | 'close' | null {
  const dy = currentY - startY;
  if (!open && dy >= PULL_THRESHOLD) return 'open';
  if (open && dy <= -PULL_THRESHOLD) return 'close';
  return null;
}

/** Vrai sur un écran étroit (téléphone) : la fiche devient un panneau qui se déroule du haut. */
export function useIsPhone(query = '(max-width: 640px)'): boolean {
  const get = () => typeof matchMedia === 'function' && matchMedia(query).matches;
  const [phone, setPhone] = useState(get);
  useEffect(() => {
    if (typeof matchMedia !== 'function') return;
    const mq = matchMedia(query);
    const on = () => setPhone(mq.matches);
    mq.addEventListener('change', on);
    return () => mq.removeEventListener('change', on);
  }, [query]);
  return phone;
}
