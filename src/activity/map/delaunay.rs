//! Triangulation de Delaunay (portage de l'algorithme de `delaunator`, celui qu'utilise Azgaar).
//!
//! Sert à reconstruire le graphe « pack » d'une sauvegarde Azgaar : le `.map` ne garde que les
//! points de la grille et la donnée de chaque cellule (biome, État…), pas les cellules du pack ni
//! leurs positions, qu'Azgaar recalcule au chargement à partir des voisins de Voronoï de la grille.
//! Seuls les **voisins** servent (ils ne dépendent pas de l'ordre interne de la triangulation,
//! hors cas dégénérés), d'où un portage simple, sans prédicats exacts.

use std::cmp::Ordering;

const EPSILON: f64 = f64::EPSILON;

/// Triangulation : `triangles[3t..3t+3]` sont les sommets du triangle `t` (sens trigonométrique,
/// y vers le bas) ; `halfedges[e]` est la demi-arête opposée, ou `-1` sur l'enveloppe convexe.
#[derive(Debug, Clone)]
pub struct Delaunay {
    pub triangles: Vec<usize>,
    pub halfedges: Vec<isize>,
}

fn dist2(ax: f64, ay: f64, bx: f64, by: f64) -> f64 {
    let (dx, dy) = (ax - bx, ay - by);
    dx * dx + dy * dy
}

fn orient_if_sure(px: f64, py: f64, rx: f64, ry: f64, qx: f64, qy: f64) -> f64 {
    let l = (ry - py) * (qx - px);
    let r = (rx - px) * (qy - py);
    if (l - r).abs() >= 3.330_669_073_875_471_6e-16 * (l + r).abs() { l - r } else { 0.0 }
}

/// Vrai si (r, q, p) tournent dans le sens horaire (convention de delaunator).
fn orient(rx: f64, ry: f64, qx: f64, qy: f64, px: f64, py: f64) -> bool {
    let a = orient_if_sure(px, py, rx, ry, qx, qy);
    let v = if a != 0.0 {
        a
    } else {
        let b = orient_if_sure(rx, ry, qx, qy, px, py);
        if b != 0.0 { b } else { orient_if_sure(qx, qy, px, py, rx, ry) }
    };
    v < 0.0
}

#[allow(clippy::too_many_arguments)]
fn in_circle(ax: f64, ay: f64, bx: f64, by: f64, cx: f64, cy: f64, px: f64, py: f64) -> bool {
    let (dx, dy, ex, ey, fx, fy) = (ax - px, ay - py, bx - px, by - py, cx - px, cy - py);
    let ap = dx * dx + dy * dy;
    let bp = ex * ex + ey * ey;
    let cp = fx * fx + fy * fy;
    dx * (ey * cp - bp * fy) - dy * (ex * cp - bp * fx) + ap * (ex * fy - ey * fx) < 0.0
}

fn circumradius(ax: f64, ay: f64, bx: f64, by: f64, cx: f64, cy: f64) -> f64 {
    let (dx, dy, ex, ey) = (bx - ax, by - ay, cx - ax, cy - ay);
    let bl = dx * dx + dy * dy;
    let cl = ex * ex + ey * ey;
    let d = 0.5 / (dx * ey - dy * ex);
    let x = (ey * bl - dy * cl) * d;
    let y = (dx * cl - ex * bl) * d;
    let r = x * x + y * y;
    if r.is_finite() { r } else { f64::INFINITY }
}

fn circumcenter(ax: f64, ay: f64, bx: f64, by: f64, cx: f64, cy: f64) -> (f64, f64) {
    let (dx, dy, ex, ey) = (bx - ax, by - ay, cx - ax, cy - ay);
    let bl = dx * dx + dy * dy;
    let cl = ex * ex + ey * ey;
    let d = 0.5 / (dx * ey - dy * ex);
    (ax + (ey * bl - dy * cl) * d, ay + (dx * cl - ex * bl) * d)
}

fn pseudo_angle(dx: f64, dy: f64) -> f64 {
    let p = dx / (dx.abs() + dy.abs());
    (if dy > 0.0 { 3.0 - p } else { 1.0 + p }) / 4.0
}

