// Échange avec Azgaar (étape E9), servi sous /azgaar/ dans un bac à sable (origine opaque, voir
// AZGAAR_CSP dans src/activity/mod.rs). Azgaar est traité comme une page étrangère : on ne lit
// que les messages de son cadre, et on n'en accepte qu'une carte, qui passe ensuite par le même
// envoi et les mêmes contrôles serveur qu'un fichier `.map` choisi à la main.

/** Le fichier lui-même : en développement, `/azgaar/` renverrait la page de l'activité. */
export const AZGAAR_PATH = '/azgaar/index.html';
/** Attribut `sandbox` du cadre (le serveur impose déjà le même bac à sable par la CSP). */
export const AZGAAR_SANDBOX = 'allow-scripts allow-downloads allow-modals';
/** Taille max acceptée par le serveur. */
export const MAX_MAP_BYTES = 32 * 1024 * 1024;

export type AzgaarMessage =
  | { kind: 'ready' }
  | { kind: 'map'; id: number; data: string }
  | { kind: 'error'; id: number; message: string };

/** Demande de la carte affichée ; `id` relie la réponse à la demande. */
export const exportRequest = (id: number) => ({ type: 'verseengine:export-request', id });

/**
 * Message reçu, s'il vient bien du cadre d'Azgaar et a la forme attendue ; sinon `null`.
 * `frame` : la fenêtre du cadre (`iframe.contentWindow`).
 */
export function parseAzgaarMessage(event: { source: unknown; data: unknown }, frame: unknown): AzgaarMessage | null {
  if (!frame || event.source !== frame) return null;
  const d = event.data;
  if (!d || typeof d !== 'object') return null;
  const m = d as Record<string, unknown>;
  if (m.type === 'verseengine:ready') return { kind: 'ready' };
  if (typeof m.id !== 'number' || !Number.isSafeInteger(m.id)) return null;
  if (m.type === 'verseengine:map' && typeof m.data === 'string') return { kind: 'map', id: m.id, data: m.data };
  if (m.type === 'verseengine:error') return { kind: 'error', id: m.id, message: typeof m.message === 'string' ? m.message.slice(0, 200) : '' };
  return null;
}

/** Contenu du `.map` à envoyer au serveur ; `null` s'il est vide ou trop gros. */
export function mapBytes(data: string): Uint8Array | null {
  // Une unité UTF-16 donne au plus 3 octets UTF-8 : refus immédiat d'un texte manifestement trop long.
  if (!data || data.length > MAX_MAP_BYTES) return null;
  const bytes = new TextEncoder().encode(data);
  return bytes.length > MAX_MAP_BYTES ? null : bytes;
}
