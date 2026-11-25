//! Carte façon Azgaar en vecteurs : contours des terres, lacs, biomes et États, calculés comme
//! Azgaar (`getIsolines`) à partir des cellules du pack gardées à l'import (`MapStyle.pack`).
//!
//! La géométrie (cellules de Voronoï) est calculée une fois par carte et gardée en mémoire ; les
//! contours sont recalculés à chaque requête avec le masque du joueur (brouillard) : une cellule
//! cachée n'entre dans aucun contour, rien de ce qu'elle contient n'est envoyé.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use mongodb::bson::oid::ObjectId;
use serde::Serialize;

use crate::activity::map::delaunay::{triangulate, voronoi_cells, VoronoiCells};
use crate::activity::map::isolines::{border_lines, chaikin_closed, chaikin_open, region_loops, svg_path};
use crate::database::universe_maps::{PackCells, PACK_LAKE, PACK_LAND};

/// Cartes gardées en mémoire au plus (une par univers actif).
const MAX_CACHED: usize = 32;
/// Passes de lissage de la côte et des lacs (Azgaar : courbe B-spline).
const COAST_SMOOTHING: usize = 2;

/// Cellules d'une carte et leurs données, prêtes pour les contours.
pub struct VectorSource {
    pub cells: VoronoiCells,
    pub points: Vec<(f64, f64)>,
    pub pack: PackCells,
}

impl std::fmt::Debug for VectorSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "VectorSource({} cellules)", self.pack.real)
    }
}

type Cache = Mutex<HashMap<(ObjectId, u64), Arc<VectorSource>>>;

fn cache() -> &'static Cache {
    static C: OnceLock<Cache> = OnceLock::new();
    C.get_or_init(Default::default)
}

/// Cellules de la carte `map_id` (version `updated_at`), calculées au premier appel.
pub fn source(map_id: ObjectId, updated_at: u64, pack: &PackCells) -> Option<Arc<VectorSource>> {
    if !pack.is_consistent() || pack.real == 0 {
        return None;
    }
    let key = (map_id, updated_at);
    if let Some(s) = cache().lock().unwrap_or_else(|p| p.into_inner()).get(&key) {
        return Some(s.clone());
    }
    let points = pack.points();
    let d = triangulate(&points);
    let cells = voronoi_cells(&points, &d, pack.real as usize);
    let src = Arc::new(VectorSource { cells, points, pack: pack.clone() });
    let mut map = cache().lock().unwrap_or_else(|p| p.into_inner());
    // Une autre version de la même carte ne sert plus.
    map.retain(|k, _| k.0 != map_id);
    if map.len() >= MAX_CACHED {
        if let Some(k) = map.keys().next().copied() {
            map.remove(&k);
        }
    }
    map.insert(key, src.clone());
    Some(src)
}

/// Contours envoyés au client (chemins SVG, coordonnées de la carte).
#[derive(Debug, Serialize, Clone, PartialEq)]
pub struct VectorDto {
    /// Terres émergées, côte lissée.
    pub land: String,
    /// Lacs, bords lissés.
    pub lakes: String,
    /// (biome, chemin) : polygones bruts, à découper par les terres.
    pub biomes: Vec<(u8, String)>,
    /// (État, chemin), sans l'État 0 (terres neutres).
    pub states: Vec<(u8, String)>,
    /// Frontières entre États (lignes).
    pub borders: String,
}