struct Builder<'a> {
    pts: &'a [(f64, f64)],
    triangles: Vec<usize>,
    halfedges: Vec<isize>,
    hull_prev: Vec<usize>,
    hull_next: Vec<usize>,
    hull_tri: Vec<usize>,
    hull_hash: Vec<isize>,
    hull_start: usize,
    cx: f64,
    cy: f64,
}

impl Builder<'_> {
    fn hash_key(&self, x: f64, y: f64) -> usize {
        let n = self.hull_hash.len();
        ((pseudo_angle(x - self.cx, y - self.cy) * n as f64).floor() as usize) % n
    }

    fn link(&mut self, a: usize, b: isize) {
        self.halfedges[a] = b;
        if b >= 0 {
            self.halfedges[b as usize] = a as isize;
        }
    }

    fn add_triangle(&mut self, i0: usize, i1: usize, i2: usize, a: isize, b: isize, c: isize) -> usize {
        let t = self.triangles.len();
        self.triangles.extend([i0, i1, i2]);
        self.halfedges.extend([-1, -1, -1]);
        self.link(t, a);
        self.link(t + 1, b);
        self.link(t + 2, c);
        t
    }

    fn legalize(&mut self, mut a: usize) -> usize {
        let mut stack: Vec<usize> = Vec::new();
        let mut ar;
        loop {
            let b = self.halfedges[a];
            let a0 = a - a % 3;
            ar = a0 + (a + 2) % 3;
            if b < 0 {
                match stack.pop() {
                    Some(next) => {
                        a = next;
                        continue;
                    }
                    None => break,
                }
            }
            let b = b as usize;
            let b0 = b - b % 3;
            let al = a0 + (a + 1) % 3;
            let bl = b0 + (b + 2) % 3;
            let (p0, pr, pl, p1) = (self.triangles[ar], self.triangles[a], self.triangles[al], self.triangles[bl]);
            let p = |i: usize| self.pts[i];
            let illegal = in_circle(p(p0).0, p(p0).1, p(pr).0, p(pr).1, p(pl).0, p(pl).1, p(p1).0, p(p1).1);
            if illegal {
                self.triangles[a] = p1;
                self.triangles[b] = p0;
                let hbl = self.halfedges[bl];
                // Arête basculée de l'autre côté de l'enveloppe (rare) : on corrige la référence.
                if hbl < 0 {
                    let mut e = self.hull_start;
                    loop {
                        if self.hull_tri[e] == bl {
                            self.hull_tri[e] = a;
                            break;
                        }
                        e = self.hull_prev[e];
                        if e == self.hull_start {
                            break;
                        }
                    }
                }
                self.link(a, hbl);
                let har = self.halfedges[ar];
                self.link(b, har);
                self.link(ar, bl as isize);
                let br = b0 + (b + 1) % 3;
                if stack.len() < 1 << 16 {
                    stack.push(br);
                }
            } else {
                match stack.pop() {
                    Some(next) => a = next,
                    None => break,
                }
            }
        }
        ar
    }
}

