//! « Générer une map » (étape E7, décision I11 de `docs/activity/IMPORT_SPEC.md`) : une carte
//! simple, **déterministe** (même graine et même nombre de lieux → même carte, pour toujours :
//! générateur pseudo-aléatoire maison, indépendant des versions de bibliothèques).
//!
//! - lieux répartis sans se toucher (échantillonnage par rejet, distance minimale) ;
//! - routes : graphe de voisinage relatif (une route A–B existe si aucun lieu C n'est plus proche
//!   à la fois de A et de B) : connexe, sans croisement, environ 2,5 routes par lieu ;
//! - tracé : courbe de Bézier quadratique ; son point de contrôle est ce que l'admin déplace pour
//!   retoucher le tracé ;
//! - noms : syllabes, sans doublon.
//!
//! L'admin peut ensuite renommer, ajouter ou supprimer des lieux et des routes, et retoucher les
//! tracés ; la carte modifiée revient au serveur ([`GeneratedMap::validate`]) puis suit le même
//! chemin qu'un import (plan, répartition, création). Pas de relief stocké : l'activité affiche
//! son terrain 3D procédural.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use crate::activity::map::azgaar::{AzgaarMap, Burg, Route, RoutePoint};
use crate::activity::map::network::RoadCandidate;

/// Côté de la carte (unités de carte).
pub const MAP_SIDE: f64 = 1000.0;
pub const MIN_PLACES: usize = 2;
pub const MAX_PLACES: usize = 600;
/// Distance moyenne visée entre deux lieux voisins (km).
pub const NEIGHBOUR_KM: f64 = 30.0;
/// Points d'un tracé de route.
pub const PATH_POINTS: usize = 12;
pub const MAX_NAME_CHARS: usize = 100;

