//! Brouillard (étape E8, décisions I17 et Q4 de `docs/activity/IMPORT_SPEC.md`).
//!
//! Option propre à chaque univers (`Universe.fog_enabled`, au choix du créateur). La carte n'est
//! **lisible que dans les zones découvertes**, pour tout le monde hors du mode admin : un
//! modérateur ou un administrateur voit comme un joueur, et toute la carte en mode admin
//! (`fog_applies`). Le front dessine une vraie brume animée et opaque ; mais c'est le serveur
//! qui garantit qu'aucune donnée hors des zones découvertes ne part vers le client :
//! - lieux envoyés : les lieux découverts et les destinations possibles depuis la position
//!   (leur nom est déjà dans la liste des destinations) ;
//! - routes envoyées : celles déjà empruntées, la route en cours et celles qui partent d'ici ;
//! - relief : hauteurs effacées hors des zones découvertes ;
//! - image de fond (export Azgaar) : jamais envoyée sous brouillard (impossible à découper).
//!
//! Zones découvertes : la position actuelle, les extrémités des routes déjà empruntées
//! (`known_roads`) et le départ ou l'arrivée du trajet en cours ; plus un couloir le long des
//! routes empruntées.

use std::collections::HashSet;

use serde::Serialize;

use crate::activity::map_view::MapDto;
use crate::activity::permissions::AccessLevel;

/// Le brouillard s'applique-t-il ? Oui si l'univers l'a activé, sauf en mode admin (`admin_view`,
/// demandé par le client) pour une session qui a droit au mode admin. Le client ne peut donc que
/// *demander* la vue entière : les droits viennent de la session, calculés par le bot.
///
/// Avant, seuls les joueurs avaient le brouillard : un administrateur ou un créateur d'univers
/// avec les droits d'administrateur ne le voyait jamais, même hors du mode admin.
pub fn fog_applies(enabled: bool, access: AccessLevel, admin_view: bool) -> bool {
    enabled && !(admin_view && access.can_use_admin_mode())
}

/// Ce que le joueur a découvert (identifiants Discord, comme dans `MapDto`).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Discovery {
    /// Lieu où se trouve le joueur.
    pub here: Option<u64>,
    /// Départ et arrivée du trajet en cours.
    pub trip: Vec<u64>,
    /// Route en cours.
    pub current_road: Option<u64>,
    /// Routes déjà empruntées.
    pub known_roads: HashSet<u64>,
}

/// Ce que le front doit dégager de la brume.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FogDto {
    /// Rayon d'une zone découverte autour d'un lieu (unités de carte).
    pub radius: f64,
    /// Lieux découverts : zone dégagée de rayon `radius`.
    pub discovered: Vec<String>,
    /// Destinations possibles non découvertes : entrevues (zone plus petite).
    pub glimpsed: Vec<String>,
    /// Routes découvertes : couloir dégagé.
    pub roads: Vec<String>,
}

/// Rayon d'une zone découverte : 60 % de la longueur médiane des routes (à vol d'oiseau), ou
/// 5 % du côté de la carte sans route.
pub fn fog_radius(dto: &MapDto) -> f64 {
    let pos = |id: &str| dto.places.iter().find(|p| p.id == id).map(|p| (p.x, p.y));
    let mut lengths: Vec<f64> = dto
        .roads
        .iter()
        .filter_map(|r| Some((pos(&r.from)?, pos(&r.to)?)))
        .map(|(a, b)| ((a.0 - b.0).powi(2) + (a.1 - b.1).powi(2)).sqrt())
        .filter(|l| *l > 0.0)
        .collect();
    if lengths.is_empty() {
        return 0.05 * dto.width.max(dto.height);
    }
    lengths.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    0.6 * lengths[lengths.len() / 2]
}

fn dist_to_segment(p: (f64, f64), a: (f64, f64), b: (f64, f64)) -> f64 {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let len2 = dx * dx + dy * dy;
    let t = if len2 == 0.0 { 0.0 } else { (((p.0 - a.0) * dx + (p.1 - a.1) * dy) / len2).clamp(0.0, 1.0) };
    ((p.0 - a.0 - t * dx).powi(2) + (p.1 - a.1 - t * dy).powi(2)).sqrt()
}