/// Contours des cellules visibles (`visible(c)`) : une cellule cachée n'apparaît dans aucun.
pub fn vector_dto(src: &VectorSource, visible: impl Fn(usize) -> bool) -> VectorDto {
    let p = &src.pack;
    let land = |c: usize| visible(c) && p.water.get(c) == Some(&PACK_LAND);
    // Biomes et États : une passe (coins des cellules arrondis ; le trait de même couleur côté
    // client masque les petits écarts entre régions voisines).
    let soften = |loops: Vec<Vec<(f64, f64)>>| -> Vec<Vec<(f64, f64)>> { loops.iter().map(|l| chaikin_closed(l, 1)).collect() };
    let smooth = |loops: Vec<Vec<(f64, f64)>>| -> Vec<Vec<(f64, f64)>> { loops.iter().map(|l| chaikin_closed(l, COAST_SMOOTHING)).collect() };

    let land_loops = region_loops(&src.cells, |c| land(c).then_some(()));
    let lake_loops = region_loops(&src.cells, |c| (visible(c) && p.water.get(c) == Some(&PACK_LAKE)).then_some(()));
    let biomes = region_loops(&src.cells, |c| if land(c) { p.biome.get(c).copied() } else { None });
    let states = region_loops(&src.cells, |c| if land(c) { p.state.get(c).copied().filter(|s| *s > 0) } else { None });
    let borders = border_lines(&src.cells, |c| if land(c) { p.state.get(c).copied() } else { None });
    let borders: Vec<Vec<(f64, f64)>> = borders.iter().map(|l| chaikin_open(l, 1)).collect();

    VectorDto {
        land: land_loops.into_values().map(|l| svg_path(&smooth(l), true)).collect(),
        lakes: lake_loops.into_values().map(|l| svg_path(&smooth(l), true)).collect(),
        biomes: biomes.into_iter().map(|(b, l)| (b, svg_path(&soften(l), true))).collect(),
        states: states.into_iter().map(|(s, l)| (s, svg_path(&soften(l), true))).collect(),
        borders: svg_path(&borders, false),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::universe_maps::{PACK_HIDDEN, PACK_OCEAN};

    /// Grille 10 × 10 de cellules (pas 10) entourée d'un anneau de bordure : terre au centre
    /// (biome selon la colonne, État 1 à gauche, 2 à droite), mer autour, un lac.
    fn pack() -> PackCells {
        let mut pts = vec![];
        let (mut biome, mut state, mut water) = (vec![], vec![], vec![]);
        for r in 0..10 {
            for c in 0..10 {
                let jitter = ((r * 7 + c * 3) % 5) as f64 * 0.3;
                pts.push((c as f64 * 10.0 + 5.0 + jitter, r as f64 * 10.0 + 5.0 - jitter));
                let is_land = (2..8).contains(&r) && (2..8).contains(&c);
                let lake = r == 4 && c == 4;
                water.push(if lake { PACK_LAKE } else if is_land { PACK_LAND } else { PACK_OCEAN });
                biome.push(if is_land && !lake { if c < 5 { 3 } else { 6 } } else { 0 });
                state.push(if is_land && !lake { if c < 5 { 1 } else { 2 } } else { 0 });
            }
        }
        let real = pts.len() as u32;
        for i in 0..40 {
            let a = i as f64 / 40.0 * std::f64::consts::TAU;
            pts.push((50.0 + 80.0 * a.cos(), 50.0 + 80.0 * a.sin()));
        }
        PackCells { real, points: PackCells::encode_points(&pts), biome, state, water }
    }

    #[test]
    fn layers_follow_the_cells() {
        let src = source(ObjectId::new(), 1, &pack()).unwrap();
        let v = vector_dto(&src, |_| true);
        assert!(v.land.starts_with('M') && v.land.contains('Z'));
        assert!(!v.lakes.is_empty(), "the lake");
        assert_eq!(v.biomes.iter().map(|b| b.0).collect::<Vec<_>>(), vec![3, 6]);
        assert_eq!(v.states.iter().map(|s| s.0).collect::<Vec<_>>(), vec![1, 2]);
        assert!(!v.borders.is_empty(), "border between states 1 and 2");
    }

    #[test]
    fn hidden_cells_send_nothing() {
        let src = source(ObjectId::new(), 1, &pack()).unwrap();
        // Seule la moitié gauche est visible : ni le biome 6 ni l'État 2 ne doivent apparaître,
        // et aucune frontière (il n'y a plus deux États visibles côte à côte).
        let left = |c: usize| (c % 10) < 5;
        let v = vector_dto(&src, left);
        assert_eq!(v.biomes.iter().map(|b| b.0).collect::<Vec<_>>(), vec![3]);
        assert_eq!(v.states.iter().map(|s| s.0).collect::<Vec<_>>(), vec![1]);
        assert!(v.borders.is_empty());
        // Rien de visible : rien du tout.
        let none = vector_dto(&src, |_| false);
        assert!(none.land.is_empty() && none.lakes.is_empty() && none.biomes.is_empty() && none.states.is_empty() && none.borders.is_empty());
        // Cellules hors zone (lasso) : comme cachées.
        let mut p = pack();
        p.water.iter_mut().for_each(|w| *w = PACK_HIDDEN);
        let v = vector_dto(&source(ObjectId::new(), 1, &p).unwrap(), |_| true);
        assert!(v.land.is_empty() && v.biomes.is_empty());
    }

    #[test]
    fn inconsistent_data_gives_no_source() {
        let mut p = pack();
        p.biome.pop();
        assert!(source(ObjectId::new(), 1, &p).is_none());
    }
}