/// Triangule `pts`. Moins de trois points, ou tous alignés : aucune triangulation (vide).
pub fn triangulate(pts: &[(f64, f64)]) -> Delaunay {
    let n = pts.len();
    let empty = Delaunay { triangles: vec![], halfedges: vec![] };
    if n < 3 {
        return empty;
    }
    let (mut min_x, mut min_y, mut max_x, mut max_y) = (f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY);
    for &(x, y) in pts {
        min_x = min_x.min(x);
        min_y = min_y.min(y);
        max_x = max_x.max(x);
        max_y = max_y.max(y);
    }
    let (cx, cy) = ((min_x + max_x) / 2.0, (min_y + max_y) / 2.0);
    // Graine : le point le plus proche du centre, son plus proche voisin, puis le point qui forme
    // avec eux le plus petit cercle circonscrit.
    let closest = |from: (f64, f64), skip: &[usize]| {
        (0..n)
            .filter(|i| !skip.contains(i))
            .map(|i| (i, dist2(from.0, from.1, pts[i].0, pts[i].1)))
            .filter(|&(_, d)| !skip.is_empty() || d >= 0.0)
            .filter(|&(_, d)| skip.is_empty() || d > 0.0)
            .min_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(Ordering::Equal))
            .map(|(i, _)| i)
    };
    let Some(i0) = closest((cx, cy), &[]) else { return empty };
    let Some(mut i1) = closest(pts[i0], &[i0]) else { return empty };
    let mut i2 = usize::MAX;
    let mut min_radius = f64::INFINITY;
    for i in 0..n {
        if i == i0 || i == i1 {
            continue;
        }
        let r = circumradius(pts[i0].0, pts[i0].1, pts[i1].0, pts[i1].1, pts[i].0, pts[i].1);
        if r < min_radius {
            i2 = i;
            min_radius = r;
        }
    }
    if !min_radius.is_finite() {
        return empty; // tous alignés
    }
    if orient(pts[i0].0, pts[i0].1, pts[i1].0, pts[i1].1, pts[i2].0, pts[i2].1) {
        std::mem::swap(&mut i1, &mut i2);
    }
    let (ccx, ccy) = circumcenter(pts[i0].0, pts[i0].1, pts[i1].0, pts[i1].1, pts[i2].0, pts[i2].1);
    let mut ids: Vec<usize> = (0..n).collect();
    let dists: Vec<f64> = pts.iter().map(|&(x, y)| dist2(x, y, ccx, ccy)).collect();
    ids.sort_by(|&a, &b| dists[a].partial_cmp(&dists[b]).unwrap_or(Ordering::Equal).then(a.cmp(&b)));

    let hash_size = (n as f64).sqrt().ceil().max(1.0) as usize;
    let mut bld = Builder {
        pts,
        triangles: Vec::with_capacity(6 * n),
        halfedges: Vec::with_capacity(6 * n),
        hull_prev: vec![0; n],
        hull_next: vec![0; n],
        hull_tri: vec![0; n],
        hull_hash: vec![-1; hash_size],
        hull_start: i0,
        cx: ccx,
        cy: ccy,
    };
    bld.hull_next[i0] = i1;
    bld.hull_prev[i2] = i1;
    bld.hull_next[i1] = i2;
    bld.hull_prev[i0] = i2;
    bld.hull_next[i2] = i0;
    bld.hull_prev[i1] = i0;
    bld.hull_tri[i0] = 0;
    bld.hull_tri[i1] = 1;
    bld.hull_tri[i2] = 2;
    for i in [i0, i1, i2] {
        let k = bld.hash_key(pts[i].0, pts[i].1);
        bld.hull_hash[k] = i as isize;
    }
    bld.add_triangle(i0, i1, i2, -1, -1, -1);

    let (mut xp, mut yp) = (f64::NAN, f64::NAN);
    for (k, &i) in ids.iter().enumerate() {
        let (x, y) = pts[i];
        // Points presque confondus : ignorés.
        if k > 0 && (x - xp).abs() <= EPSILON && (y - yp).abs() <= EPSILON {
            continue;
        }
        xp = x;
        yp = y;
        if i == i0 || i == i1 || i == i2 {
            continue;
        }
        // Arête visible de l'enveloppe, trouvée par le hachage angulaire.
        let mut start = 0usize;
        let key = bld.hash_key(x, y);
        for j in 0..hash_size {
            let s = bld.hull_hash[(key + j) % hash_size];
            if s >= 0 && s as usize != bld.hull_next[s as usize] {
                start = s as usize;
                break;
            }
        }
        start = bld.hull_prev[start];
        let mut e = start;
        let mut found = true;
        loop {
            let q = bld.hull_next[e];
            if orient(x, y, pts[e].0, pts[e].1, pts[q].0, pts[q].1) {
                break;
            }
            e = q;
            if e == start {
                found = false;
                break;
            }
        }
        if !found {
            continue; // probablement un doublon
        }
        let he = bld.hull_tri[e] as isize;
        let t = bld.add_triangle(e, i, bld.hull_next[e], -1, -1, he);
        bld.hull_tri[i] = bld.legalize(t + 2);
        bld.hull_tri[e] = t;

        // Vers l'avant le long de l'enveloppe.
        let mut nn = bld.hull_next[e];
        loop {
            let q = bld.hull_next[nn];
            if !orient(x, y, pts[nn].0, pts[nn].1, pts[q].0, pts[q].1) {
                break;
            }
            let (hi, hn) = (bld.hull_tri[i] as isize, bld.hull_tri[nn] as isize);
            let t = bld.add_triangle(nn, i, q, hi, -1, hn);
            bld.hull_tri[i] = bld.legalize(t + 2);
            bld.hull_next[nn] = nn; // retiré
            nn = q;
        }
        // Puis vers l'arrière.
        if e == start {
            loop {
                let q = bld.hull_prev[e];
                if !orient(x, y, pts[q].0, pts[q].1, pts[e].0, pts[e].1) {
                    break;
                }
                let (he, hq) = (bld.hull_tri[e] as isize, bld.hull_tri[q] as isize);
                let t = bld.add_triangle(q, i, e, -1, he, hq);
                bld.legalize(t + 2);
                bld.hull_tri[q] = t;
                bld.hull_next[e] = e; // retiré
                e = q;
            }
        }
        bld.hull_start = e;
        bld.hull_prev[i] = e;
        bld.hull_next[e] = i;
        bld.hull_prev[nn] = i;
        bld.hull_next[i] = nn;
        let k = bld.hash_key(x, y);
        bld.hull_hash[k] = i as isize;
        let k = bld.hash_key(pts[e].0, pts[e].1);
        bld.hull_hash[k] = e as isize;
    }
    Delaunay { triangles: bld.triangles, halfedges: bld.halfedges }
}

