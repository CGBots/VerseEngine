//! Réseau routier Azgaar → routes VerseEngine, et zone explorable.
//!
//! Les routes Azgaar sont des polylignes `[[x, y, cellule], …]` qui se croisent :
//! deux routes qui passent par la même cellule forment une **bifurcation**. Une route
//! VerseEngine relie deux lieux *voisins* ; sa distance est donc la somme des tronçons
//! (éventuellement sur plusieurs routes Azgaar) entre deux burgs retenus, sans passer
//! par un troisième burg retenu.

use std::collections::{BinaryHeap, HashMap, HashSet};
use std::cmp::Ordering;

use crate::activity::map::azgaar::{AzgaarMap, RoutePoint};

/// Kilomètres par unité de distance Azgaar.
pub fn unit_to_km(unit: Option<&str>) -> f64 {
    match unit.map(|u| u.trim().to_lowercase()) {
        Some(u) if u == "mi" || u.starts_with("mile") => 1.609_344,
        Some(u) if u == "m" => 0.001,
        Some(u) if u.starts_with("league") || u == "lg" => 4.828,
        Some(u) if u == "nmi" || u.starts_with("nautical") => 1.852,
        _ => 1.0, // km par défaut
    }
}

// ---------------------------------------------------------------------------
// Zone explorable
// ---------------------------------------------------------------------------

/// Rendu de l'extérieur de la zone explorable (choix de l'admin).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OutsideMode {
    /// La carte est recadrée sur la boîte englobante de la zone.
    Crop,
    /// L'extérieur reste visible, grisé.
    Grey,
    /// L'extérieur est effacé (terrain à plat, transparent).
    Remove,
}

/// Polygone en coordonnées de la carte Azgaar (pixels). Vide = toute la carte.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Zone {
    pub polygon: Vec<(f64, f64)>,
}

impl Zone {
    pub fn contains(&self, x: f64, y: f64) -> bool {
        let p = &self.polygon;
        if p.len() < 3 { return true; }
        // Lancer de rayon (règle pair-impair).
        let mut inside = false;
        let mut j = p.len() - 1;
        for i in 0..p.len() {
            let (xi, yi) = p[i];
            let (xj, yj) = p[j];
            if (yi > y) != (yj > y) && x < (xj - xi) * (y - yi) / (yj - yi) + xi {
                inside = !inside;
            }
            j = i;
        }
        inside
    }

    /// Boîte englobante (min_x, min_y, max_x, max_y), bornée à la carte.
    pub fn bounds(&self, width: f64, height: f64) -> (f64, f64, f64, f64) {
        if self.polygon.len() < 3 { return (0.0, 0.0, width, height); }
        let mut b = (f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY);
        for &(x, y) in &self.polygon {
            b = (b.0.min(x), b.1.min(y), b.2.max(x), b.3.max(y));
        }
        (b.0.max(0.0), b.1.max(0.0), b.2.min(width), b.3.min(height))
    }

    /// Masque `cols × rows` (vrai = explorable) pour la heightmap.
    pub fn terrain_mask(&self, width: f64, height: f64, cols: usize, rows: usize) -> Vec<bool> {
        let mut mask = Vec::with_capacity(cols * rows);
        for r in 0..rows {
            for c in 0..cols {
                let x = (c as f64 + 0.5) / cols as f64 * width;
                let y = (r as f64 + 0.5) / rows as f64 * height;
                mask.push(self.contains(x, y));
            }
        }
        mask
    }
}

// ---------------------------------------------------------------------------
// Graphe routier
// ---------------------------------------------------------------------------

