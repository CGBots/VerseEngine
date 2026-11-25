/** Position du lecteur de musique flottant, dans la carte (pixels, coin haut gauche). */
export interface DockPos {
  x: number;
  y: number;
}

/** Marge gardée avec les bords. */
export const DOCK_MARGIN = 8;
/** Distance sous laquelle le lecteur se colle à un bord (aimant). */
export const DOCK_SNAP = 48;

/** Garde le lecteur (w × h) dans la carte (W × H), à `DOCK_MARGIN` des bords au moins. */
export function clampDock(p: DockPos, w: number, h: number, W: number, H: number): DockPos {
  const maxX = Math.max(DOCK_MARGIN, W - w - DOCK_MARGIN);
  const maxY = Math.max(DOCK_MARGIN, H - h - DOCK_MARGIN);
  return { x: Math.min(Math.max(p.x, DOCK_MARGIN), maxX), y: Math.min(Math.max(p.y, DOCK_MARGIN), maxY) };
}

/** Au lâcher : dans la carte, et collé au bord le plus proche s'il est à moins de `DOCK_SNAP`. */
export function snapDock(p: DockPos, w: number, h: number, W: number, H: number): DockPos {
  const c = clampDock(p, w, h, W, H);
  const maxX = Math.max(DOCK_MARGIN, W - w - DOCK_MARGIN);
  const maxY = Math.max(DOCK_MARGIN, H - h - DOCK_MARGIN);
  const x = c.x - DOCK_MARGIN < DOCK_SNAP ? DOCK_MARGIN : maxX - c.x < DOCK_SNAP ? maxX : c.x;
  const y = c.y - DOCK_MARGIN < DOCK_SNAP ? DOCK_MARGIN : maxY - c.y < DOCK_SNAP ? maxY : c.y;
  return { x, y };
}

const KEY = 've.music.dock';

/** Position gardée par ce navigateur (confort du joueur) ; `null` : place par défaut. */
export function loadDock(): DockPos | null {
  try {
    const v = JSON.parse(localStorage.getItem(KEY) ?? 'null');
    return v && Number.isFinite(v.x) && Number.isFinite(v.y) ? { x: v.x, y: v.y } : null;
  } catch {
    return null;
  }
}

export function saveDock(p: DockPos): void {
  try {
    localStorage.setItem(KEY, JSON.stringify(p));
  } catch {
    // Stockage indisponible : la position ne sera pas retenue.
  }
}
