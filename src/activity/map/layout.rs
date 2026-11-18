//! Placement automatique des lieux à partir des distances des routes (« Source B »).
//!
//! Algorithme (déterministe, sans aléa) :
//! 1. plus courts chemins (Dijkstra) entre tous les lieux d'une composante connexe ;
//! 2. MDS classique (double centrage, puissance itérée) pour une position initiale ;
//! 3. raffinement par *stress majorization* localisée (Gansner, Koren & North 2004),
//!    pondérée par `1/d²`, qui minimise `Σ w_ij (‖x_i − x_j‖ − d_ij)²` ;
//! 4. composantes non connexes juxtaposées, puis mise à l'échelle uniforme dans
//!    `[0, CANVAS]²` (les rapports de distances sont conservés).

use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashMap};

/// Taille du canevas de sortie (unités arbitraires, carré).
pub const CANVAS: f64 = 1000.0;
const MAX_ITERATIONS: usize = 300;
const EPSILON: f64 = 1e-7;

/// Une route du graphe : deux identifiants de lieux et une longueur (km, > 0).
#[derive(Debug, Clone, Copy)]
pub struct Edge {
    pub a: u64,
    pub b: u64,
    pub length: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Clone)]
pub struct Layout {
    /// Position de chaque lieu, dans le même ordre que l'entrée `nodes`.
    pub positions: Vec<(u64, Point)>,
    /// Unités de canevas par kilomètre (identique pour toutes les composantes).
    pub units_per_km: f64,
    /// Stress normalisé `Σ w(‖‖−d)² / Σ w d²` (0 = distances parfaitement respectées).
    pub normalized_stress: f64,
}

#[derive(Copy, Clone, PartialEq)]
struct HeapItem(f64, usize);
impl Eq for HeapItem {}
impl Ord for HeapItem {
    fn cmp(&self, other: &Self) -> Ordering {
        other.0.partial_cmp(&self.0).unwrap_or(Ordering::Equal).then_with(|| self.1.cmp(&other.1))
    }
}
impl PartialOrd for HeapItem {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> { Some(self.cmp(other)) }
}

fn dijkstra(adj: &[Vec<(usize, f64)>], source: usize) -> Vec<f64> {
    let mut dist = vec![f64::INFINITY; adj.len()];
    let mut heap = BinaryHeap::new();
    dist[source] = 0.0;
    heap.push(HeapItem(0.0, source));
    while let Some(HeapItem(d, u)) = heap.pop() {
        if d > dist[u] { continue; }
        for &(v, w) in &adj[u] {
            let nd = d + w;
            if nd < dist[v] {
                dist[v] = nd;
                heap.push(HeapItem(nd, v));
            }
        }
    }
    dist
}