/// Route proposée entre deux burgs voisins.
#[derive(Debug, Clone, PartialEq)]
pub struct RoadCandidate {
    pub burg_a: u64,
    pub burg_b: u64,
    pub distance_km: f64,
    /// Au moins un tronçon est une route maritime (`searoutes`) : candidate « navette ».
    pub uses_sea: bool,
    /// Identifiants des routes Azgaar empruntées (bifurcations comprises).
    pub azgaar_routes: Vec<u64>,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
enum NodeKey {
    Cell(usize),
    /// Point sans cellule : coordonnées arrondies au dixième de pixel.
    Pos(i64, i64),
}

fn key(p: &RoutePoint) -> NodeKey {
    match p.cell {
        Some(c) => NodeKey::Cell(c),
        None => NodeKey::Pos((p.x * 10.0).round() as i64, (p.y * 10.0).round() as i64),
    }
}

#[derive(Clone, Copy)]
struct Arc_ {
    to: usize,
    km: f64,
    sea: bool,
    route: Option<u64>,
}

#[derive(Copy, Clone, PartialEq)]
struct Item(f64, usize);
impl Eq for Item {}
impl Ord for Item {
    fn cmp(&self, o: &Self) -> Ordering { o.0.partial_cmp(&self.0).unwrap_or(Ordering::Equal).then_with(|| self.1.cmp(&o.1)) }
}
impl PartialOrd for Item {
    fn partial_cmp(&self, o: &Self) -> Option<Ordering> { Some(self.cmp(o)) }
}

/// Calcule les routes entre burgs **retenus** (`burg_ids`), dans la zone explorable.
///
/// - un tronçon dont une extrémité sort de la zone est coupé ;
/// - le poids d'un tronçon = longueur en pixels × échelle ; si Azgaar fournit la
///   longueur totale de la route, elle est répartie au prorata des tronçons ;
/// - un burg dont la cellule n'est sur aucune route est rattaché au point de route
///   le plus proche s'il est à moins de `snap_px`.
pub fn road_candidates(map: &AzgaarMap, burg_ids: &[u64], zone: &Zone, include_sea: bool, snap_px: f64) -> Vec<RoadCandidate> {
    let km_per_unit = unit_to_km(map.distance_unit.as_deref());
    let scale = map.distance_scale.unwrap_or(1.0);

    let mut index: HashMap<NodeKey, usize> = HashMap::new();
    let mut coords: Vec<(f64, f64)> = vec![];
    let mut adj: Vec<Vec<Arc_>> = vec![];
    let mut node = |p: &RoutePoint, index: &mut HashMap<NodeKey, usize>, coords: &mut Vec<(f64, f64)>, adj: &mut Vec<Vec<Arc_>>| -> usize {
        *index.entry(key(p)).or_insert_with(|| {
            coords.push((p.x, p.y));
            adj.push(vec![]);
            coords.len() - 1
        })
    };

    for route in &map.routes {
        let sea = route.group.as_deref() == Some("searoutes");
        if sea && !include_sea { continue; }
        let seg_px: Vec<f64> = route.points.windows(2).map(|w| ((w[1].x - w[0].x).powi(2) + (w[1].y - w[0].y).powi(2)).sqrt()).collect();
        let total_px: f64 = seg_px.iter().sum();
        let unit_per_px = match route.length {
            Some(l) if total_px > 0.0 => l / total_px,
            _ => scale,
        };
        for (w, px) in route.points.windows(2).zip(seg_px) {
            if !zone.contains(w[0].x, w[0].y) || !zone.contains(w[1].x, w[1].y) { continue; }
            let a = node(&w[0], &mut index, &mut coords, &mut adj);
            let b = node(&w[1], &mut index, &mut coords, &mut adj);
            if a == b { continue; }
            let km = px * unit_per_px * km_per_unit;
            adj[a].push(Arc_ { to: b, km, sea, route: route.id });
            adj[b].push(Arc_ { to: a, km, sea, route: route.id });
        }
    }
    if coords.is_empty() { return vec![]; }

    // Nœud de chaque burg retenu (dans la zone).
    let wanted: HashSet<u64> = burg_ids.iter().copied().collect();
    let mut burg_node: Vec<(u64, usize)> = vec![];
    for b in map.burgs.iter().filter(|b| wanted.contains(&b.id) && zone.contains(b.x, b.y)) {
        let direct = b.cell.and_then(|c| index.get(&NodeKey::Cell(c)).copied());
        let n = direct.or_else(|| {
            coords
                .iter()
                .enumerate()
                .map(|(i, &(x, y))| (i, ((x - b.x).powi(2) + (y - b.y).powi(2)).sqrt()))
                .filter(|&(_, d)| d <= snap_px)
                .min_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(Ordering::Equal))
                .map(|(i, _)| i)
        });
        if let Some(n) = n { burg_node.push((b.id, n)); }
    }
    let node_burg: HashMap<usize, u64> = burg_node.iter().map(|&(b, n)| (n, b)).collect();

