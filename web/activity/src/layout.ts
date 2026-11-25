import { useEffect, useState } from 'react';

/**
 * Mise en page selon la place disponible (pas selon l'appareil) : une seule interface partout.
 * - `wide` : assez large et assez haut pour garder la fiche du personnage en colonne à côté de
 *   la carte (ordinateur, Discord sur PC, Z Fold déplié en paysage, tablette en paysage) ;
 * - `compact` : la même fiche s'ouvre en volet par-dessus la carte (téléphones, Z Fold plié ou
 *   déplié en portrait, écrans très bas en paysage).
 */
export type LayoutMode = 'wide' | 'compact';

/** Largeur minimale pour une colonne de fiche (300 px) et une carte encore confortable. */
export const WIDE_MIN_WIDTH = 1000;
/** En dessous, la colonne serait trop basse pour la fiche (paysage d'un téléphone). */
export const WIDE_MIN_HEIGHT = 520;
/** En dessous, les panneaux flottants prennent toute la largeur. */
export const NARROW_MAX_WIDTH = 600;

export function layoutFor(width: number, height: number): LayoutMode {
  return width >= WIDE_MIN_WIDTH && height >= WIDE_MIN_HEIGHT ? 'wide' : 'compact';
}

/** Mise en page courante, recalculée quand la fenêtre change (rotation, dépliage). */
export function useLayout(): LayoutMode {
  const get = () => (typeof window === 'undefined' ? 'wide' : layoutFor(window.innerWidth, window.innerHeight));
  const [mode, setMode] = useState<LayoutMode>(get);
  useEffect(() => {
    const on = () => setMode(get());
    window.addEventListener('resize', on);
    return () => window.removeEventListener('resize', on);
  }, []);
  return mode;
}