/// Générateur SplitMix64 : simple, rapide, stable.
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Self(seed)
    }
    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
    /// Réel dans [0, 1).
    pub fn unit(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }
    pub fn below(&mut self, n: usize) -> usize {
        (self.next_u64() % n.max(1) as u64) as usize
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GenPlace {
    /// Identifiant dans la carte générée (comme un bourg Azgaar).
    pub id: u64,
    pub name: String,
    pub x: f64,
    pub y: f64,
    #[serde(default)]
    pub capital: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GenRoad {
    pub a: u64,
    pub b: u64,
    /// Point de contrôle de la courbe (le milieu du tracé en suit la moitié du chemin).
    pub control: [f64; 2],
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeneratedMap {
    pub seed: u64,
    pub width: f64,
    pub height: f64,
    /// Kilomètres par unité de carte.
    pub km_per_unit: f64,
    pub places: Vec<GenPlace>,
    pub roads: Vec<GenRoad>,
}

const START: [&str; 24] = ["Bel", "Mor", "Val", "Cor", "Ar", "Lin", "Bro", "Fel", "Gal", "Ros", "Tal", "Ven", "Dor", "Ker", "Sain", "Mon", "Fon", "Pré", "Hau", "Bra", "Cal", "Lor", "Mar", "Thé"];
const MIDDLE: [&str; 12] = ["", "", "", "a", "e", "i", "o", "en", "an", "er", "ou", "il"];
const END: [&str; 20] = ["mont", "val", "bourg", "court", "lieu", "ville", "dun", "brac", "mar", "lac", "fort", "gard", "nac", "rive", "pierre", "bois", "roc", "lune", "vent", "stel"];

/// Nom pseudo-aléatoire (syllabes).
pub fn place_name(rng: &mut Rng) -> String {
    let mut s = format!("{}{}{}", START[rng.below(START.len())], MIDDLE[rng.below(MIDDLE.len())], END[rng.below(END.len())]);
    if rng.below(8) == 0 {
        s = format!("{s}-sur-{}", ["Mer", "Lac", "Rive", "Mont"][rng.below(4)]);
    }
    s
}

fn d(a: (f64, f64), b: (f64, f64)) -> f64 {
    ((a.0 - b.0).powi(2) + (a.1 - b.1).powi(2)).sqrt()
}

/// Graphe de voisinage relatif : paires (i, j), i < j.
pub fn relative_neighbourhood(points: &[(f64, f64)]) -> Vec<(usize, usize)> {
    let n = points.len();
    let mut out = vec![];
    for i in 0..n {
        for j in (i + 1)..n {
            let dij = d(points[i], points[j]);
            let blocked = (0..n).any(|k| k != i && k != j && d(points[i], points[k]).max(d(points[k], points[j])) < dij);
            if !blocked {
                out.push((i, j));
            }
        }
    }
    out
}

/// Tracé d'une route : courbe de Bézier quadratique de `a` à `b` passant par `control`.
pub fn road_path(a: (f64, f64), b: (f64, f64), control: [f64; 2]) -> Vec<(f64, f64)> {
    (0..PATH_POINTS)
        .map(|k| {
            let t = k as f64 / (PATH_POINTS - 1) as f64;
            let u = 1.0 - t;
            (u * u * a.0 + 2.0 * u * t * control[0] + t * t * b.0, u * u * a.1 + 2.0 * u * t * control[1] + t * t * b.1)
        })
        .collect()
}

pub fn path_length(path: &[(f64, f64)]) -> f64 {
    path.windows(2).map(|w| d(w[0], w[1])).sum()
}

/// Génère une carte de `count` lieux (borné à [`MIN_PLACES`], [`MAX_PLACES`]).
pub fn generate(seed: u64, count: usize) -> GeneratedMap {
    let count = count.clamp(MIN_PLACES, MAX_PLACES);
    let mut rng = Rng::new(seed);
    let spacing = MAP_SIDE / (count as f64).sqrt();
    // Au moins 6 % du côté : les noms des lieux du bord restent dans la carte.
    let margin = (spacing * 0.35).max(MAP_SIDE * 0.06);
    let mut min_dist = spacing * 0.65;
    let mut points: Vec<(f64, f64)> = vec![];
    let mut attempts = 0;
    while points.len() < count {
        attempts += 1;
        // Trop d'échecs : on relâche la contrainte (garantit la fin).
        if attempts % 2000 == 0 {
            min_dist *= 0.9;
        }
        let p = (margin + rng.unit() * (MAP_SIDE - 2.0 * margin), margin + rng.unit() * (MAP_SIDE - 2.0 * margin));
        if points.iter().all(|&q| d(p, q) >= min_dist) {
            points.push(p);
        }
    }
    let mut names = HashSet::new();
    let places: Vec<GenPlace> = points
        .iter()
        .enumerate()
        .map(|(i, &(x, y))| {
            let mut name = place_name(&mut rng);
            while !names.insert(name.clone()) {
                name = place_name(&mut rng);
            }
            GenPlace { id: i as u64 + 1, name, x: (x * 10.0).round() / 10.0, y: (y * 10.0).round() / 10.0, capital: false }
        })
        .collect();
    let mut places = places;
    // Capitale : le lieu le plus proche du centre.
    if let Some(c) = (0..places.len()).min_by(|&a, &b| {
        let da = d((places[a].x, places[a].y), (MAP_SIDE / 2.0, MAP_SIDE / 2.0));
        let db = d((places[b].x, places[b].y), (MAP_SIDE / 2.0, MAP_SIDE / 2.0));
        da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
    }) {
        places[c].capital = true;
    }
    let pts: Vec<(f64, f64)> = places.iter().map(|p| (p.x, p.y)).collect();
    let roads = relative_neighbourhood(&pts)
        .into_iter()
        .map(|(i, j)| {
            let (a, b) = (pts[i], pts[j]);
            // Courbure légère : contrôle décalé perpendiculairement, au plus 15 % de la longueur.
            let (mx, my) = ((a.0 + b.0) / 2.0, (a.1 + b.1) / 2.0);
            let (nx, ny) = (-(b.1 - a.1), b.0 - a.0);
            let k = (rng.unit() - 0.5) * 0.3;
            GenRoad { a: places[i].id, b: places[j].id, control: [((mx + nx * k) * 10.0).round() / 10.0, ((my + ny * k) * 10.0).round() / 10.0] }
        })
        .collect();
    GeneratedMap { seed, width: MAP_SIDE, height: MAP_SIDE, km_per_unit: NEIGHBOUR_KM / spacing, places, roads }
}

impl GeneratedMap {
    /// Vérifie une carte renvoyée par l'admin après modification. Clé d'erreur sinon.
    pub fn validate(&self) -> Result<(), &'static str> {
        if !(MIN_PLACES..=MAX_PLACES).contains(&self.places.len()) {
            return Err("activity__generate_bad_count");
        }
        if self.width != MAP_SIDE || self.height != MAP_SIDE || !self.km_per_unit.is_finite() || self.km_per_unit <= 0.0 || self.km_per_unit > 1000.0 {
            return Err("activity__invalid_request");
        }
        let inside = |x: f64, y: f64| x.is_finite() && y.is_finite() && (0.0..=self.width).contains(&x) && (0.0..=self.height).contains(&y);
        let mut ids = HashSet::new();
        for p in &self.places {
            let name = p.name.trim();
            if name.is_empty() || name.chars().count() > MAX_NAME_CHARS || !inside(p.x, p.y) || !ids.insert(p.id) || p.id == 0 {
                return Err("activity__generate_bad_place");
            }
        }
        if self.roads.len() > self.places.len() * 8 {
            return Err("activity__invalid_request");
        }
        let mut pairs = HashSet::new();
        for r in &self.roads {
            if r.a == r.b || !ids.contains(&r.a) || !ids.contains(&r.b) || !r.control.iter().all(|v| v.is_finite() && v.abs() <= 10.0 * MAP_SIDE) {
                return Err("activity__generate_bad_road");
            }
            if !pairs.insert((r.a.min(r.b), r.a.max(r.b))) {
                return Err("activity__generate_bad_road");
            }
        }
        Ok(())
    }

    /// Carte au format de l'import (bourgs et routes), sans relief.
    pub fn to_azgaar(&self) -> AzgaarMap {
        let pos = |id: u64| self.places.iter().find(|p| p.id == id).map(|p| (p.x, p.y)).unwrap_or_default();
        AzgaarMap {
            name: None,
            width: self.width,
            height: self.height,
            distance_scale: Some(self.km_per_unit),
            distance_unit: Some("km".into()),
            burgs: self.places.iter().map(|p| Burg { id: p.id, name: p.name.trim().to_string(), x: p.x, y: p.y, cell: None, capital: p.capital }).collect(),
            cells: vec![],
            routes: self
                .roads
                .iter()
                .enumerate()
                .map(|(i, r)| Route {
                    id: Some(i as u64),
                    group: Some("roads".into()),
                    length: None,
                    points: road_path(pos(r.a), pos(r.b), r.control).into_iter().map(|(x, y)| RoutePoint { x, y, cell: None }).collect(),
                })
                .collect(),
            style: None,
        }
    }

    /// Routes telles que dessinées : une route par paire choisie, **sans** retirer les détours
    /// (l'admin a décidé du réseau), distance = longueur du tracé.
    pub fn road_candidates(&self) -> Vec<RoadCandidate> {
        let pos = |id: u64| self.places.iter().find(|p| p.id == id).map(|p| (p.x, p.y)).unwrap_or_default();
        self.roads
            .iter()
            .enumerate()
            .map(|(i, r)| {
                let (a, b) = (r.a.min(r.b), r.a.max(r.b));
                let path = road_path(pos(a), pos(b), r.control);
                RoadCandidate { burg_a: a, burg_b: b, distance_km: path_length(&path) * self.km_per_unit, uses_sea: false, azgaar_routes: vec![i as u64], path }
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_same_seed_gives_the_same_map() {
        assert_eq!(generate(42, 30), generate(42, 30));
        assert_ne!(generate(42, 30), generate(43, 30));
        // Valeurs figées : un changement du générateur changerait les cartes des graines déjà
        // montrées aux admins.
        let m = generate(1, 5);
        let first: Vec<(&str, f64, f64)> = m.places.iter().map(|p| (p.name.as_str(), p.x, p.y)).collect();
        assert_eq!(first[0], ("Morilrive", 545.7, 668.8));
        assert_eq!(first[4], ("Préirive", 182.2, 171.6));
        assert_eq!(m.roads.len(), 4);
    }

    #[test]
    fn places_are_spread_named_once_and_inside_the_map() {
        let m = generate(7, 120);
        assert_eq!(m.places.len(), 120);
        let names: HashSet<_> = m.places.iter().map(|p| p.name.clone()).collect();
        assert_eq!(names.len(), 120, "no duplicate name");
        let inner = (MAP_SIDE * 0.06)..=(MAP_SIDE * 0.94);
        assert!(m.places.iter().all(|p| inner.contains(&p.x) && inner.contains(&p.y)), "away from the edges");
        assert_eq!(m.places.iter().filter(|p| p.capital).count(), 1);
        assert!(m.validate().is_ok());
    }

    #[test]
    fn the_road_network_is_connected_with_about_two_and_a_half_roads_per_place() {
        let m = generate(11, 200);
        let n = m.places.len();
        let degree = 2.0 * m.roads.len() as f64 / n as f64;
        assert!((2.0..=3.2).contains(&degree), "{degree}");
        // Connexité (parcours en largeur).
        let mut seen = HashSet::from([m.places[0].id]);
        let mut stack = vec![m.places[0].id];
        while let Some(u) = stack.pop() {
            for r in m.roads.iter().filter(|r| r.a == u || r.b == u) {
                let v = if r.a == u { r.b } else { r.a };
                if seen.insert(v) {
                    stack.push(v);
                }
            }
        }
        assert_eq!(seen.len(), n);
    }

    #[test]
    fn neighbours_are_about_thirty_km_apart() {
        let m = generate(5, 60);
        let cands = m.road_candidates();
        let mean = cands.iter().map(|c| c.distance_km).sum::<f64>() / cands.len() as f64;
        assert!((15.0..=50.0).contains(&mean), "{mean}");
        assert!(cands.iter().all(|c| c.burg_a < c.burg_b && c.path.len() == PATH_POINTS));
    }

    #[test]
    fn a_curve_goes_through_its_ends() {
        let p = road_path((0.0, 0.0), (10.0, 0.0), [5.0, 4.0]);
        assert_eq!(p[0], (0.0, 0.0));
        assert_eq!(*p.last().unwrap(), (10.0, 0.0));
        assert!(path_length(&p) > 10.0);
    }

    #[test]
    fn edits_are_validated() {
        let mut m = generate(3, 10);
        assert!(m.validate().is_ok());
        m.places[0].name = "  ".into();
        assert_eq!(m.validate(), Err("activity__generate_bad_place"));
        let mut m = generate(3, 10);
        m.places[1].x = f64::NAN;
        assert_eq!(m.validate(), Err("activity__generate_bad_place"));
        let mut m = generate(3, 10);
        let r = m.roads[0].clone();
        m.roads.push(GenRoad { a: r.b, b: r.a, control: r.control });
        assert_eq!(m.validate(), Err("activity__generate_bad_road"), "duplicate road");
        let mut m = generate(3, 10);
        m.roads.push(GenRoad { a: 1, b: 999, control: [0.0, 0.0] });
        assert_eq!(m.validate(), Err("activity__generate_bad_road"), "unknown place");
        let mut m = generate(3, 10);
        m.places.truncate(1);
        m.roads.clear();
        assert_eq!(m.validate(), Err("activity__generate_bad_count"));
    }

    #[test]
    fn a_generated_map_feeds_the_import_plan_with_its_own_roads() {
        let m = generate(9, 40);
        let a = m.to_azgaar();
        assert_eq!(a.burgs.len(), 40);
        assert!(a.cells.is_empty(), "no stored relief: the activity draws its own");
        let opts = crate::activity::map::import_plan::ImportOptions {
            servers: vec![crate::activity::map::import_plan::KnownServer { guild_id: 1, load: Default::default() }],
            explicit_roads: Some(m.road_candidates()),
            ..Default::default()
        };
        let plan = crate::activity::map::import_plan::build_plan(&a, &opts, &[], &[]).unwrap();
        assert_eq!(plan.places.len(), 40);
        assert_eq!(plan.roads.len(), m.roads.len(), "every drawn road is kept, no detour removed");
    }
}

