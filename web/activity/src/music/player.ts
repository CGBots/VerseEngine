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

/** Lecteur YouTube visible (cadre officiel) : le moteur ne fait que régler son volume. */
export interface VideoSink {
  /** Volume de 0 à 1. */
  setVolume(volume: number): void;
}

export interface PlaylistTrack {
  id: string;
  title: string;
  kind: 'file' | 'youtube';
  video_id?: string;
}

export interface PlaylistInput {
  /** Endroit (`place:<id>` ou `road:<id>`) ; `null` : nulle part, silence. */
  spot: string | null;
  tracks: PlaylistTrack[];
}

export interface PlayerState {
  /** Titre en cours ; `null` : rien ne joue. */
  title: string | null;
  /** Le navigateur a refusé de lancer le son sans geste de l'utilisateur. */
  blocked: boolean;
  /** Vidéo YouTube à afficher (lecteur visible) ; `null` : aucune. */
  video: string | null;
  /** La playlist de l'endroit contient des vidéos YouTube (la case « Masquer » a un sens). */
  hasVideos: boolean;
}

/**
 * Lecteur de l'activité : joue la playlist de l'endroit du joueur, en aléatoire et en boucle,
 * avec un fondu à chaque changement d'endroit. Fichiers audio : `load` renvoie une URL `blob:`
 * (téléchargée avec le jeton de session), libérée quand elle ne sert plus. Vidéos YouTube : lues
 * par le lecteur officiel, visible ; la case « Masquer » (ou un lecteur qui ne se charge pas) les
 * retire de la file, sans pause : seuls les fichiers audio restent.
 */
export class MusicPlayer {
  private readonly audio: AudioLike;
  private queue = new ShuffleQueue();
  private tracks = new Map<string, PlaylistTrack>();
  private all: PlaylistTrack[] = [];
  private spot: string | null = null;
  private current: string | null = null;
  private settings: MusicSettings = DEFAULT_SETTINGS;
  /** Le lecteur YouTube ne marche pas ici (Discord, réseau…) : comme « Masquer ». */
  private videoUnavailable = false;
  private fade = 0;
  private fadeTimer: ReturnType<typeof setInterval> | null = null;
  /** Incrémenté à chaque changement d'endroit : les chargements d'avant sont abandonnés. */
  private generation = 0;
  private cache = new Map<string, Promise<string>>();
  private failures = 0;
  private state: PlayerState = { title: null, blocked: false, video: null, hasVideos: false };
  private disposed = false;

  constructor(
    createAudio: () => AudioLike,
    private readonly load: (id: string) => Promise<string>,
    private readonly onState: (s: PlayerState) => void,
    private readonly revoke: (url: string) => void = (u) => URL.revokeObjectURL(u),
    private readonly video: VideoSink = { setVolume: () => {} },
  ) {
    this.audio = createAudio();
    this.audio.addEventListener('ended', () => {
      if (this.isFile(this.current)) void this.playNext(this.generation, false);
    });
    this.audio.addEventListener('error', () => {
      if (this.isFile(this.current)) this.skip(this.generation);
    });
  }

  private isFile(id: string | null): boolean {
    return !!id && this.tracks.get(id)?.kind === 'file';
  }

  private videosAllowed(): boolean {
    return !this.settings.videoHidden && !this.videoUnavailable;
  }

  /** Morceaux jouables ici : sans les vidéos si elles sont masquées ou indisponibles. */
  private playable(): string[] {
    return this.all.filter((t) => t.kind === 'file' || (this.videosAllowed() && t.video_id)).map((t) => t.id);
  }

  setSettings(s: MusicSettings): void {
    const visibilityChanged = s.videoHidden !== this.settings.videoHidden;
    this.settings = s;
    this.applyVolume();
    if (visibilityChanged) this.refilter();
  }

  /** La vidéo en cours est terminée. */
  videoEnded(): void {
    if (this.current && !this.isFile(this.current)) void this.playNext(this.generation, false);
  }

  /** Le lecteur YouTube ne se charge pas ici : on continue avec les fichiers audio seulement. */
  videoFailed(): void {
    if (this.videoUnavailable) return;
    this.videoUnavailable = true;
    this.refilter();
  }

