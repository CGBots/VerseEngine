/** Réglages du son, propres à chaque joueur et gardés dans ce navigateur seulement. */
export interface MusicSettings {
  /** 0 à 1. */
  volume: number;
  muted: boolean;
  /** Lecteur YouTube masqué : seuls les fichiers audio sont joués. */
  videoHidden: boolean;
}

const KEY = 'verseengine.music';
export const DEFAULT_SETTINGS: MusicSettings = { volume: 0.5, muted: false, videoHidden: false };

/** Lecture tolérante : valeur absente, illisible ou hors bornes → réglage par défaut. */
export function parseSettings(raw: string | null): MusicSettings {
  try {
    const v = JSON.parse(raw ?? '') as Partial<MusicSettings>;
    const volume = typeof v.volume === 'number' && Number.isFinite(v.volume) ? Math.min(1, Math.max(0, v.volume)) : DEFAULT_SETTINGS.volume;
    return { volume, muted: v.muted === true, videoHidden: v.videoHidden === true };
  } catch {
    return { ...DEFAULT_SETTINGS };
  }
}

export function loadSettings(): MusicSettings {
  try {
    return parseSettings(localStorage.getItem(KEY));
  } catch {
    return { ...DEFAULT_SETTINGS };
  }
}

export function saveSettings(s: MusicSettings): void {
  try {
    localStorage.setItem(KEY, JSON.stringify(s));
  } catch {
    // Stockage indisponible (navigation privée…) : le réglage vaut pour cette visite seulement.
  }
}

/** Volume réel : réglage du joueur × position du fondu (0 à 1), avec une courbe perçue plus douce. */
export function effectiveVolume(s: MusicSettings, fade: number): number {
  if (s.muted) return 0;
  const f = Math.min(1, Math.max(0, fade));
  return Math.min(1, Math.max(0, s.volume * s.volume * f));
}