/// Calcule un placement. Les arêtes vers des lieux inconnus, de longueur non finie
/// ou ≤ 0 sont ignorées ; les doublons gardent la plus courte longueur.
pub fn compute_layout(nodes: &[u64], edges: &[Edge]) -> Layout {
    let n = nodes.len();
    if n == 0 {
        return Layout { positions: vec![], units_per_km: 1.0, normalized_stress: 0.0 };
    }
    let index: HashMap<u64, usize> = nodes.iter().enumerate().map(|(i, &id)| (id, i)).collect();
    let mut adj: Vec<Vec<(usize, f64)>> = vec![vec![]; n];
    for e in edges {
        let (Some(&i), Some(&j)) = (index.get(&e.a), index.get(&e.b)) else { continue };
        if i == j || !e.length.is_finite() || e.length <= 0.0 { continue; }
        adj[i].push((j, e.length));
        adj[j].push((i, e.length));
    }
    let dist: Vec<Vec<f64>> = (0..n).map(|i| dijkstra(&adj, i)).collect();

    // Composantes connexes (ordre déterministe : par premier indice).
    let mut component = vec![usize::MAX; n];
    let mut components: Vec<Vec<usize>> = vec![];
    for i in 0..n {
        if component[i] != usize::MAX { continue; }
        let members: Vec<usize> = (0..n).filter(|&j| dist[i][j].is_finite()).collect();
        for &m in &members { component[m] = components.len(); }
        components.push(members);
    }

    let mut local: Vec<Vec<Point>> = Vec::with_capacity(components.len());
    let mut num = 0.0;
    let mut den = 0.0;
    for members in &components {
        let sub: Vec<Vec<f64>> = members.iter().map(|&i| members.iter().map(|&j| dist[i][j]).collect()).collect();
        let pts = layout_component(&sub);
        let (a, b) = stress_terms(&pts, &sub);
        num += a;
        den += b;
        local.push(pts);
    }

    // Juxtaposition horizontale des composantes, avec un espacement proportionnel.
    let mean_edge = {
        let lens: Vec<f64> = adj.iter().flatten().map(|&(_, w)| w).collect();
        if lens.is_empty() { 1.0 } else { lens.iter().sum::<f64>() / lens.len() as f64 }
    };
    let gap = mean_edge;
    let mut world = vec![Point { x: 0.0, y: 0.0 }; n];
    let mut cursor = 0.0;
    for (members, pts) in components.iter().zip(&local) {
        let (min_x, max_x, min_y, _) = bounds(pts);
        for (&i, p) in members.iter().zip(pts) {
            world[i] = Point { x: p.x - min_x + cursor, y: p.y - min_y };
        }
        cursor += (max_x - min_x) + gap;
    }

    let (min_x, max_x, min_y, max_y) = bounds(&world);
    let extent = (max_x - min_x).max(max_y - min_y);
    let scale = if extent > EPSILON { CANVAS / extent } else { 1.0 };
    let off_x = (CANVAS - (max_x - min_x) * scale) / 2.0;
    let off_y = (CANVAS - (max_y - min_y) * scale) / 2.0;
    let positions = nodes
        .iter()
        .zip(&world)
        .map(|(&id, p)| (id, Point { x: (p.x - min_x) * scale + off_x, y: (p.y - min_y) * scale + off_y }))
        .collect();

    Layout { positions, units_per_km: scale, normalized_stress: if den > 0.0 { num / den } else { 0.0 } }
}

fn bounds(pts: &[Point]) -> (f64, f64, f64, f64) {
    let mut b = (f64::INFINITY, f64::NEG_INFINITY, f64::INFINITY, f64::NEG_INFINITY);
    for p in pts {
        b.0 = b.0.min(p.x);
        b.1 = b.1.max(p.x);
        b.2 = b.2.min(p.y);
        b.3 = b.3.max(p.y);
    }
    if pts.is_empty() { (0.0, 0.0, 0.0, 0.0) } else { b }
}

/// Termes du stress pondéré (numérateur, dénominateur) pour une composante.
fn stress_terms(pts: &[Point], d: &[Vec<f64>]) -> (f64, f64) {
    let mut num = 0.0;
    let mut den = 0.0;
    for i in 0..pts.len() {
        for j in (i + 1)..pts.len() {
            let dij = d[i][j];
            if dij <= EPSILON { continue; }
            let w = 1.0 / (dij * dij);
            let e = distance(pts[i], pts[j]) - dij;
            num += w * e * e;
            den += w * dij * dij;
        }
    }
    (num, den)
}

fn distance(a: Point, b: Point) -> f64 { ((a.x - b.x).powi(2) + (a.y - b.y).powi(2)).sqrt() }

/// Stress normalisé d'un placement (exposé pour les tests et le panneau admin).
pub fn normalized_stress(pts: &[Point], d: &[Vec<f64>]) -> f64 {
    let (a, b) = stress_terms(pts, d);
    if b > 0.0 { a / b } else { 0.0 }
}

fn layout_component(d: &[Vec<f64>]) -> Vec<Point> {
    let n = d.len();
    match n {
        0 => return vec![],
        1 => return vec![Point { x: 0.0, y: 0.0 }],
        2 => return vec![Point { x: 0.0, y: 0.0 }, Point { x: d[0][1], y: 0.0 }],
        _ => {}
    }
    let mut pts = classical_mds(d);
    // Petite perturbation déterministe pour sortir d'une configuration colinéaire
    // (la stress majorization ne peut pas quitter une droite par elle-même).
    let scale = d.iter().flatten().cloned().fold(0.0, f64::max).max(1.0);
    for (i, p) in pts.iter_mut().enumerate() {
        p.x += 1e-3 * scale * ((i as f64) * 0.7 + 0.3).cos();
        p.y += 1e-3 * scale * ((i as f64) * 1.7 + 0.1).sin();
    }
    stress_majorization(&mut pts, d);
    pts
}

