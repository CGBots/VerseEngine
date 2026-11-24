/**
 * File de lecture « aléatoire, en boucle » (décision M3 de MUSIC_SPEC.md) : ordre mélangé, aucun
 * morceau répété avant que tous soient passés ; au tour suivant, le premier n'est jamais le
 * dernier joué (sauf s'il n'y a qu'un morceau).
 */
export class ShuffleQueue {
  private tracks: string[] = [];
  private bag: string[] = [];
  private last: string | null = null;

  constructor(private readonly random: () => number = Math.random) {}

  private shuffled(ids: string[]): string[] {
    const out = [...ids];
    for (let i = out.length - 1; i > 0; i--) {
      const j = Math.floor(this.random() * (i + 1));
      [out[i], out[j]] = [out[j], out[i]];
    }
    return out;
  }

  /** Nouvelle liste (même endroit, bibliothèque modifiée) : les morceaux retirés sortent du tour
   * en cours, les nouveaux y entrent à une place au hasard. */
  setTracks(ids: string[]): void {
    const unique = [...new Set(ids)];
    const added = unique.filter((id) => !this.tracks.includes(id));
    this.tracks = unique;
    this.bag = this.bag.filter((id) => unique.includes(id));
    for (const id of added) this.bag.splice(Math.floor(this.random() * (this.bag.length + 1)), 0, id);
  }

  has(id: string): boolean {
    return this.tracks.includes(id);
  }

  get size(): number {
    return this.tracks.length;
  }

  /** Morceau suivant ; `null` si la liste est vide. */
  next(): string | null {
    if (this.tracks.length === 0) return null;
    if (this.bag.length === 0) {
      this.bag = this.shuffled(this.tracks);
      if (this.bag.length > 1 && this.bag[0] === this.last) this.bag.push(this.bag.shift()!);
    }
    this.last = this.bag.shift()!;
    return this.last;
  }

  /** Le morceau qui suivra (sans le consommer), pour le précharger. */
  peek(): string | null {
    if (this.bag.length > 0) return this.bag[0];
    return this.tracks.length > 1 ? null : this.tracks[0] ?? null;
  }
}