/// Voisins de Voronoï de chaque point réel (`< real`), comme `calculateVoronoi` d'Azgaar :
/// `neighbors[p]` ne garde que les points réels ; `border[p]` est vrai si un voisin est un point
/// de bordure (`>= real`). Un point absent de la triangulation n'a aucun voisin.
pub fn voronoi_neighbors(d: &Delaunay, real: usize) -> (Vec<Vec<usize>>, Vec<bool>) {
    let mut neighbors: Vec<Option<Vec<usize>>> = vec![None; real];
    let mut border = vec![false; real];
    let next = |e: usize| if e % 3 == 2 { e - 2 } else { e + 1 };
    for e in 0..d.triangles.len() {
        let p = d.triangles[next(e)];
        if p >= real || neighbors[p].is_some() {
            continue;
        }
        // Demi-arêtes entrantes autour de `p` (au plus 20, comme Azgaar).
        let mut edges = vec![];
        let mut incoming = e;
        loop {
            edges.push(incoming);
            let outgoing = next(incoming);
            let h = d.halfedges[outgoing];
            if h < 0 || h as usize == e || edges.len() >= 20 {
                break;
            }
            incoming = h as usize;
        }
        let all: Vec<usize> = edges.iter().map(|&e| d.triangles[e]).collect();
        let kept: Vec<usize> = all.iter().copied().filter(|&c| c < real).collect();
        border[p] = all.len() > kept.len();
        neighbors[p] = Some(kept);
    }
    (neighbors.into_iter().map(Option::unwrap_or_default).collect(), border)
}

/// Cellules de Voronoï des points réels (`< real`) : sommets = centres circonscrits des triangles.
pub struct VoronoiCells {
    /// Sommet `t` = centre circonscrit du triangle `t`.
    pub vertices: Vec<(f64, f64)>,
    /// Pour chaque point réel : polygone fermé (indices de sommets, dans l'ordre du tour) et, pour
    /// chaque arête `polygon[i] → polygon[i+1]` (la dernière revient au premier), le point voisin
    /// de l'autre côté. Vide si le point touche l'enveloppe convexe (cellule ouverte) ou n'est pas triangulé.
    pub cells: Vec<Vec<(u32, u32)>>, // (sommet, voisin de l'arête qui part de ce sommet)
}