/// Applique le brouillard à la carte d'un joueur : ne garde que ce qu'il peut voir.
pub fn apply_fog(mut dto: MapDto, discovery: &Discovery) -> MapDto {
    let radius = fog_radius(&dto);
    let id = |v: u64| v.to_string();

    // Routes découvertes, et routes qui partent de la position.
    let mut roads: HashSet<String> = discovery.known_roads.iter().map(|&r| id(r)).collect();
    if let Some(r) = discovery.current_road {
        roads.insert(id(r));
    }
    let here = discovery.here.map(id);
    let leaving: HashSet<String> = dto.roads.iter().filter(|r| Some(&r.from) == here.as_ref() || Some(&r.to) == here.as_ref()).map(|r| r.id.clone()).collect();

    // Lieux découverts : position, trajet, extrémités des routes découvertes.
    let mut discovered: HashSet<String> = discovery.trip.iter().map(|&p| id(p)).collect();
    discovered.extend(here.clone());
    for r in dto.roads.iter().filter(|r| roads.contains(&r.id)) {
        discovered.insert(r.from.clone());
        discovered.insert(r.to.clone());
    }
    // Entrevues : destinations depuis la position.
    let glimpsed: HashSet<String> = dto
        .roads
        .iter()
        .filter(|r| leaving.contains(&r.id))
        .flat_map(|r| [r.from.clone(), r.to.clone()])
        .filter(|p| !discovered.contains(p))
        .collect();

    dto.places.retain(|p| discovered.contains(&p.id) || glimpsed.contains(&p.id));
    dto.roads.retain(|r| roads.contains(&r.id) || leaving.contains(&r.id));

    // Relief et rendu façon Azgaar : effacés hors des zones découvertes (lieux découverts et
    // couloirs des routes découvertes).
    let centers: Vec<(f64, f64)> = dto.places.iter().filter(|p| discovered.contains(&p.id)).map(|p| (p.x, p.y)).collect();
    let segments: Vec<((f64, f64), (f64, f64))> = dto
        .roads
        .iter()
        .filter(|r| roads.contains(&r.id))
        .flat_map(|r| {
            let pts: Vec<(f64, f64)> = match &r.path {
                Some(p) if p.len() >= 2 => p.iter().map(|q| (q[0], q[1])).collect(),
                _ => vec![],
            };
            pts.windows(2).map(|w| (w[0], w[1])).collect::<Vec<_>>()
        })
        .collect();
    let reach = radius * 1.3;
    let near = |p: (f64, f64)| {
        centers.iter().any(|&q| ((p.0 - q.0).powi(2) + (p.1 - q.1).powi(2)).sqrt() <= reach)
            || segments.iter().any(|&(a, b)| dist_to_segment(p, a, b) <= radius * 0.6)
    };
    let (width, height) = (dto.width, dto.height);
    let hide_cells = |cols: u32, rows: u32, values: &mut [u8], hidden: u8| {
        let (cols, rows) = (cols.max(1) as usize, rows.max(1) as usize);
        for r in 0..rows {
            for c in 0..cols {
                let p = ((c as f64 + 0.5) / cols as f64 * width, (r as f64 + 0.5) / rows as f64 * height);
                if !near(p) {
                    if let Some(v) = values.get_mut(r * cols + c) {
                        *v = hidden;
                    }
                }
            }
        }
    };
    if let Some(t) = dto.terrain.as_mut() {
        hide_cells(t.cols, t.rows, &mut t.heights, 0);
    }
    if let Some(s) = dto.style.as_mut() {
        hide_cells(s.cols, s.rows, &mut s.surface, crate::database::universe_maps::SURFACE_HIDDEN);
        hide_cells(s.cols, s.rows, &mut s.states, 0);
        // Couleurs des seuls États encore visibles (le nombre d'États ne fuit pas non plus).
        let seen: HashSet<u8> = s.states.iter().copied().collect();
        for (i, c) in s.state_colors.iter_mut().enumerate() {
            if !seen.contains(&(i as u8)) {
                c.clear();
            }
        }
        // Contours vectoriels : seulement les cellules des zones découvertes (rien d'une cellule
        // cachée n'est envoyé ; sans cellules, aucun contour).
        s.vector = s.vector_source.as_ref().map(|src| {
            let pts = &src.0.points;
            crate::activity::map::vector::vector_dto(&src.0, |c| pts.get(c).is_some_and(|&p| near(p)))
        });
        // Rivières : seuls leurs tronçons dans les zones découvertes.
        let rivers = std::mem::take(&mut s.rivers);
        for river in rivers {
            let mut run: Vec<[f64; 2]> = vec![];
            for p in river.points.iter().copied().chain(std::iter::once([f64::NAN, f64::NAN])) {
                if p[0].is_finite() && near((p[0], p[1])) {
                    run.push(p);
                } else if !run.is_empty() {
                    if run.len() >= 2 {
                        s.rivers.push(crate::database::universe_maps::StyleRiver { points: std::mem::take(&mut run), width: river.width });
                    }
                    run.clear();
                }
            }
        }
    }
    dto.background = None;
    dto.zone = None;
    let sorted = |set: HashSet<String>| {
        let mut v: Vec<String> = set.into_iter().collect();
        v.sort();
        v
    };
    let roads_kept: HashSet<String> = dto.roads.iter().filter(|r| roads.contains(&r.id)).map(|r| r.id.clone()).collect();
    dto.fog = Some(FogDto { radius, discovered: sorted(discovered), glimpsed: sorted(glimpsed), roads: sorted(roads_kept) });
    dto
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::activity::map_view::{MapPlaceDto, MapRoadDto, MapTerrainDto};
    use crate::database::universe_maps::MapSource;

    /// Ligne A(1)–B(2)–C(3)–D(4), un lieu isolé E(5) ; routes 12, 23, 34.
    fn map() -> MapDto {
        let place = |id: u64, x: f64| MapPlaceDto { id: id.to_string(), name: format!("P{id}"), x, y: 50.0, z: 0.0, burg: None };
        let road = |id: u64, a: u64, b: u64| MapRoadDto { id: id.to_string(), name: String::new(), from: a.to_string(), to: b.to_string(), distance_km: 10.0, secret: false, path: None };
        MapDto {
            source: MapSource::Azgaar,
            width: 100.0,
            height: 100.0,
            places: vec![place(1, 10.0), place(2, 30.0), place(3, 50.0), place(4, 70.0), place(5, 95.0)],
            roads: vec![road(12, 1, 2), road(23, 2, 3), road(34, 3, 4)],
            terrain: Some(MapTerrainDto { cols: 10, rows: 1, heights: vec![200; 10] }),
            zone: None,
            background: Some(crate::activity::map_view::MapBackgroundDto { url: "/api/map/background".into(), width: 100, height: 100 }),
            style: None,
            server_time: 0,
            fog: None,
        }
    }

    fn ids(v: &[String]) -> Vec<&str> {
        v.iter().map(String::as_str).collect()
    }

    #[test]
    fn a_newcomer_sees_only_where_he_is_and_the_next_places() {
        let d = Discovery { here: Some(2), ..Default::default() };
        let out = apply_fog(map(), &d);
        let fog = out.fog.clone().unwrap();
        assert_eq!(ids(&fog.discovered), vec!["2"]);
        assert_eq!(ids(&fog.glimpsed), vec!["1", "3"], "destinations from here");
        assert_eq!(out.places.iter().map(|p| p.id.as_str()).collect::<Vec<_>>(), vec!["1", "2", "3"], "D and E are never sent");
        assert_eq!(out.roads.iter().map(|r| r.id.as_str()).collect::<Vec<_>>(), vec!["12", "23"]);
        assert!(fog.roads.is_empty(), "no road travelled yet");
        assert!(out.background.is_none(), "the Azgaar image is never sent under fog");
    }

    #[test]
    fn travelled_roads_and_their_ends_are_discovered() {
        let d = Discovery { here: Some(4), known_roads: [23, 34].into_iter().collect(), ..Default::default() };
        let out = apply_fog(map(), &d);
        let fog = out.fog.unwrap();
        assert_eq!(ids(&fog.discovered), vec!["2", "3", "4"]);
        assert_eq!(ids(&fog.roads), vec!["23", "34"]);
        assert!(!out.places.iter().any(|p| p.id == "5"));
    }

    #[test]
    fn relief_is_erased_far_from_discovered_places() {
        let d = Discovery { here: Some(1), ..Default::default() };
        let out = apply_fog(map(), &d);
        let h = out.terrain.unwrap().heights;
        // Rayon : 60 % de 20 = 12, portée 15,6 autour de A (x = 10) : cellules x = 5, 15 et 25.
        assert_eq!(h, vec![200, 200, 200, 0, 0, 0, 0, 0, 0, 0]);
    }

    #[test]
    fn vector_contours_only_cover_discovered_cells() {
        use crate::activity::map_view::{MapStyleDto, VectorSourceRef};
        use crate::database::universe_maps::{PackCells, PACK_LAND};
        // Cellules 10 × 10 sur la carte de 100 × 100, biome = colonne + 1 (chaque colonne visible
        // se reconnaît), entourées d'un anneau de bordure.
        let mut pts = vec![];
        for r in 0..10 {
            for c in 0..10 {
                pts.push((c as f64 * 10.0 + 5.0 + (r % 3) as f64 * 0.2, r as f64 * 10.0 + 5.0));
            }
        }
        let biome: Vec<u8> = (0..100).map(|i| (i % 10) as u8 + 1).collect();
        for i in 0..40 {
            let a = i as f64 / 40.0 * std::f64::consts::TAU;
            pts.push((50.0 + 90.0 * a.cos(), 50.0 + 90.0 * a.sin()));
        }
        let pack = PackCells { real: 100, points: PackCells::encode_points(&pts), biome, state: vec![0; 100], water: vec![PACK_LAND; 100] };
        let src = crate::activity::map::vector::source(mongodb::bson::oid::ObjectId::new(), 1, &pack).unwrap();
        let mut m = map();
        m.style = Some(MapStyleDto {
            cols: 1, rows: 1, surface: vec![4], states: vec![], biome_colors: vec![], state_colors: vec![], rivers: vec![], exact: true,
            vector: Some(crate::activity::map::vector::vector_dto(&src, |_| true)),
            vector_source: Some(VectorSourceRef(src)),
        });
        assert_eq!(m.style.as_ref().unwrap().vector.as_ref().unwrap().biomes.len(), 10, "every column without fog");
        let out = apply_fog(m, &Discovery { here: Some(1), ..Default::default() });
        let v = out.style.unwrap().vector.unwrap();
        let seen: Vec<u8> = v.biomes.iter().map(|b| b.0).collect();
        // Autour de A (x = 10) : seulement les premières colonnes, rien au-delà de la portée.
        assert!(!seen.is_empty() && seen.iter().all(|b| *b <= 3), "nothing from hidden cells: {seen:?}");
    }

    #[test]
    fn the_azgaar_style_is_erased_and_rivers_cut_far_from_discovered_places() {
        use crate::activity::map_view::MapStyleDto;
        use crate::database::universe_maps::{StyleRiver, SURFACE_HIDDEN};
        let mut m = map();
        m.style = Some(MapStyleDto {
            cols: 10, rows: 1, surface: vec![4; 10], states: vec![1, 1, 1, 1, 1, 2, 2, 2, 2, 2],
            biome_colors: vec![], state_colors: vec![String::new(), "#ff0000".into(), "#00ff00".into()],
            rivers: vec![StyleRiver { points: vec![[5.0, 50.0], [15.0, 50.0], [60.0, 50.0], [85.0, 50.0], [95.0, 50.0]], width: [0.3, 1.0] }],
            exact: true,
            vector: None,
            vector_source: None,
        });
        let out = apply_fog(m, &Discovery { here: Some(1), ..Default::default() });
        let s = out.style.unwrap();
        let h = SURFACE_HIDDEN;
        assert_eq!(s.surface, vec![4, 4, 4, h, h, h, h, h, h, h], "same cells as the relief");
        assert_eq!(s.states, vec![1, 1, 1, 0, 0, 0, 0, 0, 0, 0]);
        assert_eq!(s.state_colors, vec!["", "#ff0000", ""], "the eastern state is not revealed");
        assert_eq!(s.rivers, vec![StyleRiver { points: vec![[5.0, 50.0], [15.0, 50.0]], width: [0.3, 1.0] }], "only the discovered stretch");
    }

    #[test]
    fn a_trip_in_progress_reveals_both_ends() {
        let d = Discovery { here: None, trip: vec![2, 3], current_road: Some(23), ..Default::default() };
        let out = apply_fog(map(), &d);
        let fog = out.fog.unwrap();
        assert_eq!(ids(&fog.discovered), vec!["2", "3"]);
        assert_eq!(ids(&fog.roads), vec!["23"]);
    }

    #[test]
    fn the_fog_is_lifted_only_in_admin_mode_for_those_allowed() {
        use AccessLevel::*;
        for access in [Player, Moderator, Admin] {
            assert!(fog_applies(true, access, false), "{access:?} outside admin mode sees the fog");
            assert!(!fog_applies(false, access, true) && !fog_applies(false, access, false), "no fog when disabled");
        }
        assert!(fog_applies(true, Player, true), "a player cannot lift the fog by asking");
        assert!(!fog_applies(true, Moderator, true));
        assert!(!fog_applies(true, Admin, true));
    }

    #[test]
    fn the_radius_follows_the_road_lengths() {
        assert!((fog_radius(&map()) - 12.0).abs() < 1e-9);
        let mut m = map();
        m.roads.clear();
        assert!((fog_radius(&m) - 5.0).abs() < 1e-9);
    }
}
