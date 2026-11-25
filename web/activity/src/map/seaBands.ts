// Contours lissés (marching squares + Chaikin) d'un champ scalaire sur grille régulière.

const LOW = -1e30; // valeur du bord virtuel (hors grille)

// Segments par cas (coins : 1=hg, 2=hd, 4=bd, 8=bg), arêtes : 0=haut, 1=droite, 2=bas, 3=gauche
const CASES: number[][] = [
  [], [3, 0], [0, 1], [3, 1], [1, 2], [], [0, 2], [3, 2],
  [3, 2], [0, 2], [], [1, 2], [3, 1], [0, 1], [3, 0], [],
];

/** Contours fermés de la région `field >= level` sur une grille `cols × rows` (valeur au centre
 *  de chaque case), en coordonnées carte : la case (c, r) a son centre en
 *  ((c + 0.5) * width / cols, (r + 0.5) * height / rows). */
export function isoBands(
  field: Float32Array | number[],
  cols: number,
  rows: number,
  levels: number[],
  width: number,
  height: number,
): { level: number; d: string }[] {
  const W = cols + 2;
  const H = rows + 2;
  // Champ bordé d'un anneau de très basses valeurs
  const g = new Float32Array(W * H).fill(LOW);
  for (let r = 0; r < rows; r++) {
    for (let c = 0; c < cols; c++) g[(r + 1) * W + c + 1] = field[r * cols + c];
  }
  // Positions des nœuds, bornées au rectangle de la carte
  const px = new Float64Array(W);
  const py = new Float64Array(H);
  for (let i = 0; i < W; i++) px[i] = Math.min(width, Math.max(0, ((i - 1 + 0.5) * width) / cols));
  for (let j = 0; j < H; j++) py[j] = Math.min(height, Math.max(0, ((j - 1 + 0.5) * height) / rows));

  const nEdges = 2 * W * H;
  const e0 = new Int32Array(nEdges);
  const e1 = new Int32Array(nEdges);
  const out: { level: number; d: string }[] = [];

  for (const level of levels) {
    e0.fill(-1);
    e1.fill(-1);
    const sa: number[] = [];
    const sb: number[] = [];
    const addSeg = (a: number, b: number) => {
      const s = sa.length;
      sa.push(a);
      sb.push(b);
      if (e0[a] < 0) e0[a] = s; else e1[a] = s;
      if (e0[b] < 0) e0[b] = s; else e1[b] = s;
    };

    for (let j = 0; j < H - 1; j++) {
      for (let i = 0; i < W - 1; i++) {
        const v0 = g[j * W + i];
        const v1 = g[j * W + i + 1];
        const v2 = g[(j + 1) * W + i + 1];
        const v3 = g[(j + 1) * W + i];
        let m = 0;
        if (v0 >= level) m |= 1;
        if (v1 >= level) m |= 2;
        if (v2 >= level) m |= 4;
        if (v3 >= level) m |= 8;
        if (m === 0 || m === 15) continue;
        // clés d'arêtes : haut, droite, bas, gauche
        const k = [
          2 * (j * W + i),
          2 * (j * W + i + 1) + 1,
          2 * ((j + 1) * W + i),
          2 * (j * W + i) + 1,
        ];
        if (m === 5 || m === 10) {
          const centerIn = (v0 + v1 + v2 + v3) / 4 >= level;
          // 5 : hg+bd dedans ; 10 : hd+bg dedans
          const joinIn = (m === 5) === centerIn; // vrai : coins hors isolés (T-R, L-B) pour 5
          if (m === 5) {
            if (joinIn) { addSeg(k[0], k[1]); addSeg(k[3], k[2]); }
            else { addSeg(k[3], k[0]); addSeg(k[1], k[2]); }
          } else if (centerIn) { addSeg(k[3], k[0]); addSeg(k[1], k[2]); }
          else { addSeg(k[0], k[1]); addSeg(k[3], k[2]); }
          continue;
        }
        const cs = CASES[m];
        addSeg(k[cs[0]], k[cs[1]]);
      }
    }
    if (sa.length === 0) continue;

    // Point d'une arête (interpolation linéaire, collé au bord virtuel si besoin)
    const edgePoint = (key: number, res: number[]) => {
      const vert = key & 1;
      const n = key >> 1;
      const j = Math.floor(n / W);
      const i = n - j * W;
      const n2 = vert ? n + W : n + 1;
      const va = g[n];
      const vb = g[n2];
      let t: number;
      if (va <= LOW) t = 0;
      else if (vb <= LOW) t = 1;
      else t = (level - va) / (vb - va);
      if (!(t >= 0)) t = 0; else if (t > 1) t = 1;
      const x0 = px[i], y0 = py[j];
      const x1 = vert ? x0 : px[i + 1];
      const y1 = vert ? py[j + 1] : y0;
      res[0] = x0 + (x1 - x0) * t;
      res[1] = y0 + (y1 - y0) * t;
    };

    const used = new Uint8Array(sa.length);
    const tmp = [0, 0];
    let d = '';
    for (let s0 = 0; s0 < sa.length; s0++) {
      if (used[s0]) continue;
      // Parcours de la boucle
      const keys: number[] = [sa[s0]];
      let seg = s0;
      let cur = sb[s0];
      used[s0] = 1;
      while (cur !== keys[0]) {
        keys.push(cur);
        let nx = e0[cur];
        if (nx === seg || used[nx]) nx = e1[cur];
        if (nx < 0 || used[nx]) break; // boucle non fermée (ne devrait pas arriver)
        used[nx] = 1;
        seg = nx;
        cur = sa[nx] === cur ? sb[nx] : sa[nx];
      }
      if (keys.length < 4) continue;
      let pts = new Float64Array(keys.length * 2);
      for (let q = 0; q < keys.length; q++) {
        edgePoint(keys[q], tmp);
        pts[2 * q] = tmp[0];
        pts[2 * q + 1] = tmp[1];
      }
      // Chaikin fermé, 2 passes
      for (let pass = 0; pass < 2; pass++) {
        const n = pts.length / 2;
        const np = new Float64Array(n * 4);
        for (let q = 0; q < n; q++) {
          const a = 2 * q;
          const b = 2 * ((q + 1) % n);
          np[4 * q] = 0.75 * pts[a] + 0.25 * pts[b];
          np[4 * q + 1] = 0.75 * pts[a + 1] + 0.25 * pts[b + 1];
          np[4 * q + 2] = 0.25 * pts[a] + 0.75 * pts[b];
          np[4 * q + 3] = 0.25 * pts[a + 1] + 0.75 * pts[b + 1];
        }
        pts = np;
      }
      for (let q = 0; q < pts.length; q += 2) {
        d += (q === 0 ? 'M' : 'L') + Math.round(pts[q] * 10) / 10 + ' ' + Math.round(pts[q + 1] * 10) / 10;
      }
      d += 'Z';
    }
    if (d) out.push({ level, d });
  }
  return out;
}