/// Calcule les cellules de Voronoï des points réels. Les voisins `>= real` (points de bordure)
/// sont conservés : à l'appelant de les traiter comme « extérieur ».
pub fn voronoi_cells(pts: &[(f64, f64)], d: &Delaunay, real: usize) -> VoronoiCells {
    let next = |e: usize| if e % 3 == 2 { e - 2 } else { e + 1 };
    let vertices: Vec<(f64, f64)> = (0..d.triangles.len() / 3)
        .map(|t| {
            let [a, b, c] = [d.triangles[3 * t], d.triangles[3 * t + 1], d.triangles[3 * t + 2]];
            let (pa, pb, pc) = (pts[a], pts[b], pts[c]);
            let (x, y) = circumcenter(pa.0, pa.1, pb.0, pb.1, pc.0, pc.1);
            if x.is_finite() && y.is_finite() {
                (x, y)
            } else {
                // Triangle dégénéré (points alignés) : on retombe sur le centre de gravité.
                ((pa.0 + pb.0 + pc.0) / 3.0, (pa.1 + pb.1 + pc.1) / 3.0)
            }
        })
        .collect();
    let mut cells: Vec<Vec<(u32, u32)>> = vec![vec![]; real];
    let mut seen = vec![false; real];
    for e in 0..d.triangles.len() {
        let p = d.triangles[next(e)];
        if p >= real || seen[p] {
            continue;
        }
        seen[p] = true;
        // Demi-arêtes entrantes autour de `p`, sans plafond ; cellule ouverte si on touche l'enveloppe.
        let mut edges = vec![];
        let mut incoming = e;
        let mut open = false;
        loop {
            edges.push(incoming);
            let h = d.halfedges[next(incoming)];
            if h < 0 {
                open = true;
                break;
            }
            if h as usize == e {
                break;
            }
            incoming = h as usize;
            if edges.len() > d.triangles.len() {
                open = true; // garde-fou : triangulation incohérente
                break;
            }
        }
        if open {
            continue;
        }
        // L'arête qui va du triangle de `edges[i]` à celui de `edges[i+1]` sépare `p` du point
        // de départ de `edges[i+1]`.
        let n = edges.len();
        cells[p] = (0..n)
            .map(|i| ((edges[i] / 3) as u32, d.triangles[edges[(i + 1) % n]] as u32))
            .collect();
    }
    VoronoiCells { vertices, cells }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Générateur déterministe (SplitMix64) pour des nuages de points de test.
    fn points(n: usize, seed: u64) -> Vec<(f64, f64)> {
        let mut s = seed;
        let mut next = || {
            s = s.wrapping_add(0x9E37_79B9_7F4A_7C15);
            let mut z = s;
            z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
            z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
            (z ^ (z >> 31)) as f64 / u64::MAX as f64
        };
        (0..n).map(|_| (next() * 1000.0, next() * 600.0)).collect()
    }

    #[test]
    fn every_triangle_has_an_empty_circumcircle() {
        let pts = points(300, 7);
        let d = triangulate(&pts);
        assert!(d.triangles.len() >= 3 * 500, "about 2n triangles");
        for t in (0..d.triangles.len()).step_by(3) {
            let [a, b, c] = [d.triangles[t], d.triangles[t + 1], d.triangles[t + 2]];
            let (cx, cy) = circumcenter(pts[a].0, pts[a].1, pts[b].0, pts[b].1, pts[c].0, pts[c].1);
            let r = dist2(cx, cy, pts[a].0, pts[a].1);
            for (i, p) in pts.iter().enumerate() {
                if i != a && i != b && i != c {
                    assert!(dist2(cx, cy, p.0, p.1) >= r * (1.0 - 1e-9), "point {i} inside triangle {t}");
                }
            }
        }
    }

    #[test]
    fn halfedges_are_symmetric_and_the_count_matches_the_hull() {
        let pts = points(200, 11);
        let d = triangulate(&pts);
        let mut hull = 0;
        for (e, &h) in d.halfedges.iter().enumerate() {
            if h < 0 {
                hull += 1;
            } else {
                assert_eq!(d.halfedges[h as usize], e as isize);
            }
        }
        // Euler : t = 2n − 2 − h.
        assert_eq!(d.triangles.len() / 3, 2 * pts.len() - 2 - hull);
    }

    #[test]
    fn a_square_lattice_gives_its_four_sides_as_neighbours() {
        // Grille légèrement déformée (comme celle d'Azgaar) ; bordure tout autour.
        let mut pts = vec![];
        for r in 0..5 {
            for c in 0..5 {
                let j = ((r * 7 + c * 13) % 5) as f64 * 0.05;
                pts.push((c as f64 * 10.0 + 5.0 + j, r as f64 * 10.0 + 5.0 - j));
            }
        }
        let real = pts.len();
        for i in 0..=5 {
            let v = i as f64 * 10.0;
            pts.extend([(v, -10.0), (v, 60.0), (-10.0, v), (60.0, v)]);
        }
        let (nb, border) = voronoi_neighbors(&triangulate(&pts), real);
        let mut center = nb[12].clone();
        center.sort();
        assert!([7, 11, 13, 17].iter().all(|n| center.contains(n)), "{center:?}");
        assert!(!border[12]);
        assert!(border[0] && border[4] && border[20], "corner cells touch the boundary");
    }

    #[test]
    fn degenerate_inputs_give_no_triangulation() {
        assert!(triangulate(&[(0.0, 0.0), (1.0, 1.0)]).triangles.is_empty());
        assert!(triangulate(&[(0.0, 0.0), (1.0, 1.0), (2.0, 2.0)]).triangles.is_empty());
    }

    /// Nuage de points réels (`n`) entouré d'un anneau de points de bordure.
    fn cloud_with_ring(n: usize, seed: u64) -> (Vec<(f64, f64)>, usize) {
        let mut pts = points(n, seed);
        let real = pts.len();
        for i in 0..=20 {
            let (x, y) = (i as f64 * 50.0, i as f64 * 30.0);
            pts.extend([(x, -100.0), (x, 700.0), (-100.0, y), (1100.0, y)]);
        }
        (pts, real)
    }

    fn area(poly: &[(f64, f64)]) -> f64 {
        let n = poly.len();
        (0..n).map(|i| poly[i].0 * poly[(i + 1) % n].1 - poly[(i + 1) % n].0 * poly[i].1).sum::<f64>().abs() / 2.0
    }

    fn contains(poly: &[(f64, f64)], (px, py): (f64, f64)) -> bool {
        let mut inside = false;
        let n = poly.len();
        for i in 0..n {
            let ((ax, ay), (bx, by)) = (poly[i], poly[(i + 1) % n]);
            if (ay > py) != (by > py) && px < (bx - ax) * (py - ay) / (by - ay) + ax {
                inside = !inside;
            }
        }
        inside
    }

    #[test]
    fn voronoi_cells_are_closed_symmetric_and_contain_their_point() {
        let (pts, real) = cloud_with_ring(300, 5);
        let d = triangulate(&pts);
        let vc = voronoi_cells(&pts, &d, real);
        assert_eq!(vc.cells.len(), real);
        let mut total = 0.0;
        let mut interior = 0;
        for (p, cell) in vc.cells.iter().enumerate() {
            if cell.is_empty() {
                continue;
            }
            interior += 1;
            let poly: Vec<(f64, f64)> = cell.iter().map(|&(v, _)| vc.vertices[v as usize]).collect();
            assert!(contains(&poly, pts[p]), "cell {p} must contain its point");
            total += area(&poly);
            for i in 0..cell.len() {
                let (a, q) = cell[i];
                let b = cell[(i + 1) % cell.len()].0;
                if (q as usize) < real {
                    let other = &vc.cells[q as usize];
                    assert!(!other.is_empty(), "neighbour {q} of {p} must be closed");
                    let found = (0..other.len()).any(|j| {
                        other[j].0 == b && other[(j + 1) % other.len()].0 == a && other[j].1 as usize == p
                    });
                    assert!(found, "edge {a}-{b} of {p} missing in {q}");
                }
            }
        }
        assert_eq!(interior, real, "the ring closes every real cell");
        // Les points tiennent dans 1000 × 600 et l'anneau borne le tout à 1200 × 800 : les cellules
        // couvrent au moins le premier, au plus le second.
        assert!(total > 600_000.0 && total < 960_000.0, "total area {total}");
    }
}