    let mut best: HashMap<(u64, u64), RoadCandidate> = HashMap::new();
    for &(src_burg, src) in &burg_node {
        let n = coords.len();
        let mut dist = vec![f64::INFINITY; n];
        let mut prev: Vec<Option<(usize, Arc_)>> = vec![None; n];
        let mut heap = BinaryHeap::new();
        dist[src] = 0.0;
        heap.push(Item(0.0, src));
        while let Some(Item(d, u)) = heap.pop() {
            if d > dist[u] { continue; }
            if u != src {
                if let Some(&other) = node_burg.get(&u) {
                    if other != src_burg {
                        // Burg voisin atteint : on remonte le chemin, sans traverser ce burg.
                        let (mut sea, mut routes, mut cur) = (false, vec![], u);
                        while let Some((p, arc)) = prev[cur] {
                            sea |= arc.sea;
                            if let Some(r) = arc.route { if !routes.contains(&r) { routes.push(r); } }
                            cur = p;
                        }
                        routes.sort_unstable();
                        let pair = (src_burg.min(other), src_burg.max(other));
                        let cand = RoadCandidate { burg_a: pair.0, burg_b: pair.1, distance_km: d, uses_sea: sea, azgaar_routes: routes };
                        best.entry(pair).and_modify(|c| if d < c.distance_km { *c = cand.clone() }).or_insert(cand);
                    }
                    continue;
                }
            }
            for arc in &adj[u] {
                let nd = d + arc.km;
                if nd < dist[arc.to] {
                    dist[arc.to] = nd;
                    prev[arc.to] = Some((u, *arc));
                    heap.push(Item(nd, arc.to));
                }
            }
        }
    }
    let mut out: Vec<RoadCandidate> = best.into_values().collect();
    out.sort_by(|a, b| (a.burg_a, a.burg_b).cmp(&(b.burg_a, b.burg_b)));
    prune_detours(out, DETOUR_TOLERANCE)
}

/// Un détour par un autre lieu au plus 10 % plus long rend la route directe inutile.
pub const DETOUR_TOLERANCE: f64 = 1.10;