  /** Change la file après « Masquer » ou un lecteur indisponible, sans pause. */
  private refilter(): void {
    this.queue.setTracks(this.playable());
    this.setState({ hasVideos: this.all.some((t) => t.kind === 'youtube') });
    if (this.current && !this.queue.has(this.current)) void this.playNext(this.generation, false);
    else if (!this.current && this.queue.size) void this.playNext(this.generation, true);
  }

  /** Nouvelle playlist : fondu vers la nouvelle si l'endroit change, sinon mise à jour en place. */
  setPlaylist(input: PlaylistInput): void {
    if (this.disposed) return;
    for (const t of input.tracks) this.tracks.set(t.id, t);
    this.all = input.tracks;
    const hasVideos = input.tracks.some((t) => t.kind === 'youtube');
    if (input.spot === this.spot) {
      this.setState({ hasVideos });
      this.refilter();
      if (this.current) this.setState({ title: this.tracks.get(this.current)?.title ?? null });
      return;
    }
    this.spot = input.spot;
    const gen = ++this.generation;
    this.queue = new ShuffleQueue();
    this.queue.setTracks(this.playable());
    this.setState({ hasVideos });
    this.fadeTo(0, () => {
      if (gen !== this.generation) return;
      this.stopCurrent();
      this.prune([]);
      if (this.queue.size) void this.playNext(gen, true);
      else this.setState({ title: null, video: null });
    });
  }

  /** Après un geste de l'utilisateur (bouton « Activer le son ») : nouvel essai. */
  unlock(): void {
    if (!this.isFile(this.current)) return;
    this.audio.play().then(() => this.setState({ blocked: false }), () => this.setState({ blocked: true }));
  }

  dispose(): void {
    this.disposed = true;
    this.generation++;
    if (this.fadeTimer) clearInterval(this.fadeTimer);
    this.stopCurrent();
    this.prune([]);
  }

  private stopCurrent(): void {
    this.audio.pause();
    this.current = null;
    this.setState({ video: null });
  }

  private setState(patch: Partial<PlayerState>): void {
    const next = { ...this.state, ...patch };
    if (next.title === this.state.title && next.blocked === this.state.blocked && next.video === this.state.video && next.hasVideos === this.state.hasVideos) return;
    this.state = next;
    this.onState(next);
  }

  private applyVolume(): void {
    const v = effectiveVolume(this.settings, this.fade);
    this.audio.volume = v;
    this.video.setVolume(v);
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
      this.fade = step > 0 ? Math.min(target, this.fade + step) : Math.max(target, this.fade + step);
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
    if (!id || gen !== this.generation || this.disposed) {
      if (!id && gen === this.generation) {
        this.stopCurrent();
        this.setState({ title: null });
      }
      return;
    }
    const track = this.tracks.get(id)!;
    const previous = this.current;
    if (track.kind === 'youtube' && track.video_id) {
      // Vidéo : le lecteur officiel (visible) la lance ; le moteur règle le volume.
      this.audio.pause();
      this.current = id;
      this.failures = 0;
      if (fadeIn) this.fade = 0;
      this.setState({ title: track.title, video: track.video_id, blocked: false });
      this.applyVolume();
      if (fadeIn) this.fadeTo(1);
      this.prune([previous].filter((x): x is string => !!x && this.isFile(x)));
      return;
    }
    let url: string;
    try {
      url = await this.url(id);
    } catch {
      return this.skip(gen);
    }
    if (gen !== this.generation || this.disposed) return;
    this.current = id;
    this.failures = 0;
    this.audio.src = url;
    if (fadeIn) this.fade = 0;
    this.applyVolume();
    this.setState({ title: track.title, video: null });
    try {
      await this.audio.play();
      this.setState({ blocked: false });
    } catch {
      this.setState({ blocked: true });
    }
    if (fadeIn) this.fadeTo(1);
    // Préchargement du suivant (fichier) ; on garde aussi le précédent.
    const upcoming = this.queue.peek();
    if (upcoming && upcoming !== id && this.isFile(upcoming)) this.url(upcoming).catch(() => {});
    this.prune([id, upcoming, previous].filter((x): x is string => !!x).slice(0, CACHE_SIZE));
  }

  /** Morceau illisible (supprimé entre-temps…) : on passe au suivant, sans boucler à l'infini. */
  private skip(gen: number): void {
    if (gen !== this.generation) return;
    this.failures += 1;
    if (this.failures > this.queue.size) {
      this.stopCurrent();
      this.setState({ title: null });
      return;
    }
    void this.playNext(gen, this.fade === 0);
  }
}
