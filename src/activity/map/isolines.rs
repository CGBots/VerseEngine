//! Contours de régions sur les cellules de Voronoï : boucles par valeur, frontières, lissage et
//! export en chemin SVG. Fonctions pures, sans entrée/sortie.

use super::delaunay::VoronoiCells;
use std::collections::{BTreeMap, HashMap};

/// Retire les points consécutifs identiques (y compris dernier = premier).
fn dedup_points(mut pts: Vec<(f64, f64)>) -> Vec<(f64, f64)> {
    pts.dedup();
    while pts.len() > 1 && pts.first() == pts.last() {
        pts.pop();
    }
    pts
}

/// Contours fermés des régions de même valeur, comme `getIsolines` d'Azgaar.
/// `value(c)` : `None` = cellule ignorée (cachée, ou hors calque) ; une arête de la cellule c
/// appartient au contour de la valeur v = value(c) si le voisin n'a pas la même valeur (voisin
/// ignoré, de bordure `>= cells.len()`, ou de valeur différente).
/// Retour : pour chaque valeur, ses boucles (suites de points, sans répéter le premier à la fin).
pub fn region_loops<V: Copy + Eq + std::hash::Hash + Ord>(
    vc: &VoronoiCells,
    value: impl Fn(usize) -> Option<V>,
) -> BTreeMap<V, Vec<Vec<(f64, f64)>>> {
    let mut edges: BTreeMap<V, Vec<(u32, u32)>> = BTreeMap::new();
    for (c, cell) in vc.cells.iter().enumerate() {
        let Some(v) = value(c) else { continue };
        for i in 0..cell.len() {
            let (from, nb) = cell[i];
            let to = cell[(i + 1) % cell.len()].0;
            let same = (nb as usize) < vc.cells.len() && value(nb as usize) == Some(v);
            if !same {
                edges.entry(v).or_default().push((from, to));
            }
        }
    }
    let mut out = BTreeMap::new();
    for (v, list) in edges {
        let mut by_start: HashMap<u32, Vec<usize>> = HashMap::new();
        for (i, &(a, _)) in list.iter().enumerate() {
            by_start.entry(a).or_default().push(i);
        }
        let mut used = vec![false; list.len()];
        let mut loops = vec![];
        for first in 0..list.len() {
            if used[first] {
                continue;
            }
            used[first] = true;
            let start = list[first].0;
            let mut path = vec![start];
            let mut cur = list[first].1;
            while cur != start {
                path.push(cur);
                let Some(e) = by_start.get(&cur).and_then(|l| l.iter().copied().find(|&e| !used[e])) else {
                    break;
                };
                used[e] = true;
                cur = list[e].1;
            }
            let pts = dedup_points(path.iter().map(|&i| vc.vertices[i as usize]).collect());
            if pts.len() >= 3 {
                loops.push(pts);
            }
        }
        if !loops.is_empty() {
            out.insert(v, loops);
        }
    }
    out
}

/// Frontières entre deux régions visibles de valeurs différentes (ex. États) : lignes ouvertes ou
/// fermées, chaque arête une seule fois. `value(c) == None` : pas de frontière avec cette cellule.
pub fn border_lines<V: Copy + Eq>(vc: &VoronoiCells, value: impl Fn(usize) -> Option<V>) -> Vec<Vec<(f64, f64)>> {
    let mut edges: Vec<(u32, u32)> = vec![];
    for (c, cell) in vc.cells.iter().enumerate() {
        let Some(v) = value(c) else { continue };
        for i in 0..cell.len() {
            let (from, nb) = cell[i];
            let nb = nb as usize;
            if nb <= c || nb >= vc.cells.len() {
                continue;
            }
            if matches!(value(nb), Some(w) if w != v) {
                edges.push((from, cell[(i + 1) % cell.len()].0));
            }
        }
    }
    let mut adj: HashMap<u32, Vec<usize>> = HashMap::new();
    for (i, &(a, b)) in edges.iter().enumerate() {
        adj.entry(a).or_default().push(i);
        adj.entry(b).or_default().push(i);
    }
    let degree = |v: u32| adj.get(&v).map_or(0, Vec::len);
    let mut used = vec![false; edges.len()];
    let mut lines = vec![];
    // Marche depuis `start` le long de `first` jusqu'à une extrémité, un carrefour ou le départ.
    let walk = |start: u32, first: usize, used: &mut Vec<bool>| {
        let mut path = vec![start];
        let (mut cur, mut e) = (start, first);
        loop {
            used[e] = true;
            let other = if edges[e].0 == cur { edges[e].1 } else { edges[e].0 };
            path.push(other);
            cur = other;
            if degree(cur) != 2 || cur == start {
                break;
            }
            match adj[&cur].iter().copied().find(|&n| !used[n]) {
                Some(n) => e = n,
                None => break,
            }
        }
        path
    };
    // D'abord les lignes qui partent d'une extrémité ou d'un carrefour, puis les boucles restantes.
    let mut starts: Vec<u32> = adj.keys().copied().filter(|&v| degree(v) != 2).collect();
    starts.sort_unstable();
    for s in starts {
        for &e in &adj[&s] {
            if !used[e] {
                lines.push(walk(s, e, &mut used));
            }
        }
    }
    for e in 0..edges.len() {
        if !used[e] {
            lines.push(walk(edges[e].0, e, &mut used));
        }
    }
    lines
        .into_iter()
        .map(|l| dedup_points(l.iter().map(|&i| vc.vertices[i as usize]).collect()))
        .filter(|l| l.len() >= 2)
        .collect()
}

