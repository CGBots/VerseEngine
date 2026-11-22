import { ShuffleQueue } from './shuffle';
import { DEFAULT_SETTINGS, effectiveVolume, type MusicSettings } from './settings';

/** Durée d'un fondu (changement d'endroit). */
export const FADE_MS = 2000;
const FADE_STEP_MS = 50;
/** Fichiers gardés en mémoire (morceau en cours, suivant, précédent). */
const CACHE_SIZE = 3;

/** Ce que le lecteur utilise d'un élément `<audio>` (remplaçable dans les tests). */
export interface AudioLike {
  src: string;
  volume: number;
  play(): Promise<void>;
  pause(): void;
  addEventListener(type: 'ended' | 'error', listener: () => void): void;
}

export interface PlaylistInput {
  /** Endroit (`place:<id>` ou `road:<id>`) ; `null` : nulle part, silence. */
  spot: string | null;
  tracks: { id: string; title: string }[];
}

export interface PlayerState {
  /** Titre en cours ; `null` : rien ne joue. */
  title: string | null;
  /** Le navigateur a refusé de lancer le son sans geste de l'utilisateur. */
  blocked: boolean;
}

/**
 * Lecteur de l'activité : joue la playlist de l'endroit du joueur, en aléatoire et en boucle,
 * avec un fondu à chaque changement d'endroit. `load` renvoie une URL `blob:` du morceau
 * (téléchargé avec le jeton de session) ; le lecteur la libère quand il n'en a plus besoin.
 */
export class MusicPlayer {
  private readonly audio: AudioLike;
  private queue = new ShuffleQueue();
  private titles = new Map<string, string>();
  private spot: string | null = null;
  private current: string | null = null;
  private settings: MusicSettings = DEFAULT_SETTINGS;
  private fade = 0;
  private fadeTimer: ReturnType<typeof setInterval> | null = null;
  /** Incrémenté à chaque changement d'endroit : les chargements d'avant sont abandonnés. */
  private generation = 0;
  private cache = new Map<string, Promise<string>>();
  private failures = 0;
  private state: PlayerState = { title: null, blocked: false };
  private disposed = false;

  constructor(
    createAudio: () => AudioLike,
    private readonly load: (id: string) => Promise<string>,
    private readonly onState: (s: PlayerState) => void,
    private readonly revoke: (url: string) => void = (u) => URL.revokeObjectURL(u),
  ) {
    this.audio = createAudio();
    this.audio.addEventListener('ended', () => void this.playNext(this.generation, false));
    this.audio.addEventListener('error', () => this.skip(this.generation));
  }

  setSettings(s: MusicSettings): void {
    this.settings = s;
    this.applyVolume();
  }

  /** Nouvelle playlist : fondu vers la nouvelle si l'endroit change, sinon mise à jour en place. */
  setPlaylist(input: PlaylistInput): void {
    if (this.disposed) return;
    for (const t of input.tracks) this.titles.set(t.id, t.title);
    const ids = input.tracks.map((t) => t.id);
    if (input.spot === this.spot) {
      this.queue.setTracks(ids);
      if (this.current && !this.queue.has(this.current)) void this.playNext(this.generation, false);
      else if (!this.current && ids.length) void this.playNext(this.generation, true);
      else if (this.current) this.setState({ title: this.titles.get(this.current) ?? null });
      return;
    }
    this.spot = input.spot;
    const gen = ++this.generation;
    this.queue = new ShuffleQueue();
    this.queue.setTracks(ids);
    this.fadeTo(0, () => {
      if (gen !== this.generation) return;
      this.audio.pause();
      this.current = null;
      this.prune([]);
      if (ids.length) void this.playNext(gen, true);
      else this.setState({ title: null });
    });
  }

  /** Après un geste de l'utilisateur (bouton « Activer le son ») : nouvel essai. */
  unlock(): void {
    if (!this.current) return;
    this.audio.play().then(() => this.setState({ blocked: false }), () => this.setState({ blocked: true }));
  }

  dispose(): void {
    this.disposed = true;
    this.generation++;
    if (this.fadeTimer) clearInterval(this.fadeTimer);
    this.audio.pause();
    this.prune([]);
  }

  private setState(patch: Partial<PlayerState>): void {
    const next = { ...this.state, ...patch };
    if (next.title === this.state.title && next.blocked === this.state.blocked) return;
    this.state = next;
    this.onState(next);
  }

  private applyVolume(): void {
    this.audio.volume = effectiveVolume(this.settings, this.fade);
  }

  private fadeTo(target: number, done?: () => void): void {
    if (this.fadeTimer) clearInterval(this.fadeTimer);
    const step = (FADE_STEP_MS / FADE_MS) * Math.sign(target - this.fade);
    if (step === 0 || !this.current) {
      this.fade = target;
      this.applyVolume();
      done?.();
      return;
    }
    this.fadeTimer = setInterval(() => {
      this.fade = step > 0 ? Math.min(target, this.fade + step) : Math.max(target, this.fade - step * -1);
      this.applyVolume();
      if (this.fade === target) {
        clearInterval(this.fadeTimer!);
        this.fadeTimer = null;
        done?.();
      }
    }, FADE_STEP_MS);
  }

  private url(id: string): Promise<string> {
    let p = this.cache.get(id);
    if (!p) {
      p = this.load(id);
      this.cache.set(id, p);
      p.catch(() => this.cache.delete(id));
    }
    return p;
  }

  /** Libère les fichiers dont on n'a plus besoin. */
  private prune(keep: string[]): void {
    for (const [id, p] of this.cache) {
      if (keep.includes(id)) continue;
      this.cache.delete(id);
      p.then(this.revoke, () => {});
    }
  }

  private async playNext(gen: number, fadeIn: boolean): Promise<void> {
    const id = this.queue.next();
    if (!id || gen !== this.generation || this.disposed) return;
    let url: string;
    try {
      url = await this.url(id);
    } catch {
      return this.skip(gen);
    }
    if (gen !== this.generation || this.disposed) return;
    const previous = this.current;
    this.current = id;
    this.failures = 0;
    this.audio.src = url;
    if (fadeIn) this.fade = 0;
    this.applyVolume();
    this.setState({ title: this.titles.get(id) ?? null });
    try {
      await this.audio.play();
      this.setState({ blocked: false });
    } catch {
      this.setState({ blocked: true });
    }
    if (fadeIn) this.fadeTo(1);
    // Préchargement du suivant ; on garde aussi le précédent (relecture immédiate en boucle).
    const upcoming = this.queue.peek();
    if (upcoming && upcoming !== id) this.url(upcoming).catch(() => {});
    this.prune([id, upcoming, previous].filter((x): x is string => !!x).slice(0, CACHE_SIZE));
  }

  /** Morceau illisible (supprimé entre-temps…) : on passe au suivant, sans boucler à l'infini. */
  private skip(gen: number): void {
    if (gen !== this.generation) return;
    this.failures += 1;
    if (this.failures > this.queue.size) {
      this.current = null;
      this.setState({ title: null });
      return;
    }
    void this.playNext(gen, this.fade === 0);
  }
}