/// MDS classique : B = −½ J D² J, deux premiers vecteurs propres par puissance itérée.
fn classical_mds(d: &[Vec<f64>]) -> Vec<Point> {
    let n = d.len();
    let sq: Vec<Vec<f64>> = d.iter().map(|r| r.iter().map(|v| v * v).collect()).collect();
    let row_mean: Vec<f64> = sq.iter().map(|r| r.iter().sum::<f64>() / n as f64).collect();
    let total_mean = row_mean.iter().sum::<f64>() / n as f64;
    let mut b = vec![vec![0.0; n]; n];
    for i in 0..n {
        for j in 0..n {
            b[i][j] = -0.5 * (sq[i][j] - row_mean[i] - row_mean[j] + total_mean);
        }
    }
    let (l1, v1) = power_iteration(&b, None);
    let (l2, v2) = power_iteration(&b, Some((l1, &v1)));
    let s1 = l1.max(0.0).sqrt();
    let s2 = l2.max(0.0).sqrt();
    (0..n).map(|i| Point { x: v1[i] * s1, y: v2[i] * s2 }).collect()
}

fn power_iteration(m: &[Vec<f64>], deflate: Option<(f64, &Vec<f64>)>) -> (f64, Vec<f64>) {
    let n = m.len();
    let mut v: Vec<f64> = (0..n).map(|i| 1.0 + ((i as f64) * 1.618).sin() * 0.5).collect();
    normalize(&mut v);
    let mut lambda = 0.0;
    for _ in 0..500 {
        let mut w: Vec<f64> = (0..n).map(|i| (0..n).map(|j| m[i][j] * v[j]).sum()).collect();
        if let Some((l, u)) = deflate {
            let dot: f64 = (0..n).map(|i| u[i] * v[i]).sum();
            for i in 0..n { w[i] -= l * u[i] * dot; }
        }
        let new_lambda: f64 = (0..n).map(|i| w[i] * v[i]).sum();
        if normalize(&mut w) < EPSILON { return (0.0, v); }
        let converged = (new_lambda - lambda).abs() <= 1e-10 * new_lambda.abs().max(1.0);
        v = w;
        lambda = new_lambda;
        if converged { break; }
    }
    (lambda, v)
}

fn normalize(v: &mut [f64]) -> f64 {
    let norm = v.iter().map(|x| x * x).sum::<f64>().sqrt();
    if norm > EPSILON { for x in v.iter_mut() { *x /= norm; } }
    norm
}