fn lerp(a: (f64, f64), b: (f64, f64), t: f64) -> (f64, f64) {
    (a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t)
}

/// Lissage de Chaikin d'une boucle fermée (`iterations` passes).
pub fn chaikin_closed(points: &[(f64, f64)], iterations: usize) -> Vec<(f64, f64)> {
    let mut cur = points.to_vec();
    for _ in 0..iterations {
        if cur.len() < 3 {
            break;
        }
        let n = cur.len();
        cur = (0..n)
            .flat_map(|i| {
                let (a, b) = (cur[i], cur[(i + 1) % n]);
                [lerp(a, b, 0.25), lerp(a, b, 0.75)]
            })
            .collect();
    }
    cur
}

/// Lissage de Chaikin d'une ligne ouverte (extrémités gardées).
pub fn chaikin_open(points: &[(f64, f64)], iterations: usize) -> Vec<(f64, f64)> {
    let mut cur = points.to_vec();
    for _ in 0..iterations {
        if cur.len() < 3 {
            break;
        }
        let mut next = vec![cur[0]];
        for w in cur.windows(2) {
            next.push(lerp(w[0], w[1], 0.25));
            next.push(lerp(w[0], w[1], 0.75));
        }
        next.push(*cur.last().expect("non vide"));
        cur = next;
    }
    cur
}

/// Nombre à 1 décimale, sans zéros inutiles (`12.5`, `3`).
fn num(x: f64) -> String {
    let s = format!("{x:.1}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" || s.is_empty() { "0".into() } else { s.into() }
}

/// Chemin SVG compact : `M x y L x y … Z` par boucle (coordonnées à 1 décimale, sans zéros
/// inutiles, ex. `12.5` / `3`), boucles concaténées. `closed` : ajoute `Z`.
pub fn svg_path(loops: &[Vec<(f64, f64)>], closed: bool) -> String {
    let mut out = String::new();
    for l in loops {
        let mut prev: Option<(String, String)> = None;
        for &(x, y) in l {
            let cur = (num(x), num(y));
            if prev.as_ref() == Some(&cur) {
                continue;
            }
            out.push(if prev.is_none() { 'M' } else { 'L' });
            out.push_str(&format!("{} {}", cur.0, cur.1));
            prev = Some(cur);
        }
        if closed && prev.is_some() {
            out.push('Z');
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::super::delaunay::{triangulate, voronoi_cells};
    use super::*;

    /// Grille 8×8 légèrement déformée, entourée d'un anneau de points de bordure.
    fn grid() -> (VoronoiCells, Vec<(f64, f64)>) {
        let mut pts = vec![];
        for r in 0..8 {
            for c in 0..8 {
                let j = ((r * 7 + c * 13) % 5) as f64 * 0.4;
                pts.push((c as f64 * 10.0 + 5.0 + j, r as f64 * 10.0 + 5.0 - j));
            }
        }
        let real = pts.len();
        for i in 0..=8 {
            let v = i as f64 * 10.0;
            pts.extend([(v, -10.0), (v, 90.0), (-10.0, v), (90.0, v)]);
        }
        let vc = voronoi_cells(&pts, &triangulate(&pts), real);
        (vc, pts[..real].to_vec())
    }

    #[test]
    fn two_halves_give_one_loop_each_and_one_border() {
        let (vc, pts) = grid();
        let side = |c: usize| Some(if pts[c].0 < 40.0 { 1 } else { 2 });
        let loops = region_loops(&vc, side);
        assert_eq!(loops[&1].len(), 1);
        assert_eq!(loops[&2].len(), 1);
        assert_eq!(border_lines(&vc, side).len(), 1);
    }

    #[test]
    fn a_hidden_cell_inside_a_region_makes_a_hole() {
        let (vc, pts) = grid();
        let hole = (0..pts.len()).find(|&c| (pts[c].0 - 15.0).abs() < 4.0 && (pts[c].1 - 35.0).abs() < 4.0).unwrap();
        let loops = region_loops(&vc, |c| if c == hole { None } else { Some(if pts[c].0 < 40.0 { 1 } else { 2 }) });
        assert_eq!(loops[&1].len(), 2);
    }

    #[test]
    fn chaikin_doubles_closed_loops_and_keeps_open_endpoints() {
        let sq = [(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (0.0, 10.0)];
        assert_eq!(chaikin_closed(&sq, 1).len(), 8);
        assert_eq!(chaikin_closed(&sq, 2).len(), 16);
        let line = [(0.0, 0.0), (10.0, 0.0), (10.0, 10.0)];
        let s = chaikin_open(&line, 2);
        assert_eq!(s.first(), Some(&(0.0, 0.0)));
        assert_eq!(s.last(), Some(&(10.0, 10.0)));
        assert_eq!(s.len(), 12);
    }

    #[test]
    fn svg_path_is_compact() {
        let l = vec![(12.46, 3.0), (14.0, 7.24), (14.01, 7.2), (-0.01, 0.0)];
        assert_eq!(svg_path(&[l.clone()], true), "M12.5 3L14 7.2L0 0Z");
        assert_eq!(svg_path(&[l.clone(), l], false), "M12.5 3L14 7.2L0 0M12.5 3L14 7.2L0 0");
    }
}