/// Retire la route A–B quand un lieu C retenu donne un chemin A–C–B au plus `tolerance` fois
/// plus long. Sur le réseau maillé d'Azgaar, presque toutes les paires de lieux sont reliées par
/// un chemin qui évite les autres : sans ce tri, 12 lieux donneraient 66 routes (une par paire).
///
/// Les routes sont examinées de la plus longue à la plus courte, et A–B n'est retirée que si A–C
/// et C–B sont encore gardées : le réseau reste connexe.
pub fn prune_detours(candidates: Vec<RoadCandidate>, tolerance: f64) -> Vec<RoadCandidate> {
    let dist: HashMap<(u64, u64), f64> = candidates.iter().map(|c| ((c.burg_a, c.burg_b), c.distance_km)).collect();
    let pair = |a: u64, b: u64| (a.min(b), a.max(b));
    let burgs: Vec<u64> = {
        let mut v: Vec<u64> = candidates.iter().flat_map(|c| [c.burg_a, c.burg_b]).collect();
        v.sort_unstable();
        v.dedup();
        v
    };
    let mut kept: HashSet<(u64, u64)> = dist.keys().copied().collect();
    let mut order: Vec<&RoadCandidate> = candidates.iter().collect();
    order.sort_by(|x, y| y.distance_km.partial_cmp(&x.distance_km).unwrap_or(Ordering::Equal).then((x.burg_a, x.burg_b).cmp(&(y.burg_a, y.burg_b))));
    for c in order {
        let (a, b) = (c.burg_a, c.burg_b);
        let redundant = burgs.iter().filter(|&&m| m != a && m != b).any(|&m| {
            match (kept.contains(&pair(a, m)), kept.contains(&pair(m, b))) {
                (true, true) => dist[&pair(a, m)] + dist[&pair(m, b)] <= c.distance_km * tolerance,
                _ => false,
            }
        });
        if redundant {
            kept.remove(&(a, b));
        }
    }
    candidates.into_iter().filter(|c| kept.contains(&(c.burg_a, c.burg_b))).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::activity::map::azgaar::parse;

    /// Carte en « T » : A(0,0) — X(10,0) — B(20,0) sur la route 0, et une bifurcation
    /// en X vers C(10,10) sur la route 1. D(30,0) prolonge la route 0 au-delà de B.
    const T_MAP: &str = r#"{
      "info": {"width": 100, "height": 100},
      "settings": {"distanceScale": 2, "distanceUnit": "km"},
      "pack": {
        "burgs": [{},
          {"i":1,"name":"A","x":0,"y":0,"cell":1},
          {"i":2,"name":"B","x":20,"y":0,"cell":3},
          {"i":3,"name":"C","x":10,"y":10,"cell":4},
          {"i":4,"name":"D","x":30,"y":0,"cell":5},
          {"i":5,"name":"Isolé","x":90,"y":90,"cell":9}
        ],
        "routes": [
          {"i":0,"group":"roads","points":[[0,0,1],[10,0,2],[20,0,3],[30,0,5]]},
          {"i":1,"group":"trails","points":[[10,0,2],[10,10,4]]},
          {"i":2,"group":"searoutes","points":[[0,0,1],[0,40,7],[30,0,5]]}
        ]
      }
    }"#;

    fn find(c: &[RoadCandidate], a: u64, b: u64) -> Option<&RoadCandidate> {
        c.iter().find(|r| r.burg_a == a.min(b) && r.burg_b == a.max(b))
    }

    #[test]
    fn sums_segments_across_bifurcations() {
        let m = parse(T_MAP).unwrap();
        let c = road_candidates(&m, &[1, 2, 3, 4, 5], &Zone::default(), false, 0.0);
        // A–C passe par la bifurcation X : 10 px (route 0) + 10 px (route 1), ×2 km/px.
        let ac = find(&c, 1, 3).unwrap();
        assert!((ac.distance_km - 40.0).abs() < 1e-9);
        assert_eq!(ac.azgaar_routes, vec![0, 1]);
        assert!((find(&c, 1, 2).unwrap().distance_km - 40.0).abs() < 1e-9);
        assert!((find(&c, 2, 4).unwrap().distance_km - 20.0).abs() < 1e-9);
        // A–D n'est pas direct : B est entre les deux.
        assert!(find(&c, 1, 4).is_none());
        // Burg isolé : aucune route.
        assert!(c.iter().all(|r| r.burg_a != 5 && r.burg_b != 5));
    }

    fn cand(a: u64, b: u64, km: f64) -> RoadCandidate {
        RoadCandidate { burg_a: a.min(b), burg_b: a.max(b), distance_km: km, uses_sea: false, azgaar_routes: vec![] }
    }

    #[test]
    fn a_road_passing_by_another_place_is_dropped() {
        // A —10— B —10— C, et un chemin A–C de 21 km qui longe B : A–C est redondante.
        let kept = prune_detours(vec![cand(1, 2, 10.0), cand(2, 3, 10.0), cand(1, 3, 21.0)], DETOUR_TOLERANCE);
        let pairs: Vec<_> = kept.iter().map(|c| (c.burg_a, c.burg_b)).collect();
        assert_eq!(pairs, vec![(1, 2), (2, 3)]);
    }

    #[test]
    fn a_real_shortcut_is_kept() {
        // Triangle : le détour par B (20 km) est bien plus long que A–C (12 km).
        assert_eq!(prune_detours(vec![cand(1, 2, 10.0), cand(2, 3, 10.0), cand(1, 3, 12.0)], DETOUR_TOLERANCE).len(), 3);
    }

    #[test]
    fn pruning_never_disconnects_places() {
        // Trois routes de même longueur : chacune est un détour « gratuit » des deux autres en
        // tolérance large, mais on n'en retire qu'une.
        let kept = prune_detours(vec![cand(1, 2, 10.0), cand(2, 3, 10.0), cand(1, 3, 10.0)], 2.5);
        assert_eq!(kept.len(), 2);
    }

    #[test]
    fn a_complete_mesh_shrinks_to_neighbours() {
        // Quatre lieux alignés tous reliés entre eux (comme sur un réseau maillé) : il reste la chaîne.
        let xs = [0.0, 10.0, 20.0, 30.0];
        let mut all = vec![];
        for i in 0..4u64 {
            for j in (i + 1)..4u64 {
                all.push(cand(i + 1, j + 1, (xs[j as usize] - xs[i as usize]) * 1.02));
            }
        }
        let pairs: Vec<_> = prune_detours(all, DETOUR_TOLERANCE).iter().map(|c| (c.burg_a, c.burg_b)).collect();
        assert_eq!(pairs, vec![(1, 2), (2, 3), (3, 4)]);
    }

    #[test]
    fn unselected_burgs_are_traversed() {
        let m = parse(T_MAP).unwrap();
        // B non retenu : A–D devient direct, longueur additionnée (30 px × 2).
        let c = road_candidates(&m, &[1, 4], &Zone::default(), false, 0.0);
        assert_eq!(c.len(), 1);
        assert!((c[0].distance_km - 60.0).abs() < 1e-9);
    }

    #[test]
    fn sea_routes_are_optional_and_flagged() {
        let m = parse(T_MAP).unwrap();
        let c = road_candidates(&m, &[1, 4], &Zone::default(), true, 0.0);
        // Par la terre 60 km, par la mer (40 + 50 px) × 2 = 180 km : la plus courte gagne.
        assert!((c[0].distance_km - 60.0).abs() < 1e-9);
        assert!(!c[0].uses_sea);
        // Sans la route terrestre (D retiré de la route 0), seule la mer relie A et D.
        let json = T_MAP.replace(r#",[30,0,5]]},"#, r#"]},"#);
        let m = parse(&json).unwrap();
        let c = road_candidates(&m, &[1, 4], &Zone::default(), true, 0.0);
        assert!(c[0].uses_sea);
        assert!((c[0].distance_km - 180.0).abs() < 1e-9);
        assert!(road_candidates(&m, &[1, 4], &Zone::default(), false, 0.0).is_empty());
    }

    #[test]
    fn declared_route_length_is_prorated() {
        let json = T_MAP.replace(r#"{"i":1,"group":"trails","#, r#"{"i":1,"group":"trails","length":7,"#);
        let m = parse(&json).unwrap();
        let c = road_candidates(&m, &[1, 3], &Zone::default(), false, 0.0);
        // A→X : 10 px × 2 = 20 km ; X→C : longueur déclarée 7 km.
        assert!((c[0].distance_km - 27.0).abs() < 1e-9);
    }

    #[test]
    fn zone_cuts_roads_and_excludes_outside_burgs() {
        let m = parse(T_MAP).unwrap();
        // Zone couvrant x ∈ [-1, 25] : D (x=30) est dehors, le tronçon B–D est coupé.
        let zone = Zone { polygon: vec![(-1.0, -1.0), (25.0, -1.0), (25.0, 20.0), (-1.0, 20.0)] };
        let c = road_candidates(&m, &[1, 2, 3, 4], &zone, false, 0.0);
        assert!(find(&c, 2, 4).is_none());
        assert!(find(&c, 1, 2).is_some());
    }

    #[test]
    fn miles_are_converted() {
        let json = T_MAP.replace(r#""distanceUnit": "km""#, r#""distanceUnit": "mi""#);
        let m = parse(&json).unwrap();
        let c = road_candidates(&m, &[2, 4], &Zone::default(), false, 0.0);
        assert!((c[0].distance_km - 20.0 * 1.609_344).abs() < 1e-9);
    }

    #[test]
    fn burg_off_route_snaps_within_tolerance() {
        let json = T_MAP.replace(r#""x":10,"y":10,"cell":4"#, r#""x":10,"y":12,"cell":42"#);
        let m = parse(&json).unwrap();
        assert!(find(&road_candidates(&m, &[1, 3], &Zone::default(), false, 1.0), 1, 3).is_none());
        assert!(find(&road_candidates(&m, &[1, 3], &Zone::default(), false, 3.0), 1, 3).is_some());
    }

    #[test]
    fn zone_geometry() {
        let z = Zone { polygon: vec![(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (0.0, 10.0)] };
        assert!(z.contains(5.0, 5.0));
        assert!(!z.contains(15.0, 5.0));
        assert!(Zone::default().contains(1e9, 1e9));
        assert_eq!(z.bounds(100.0, 100.0), (0.0, 0.0, 10.0, 10.0));
        let mask = z.terrain_mask(20.0, 20.0, 2, 2);
        assert_eq!(mask, vec![true, false, false, false]);
    }
}