fn stress_majorization(pts: &mut [Point], d: &[Vec<f64>]) {
    let n = pts.len();
    let mut previous = normalized_stress(pts, d);
    for _ in 0..MAX_ITERATIONS {
        for i in 0..n {
            let (mut sx, mut sy, mut sw) = (0.0, 0.0, 0.0);
            for j in 0..n {
                if i == j { continue; }
                let dij = d[i][j];
                if dij <= EPSILON { continue; }
                let w = 1.0 / (dij * dij);
                let cur = distance(pts[i], pts[j]).max(EPSILON);
                sx += w * (pts[j].x + dij * (pts[i].x - pts[j].x) / cur);
                sy += w * (pts[j].y + dij * (pts[i].y - pts[j].y) / cur);
                sw += w;
            }
            if sw > 0.0 { pts[i] = Point { x: sx / sw, y: sy / sw }; }
        }
        let s = normalized_stress(pts, d);
        if (previous - s).abs() < 1e-9 { break; }
        previous = s;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pos(l: &Layout, id: u64) -> Point { l.positions.iter().find(|(i, _)| *i == id).unwrap().1 }
    fn km(l: &Layout, a: u64, b: u64) -> f64 { distance(pos(l, a), pos(l, b)) / l.units_per_km }

    #[test]
    fn empty_graph() {
        let l = compute_layout(&[], &[]);
        assert!(l.positions.is_empty());
    }

    #[test]
    fn single_node_is_centered() {
        let l = compute_layout(&[7], &[]);
        assert_eq!(l.positions.len(), 1);
        let p = pos(&l, 7);
        assert!(p.x.is_finite() && p.y.is_finite());
    }

    #[test]
    fn triangle_3_4_5_is_exact() {
        let e = [Edge { a: 1, b: 2, length: 3.0 }, Edge { a: 2, b: 3, length: 4.0 }, Edge { a: 1, b: 3, length: 5.0 }];
        let l = compute_layout(&[1, 2, 3], &e);
        assert!((km(&l, 1, 2) - 3.0).abs() < 1e-3, "{}", km(&l, 1, 2));
        assert!((km(&l, 2, 3) - 4.0).abs() < 1e-3);
        assert!((km(&l, 1, 3) - 5.0).abs() < 1e-3);
        assert!(l.normalized_stress < 1e-6);
    }

    #[test]
    fn square_with_diagonals_via_shortest_paths() {
        // Carré de côté 10 sans diagonales : les diagonales valent 20 par le graphe,
        // le placement fait un compromis mais garde les côtés proches de 10.
        let e = [
            Edge { a: 1, b: 2, length: 10.0 }, Edge { a: 2, b: 3, length: 10.0 },
            Edge { a: 3, b: 4, length: 10.0 }, Edge { a: 4, b: 1, length: 10.0 },
        ];
        let l = compute_layout(&[1, 2, 3, 4], &e);
        for (a, b) in [(1, 2), (2, 3), (3, 4), (4, 1)] {
            let d = km(&l, a, b);
            assert!((d - 10.0).abs() < 2.0, "côté {a}-{b} = {d}");
        }
        assert!(l.normalized_stress < 0.05);
    }

    #[test]
    fn linear_chain_respects_distances() {
        let e = [Edge { a: 1, b: 2, length: 2.0 }, Edge { a: 2, b: 3, length: 3.0 }, Edge { a: 3, b: 4, length: 5.0 }];
        let l = compute_layout(&[1, 2, 3, 4], &e);
        assert!((km(&l, 1, 4) - 10.0).abs() < 0.05);
        assert!((km(&l, 2, 3) - 3.0).abs() < 0.05);
    }

    #[test]
    fn disconnected_components_do_not_overlap() {
        let e = [Edge { a: 1, b: 2, length: 5.0 }, Edge { a: 3, b: 4, length: 5.0 }];
        let l = compute_layout(&[1, 2, 3, 4, 5], &e);
        assert!((km(&l, 1, 2) - 5.0).abs() < 1e-6);
        assert!((km(&l, 3, 4) - 5.0).abs() < 1e-6);
        let ids = [1, 2, 3, 4, 5];
        for i in 0..ids.len() {
            for j in (i + 1)..ids.len() {
                assert!(distance(pos(&l, ids[i]), pos(&l, ids[j])) > 1.0, "{} et {} se chevauchent", ids[i], ids[j]);
            }
        }
    }

    #[test]
    fn output_fits_canvas() {
        let e: Vec<Edge> = (1..20).map(|i| Edge { a: i, b: i + 1, length: (i % 5 + 1) as f64 }).collect();
        let nodes: Vec<u64> = (1..=20).collect();
        let l = compute_layout(&nodes, &e);
        for (_, p) in &l.positions {
            assert!(p.x >= -1e-6 && p.x <= CANVAS + 1e-6 && p.y >= -1e-6 && p.y <= CANVAS + 1e-6);
        }
    }

    #[test]
    fn is_deterministic_and_ignores_bad_edges() {
        let e = [
            Edge { a: 1, b: 2, length: 4.0 }, Edge { a: 2, b: 3, length: 4.0 }, Edge { a: 3, b: 1, length: 4.0 },
            Edge { a: 1, b: 99, length: 1.0 }, Edge { a: 1, b: 2, length: f64::NAN }, Edge { a: 2, b: 2, length: 1.0 },
        ];
        let a = compute_layout(&[1, 2, 3], &e);
        let b = compute_layout(&[1, 2, 3], &e);
        assert_eq!(a.positions, b.positions);
        assert!((km(&a, 1, 3) - 4.0).abs() < 1e-3);
    }
}
