//! Carte vue par un joueur (`GET /api/map`) : logique pure, testée sans base.
//!
//! Le filtrage des routes secrètes est fait **ici, côté serveur** : une route secrète inconnue du
//! joueur n'est jamais envoyée, et un lieu qui n'est relié que par de telles routes est masqué
//! (sinon sa simple présence trahirait l'existence d'une route secrète).

use std::collections::HashSet;

use mongodb::bson::oid::ObjectId;
use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::activity::map::layout::{compute_layout, Edge, CANVAS};
use crate::database::places::Place;
use crate::database::road::Road;
use crate::database::universe_maps::{MapPosition, MapSource, MapZone, UniverseMap};

/// Empreinte des lieux et routes (secrètes comprises) qui déterminent le placement calculé.
/// L'ordre de lecture en base n'a pas d'effet.
pub fn graph_hash(places: &[Place], roads: &[Road]) -> String {
    let mut place_ids: Vec<u64> = places.iter().map(|p| p.category_id).collect();
    place_ids.sort_unstable();
    let mut edges: Vec<(u64, u64, u64, u64)> = roads
        .iter()
        .map(|r| (r.channel_id, r.place_one_id.min(r.place_two_id), r.place_one_id.max(r.place_two_id), r.distance))
        .collect();
    edges.sort_unstable();
    let mut hasher = Sha256::new();
    for id in place_ids {
        hasher.update(id.to_le_bytes());
    }
    hasher.update(b"|");
    for (c, a, b, d) in edges {
        for v in [c, a, b, d] {
            hasher.update(v.to_le_bytes());
        }
    }
    hasher.finalize().iter().map(|b| format!("{b:02x}")).collect()
}

/// Vrai si la carte en cache doit être (re)calculée.
pub fn needs_layout(stored: Option<&UniverseMap>, hash: &str) -> bool {
    match stored {
        None => true,
        Some(map) => map.source == MapSource::Computed && map.graph_hash.as_deref() != Some(hash),
    }
}

/// Placement calculé à partir des distances des routes (toutes, côté serveur uniquement).
pub fn computed_map(universe_id: ObjectId, places: &[Place], roads: &[Road], now: u64) -> UniverseMap {
    let nodes: Vec<u64> = places.iter().map(|p| p.category_id).collect();
    let edges: Vec<Edge> = roads.iter().map(|r| Edge { a: r.place_one_id, b: r.place_two_id, length: r.distance as f64 }).collect();
    let layout = compute_layout(&nodes, &edges);
    UniverseMap {
        id: ObjectId::new(),
        universe_id,
        source: MapSource::Computed,
        width: CANVAS,
        height: CANVAS,
        positions: layout.positions.iter().map(|(id, p)| MapPosition { place_id: *id, x: p.x, y: p.y, z: 0.0, azgaar_burg_id: None }).collect(),
        terrain: None,
        zone: None,
        graph_hash: Some(graph_hash(places, roads)),
        road_paths: vec![],
        background: None,
        updated_at: now,
    }
}

/// Lieux visibles : ceux sans aucune route, ceux reliés par au moins une route visible, et ceux
/// de `always` (position, destination du joueur).
pub fn visible_place_ids(places: &[Place], roads: &[Road], visible_roads: &HashSet<u64>, always: &[u64]) -> HashSet<u64> {
    places
        .iter()
        .map(|p| p.category_id)
        .filter(|id| {
            let mut touching = roads.iter().filter(|r| r.place_one_id == *id || r.place_two_id == *id).peekable();
            always.contains(id) || touching.peek().is_none() || touching.any(|r| visible_roads.contains(&r.channel_id))
        })
        .collect()
}

#[derive(Debug, Serialize, PartialEq)]
pub struct MapPlaceDto {
    pub id: String,
    pub name: String,
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct MapRoadDto {
    pub id: String,
    pub name: String,
    pub from: String,
    pub to: String,
    pub distance_km: f64,
    /// Seulement pour une route secrète déjà connue du joueur.
    pub secret: bool,
    /// Tracé de `from` vers `to` (import Azgaar). Absent : ligne droite.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<Vec<[f64; 2]>>,
}

/// Tracé d'une route orienté de `from` vers `to`.
pub fn oriented_path(paths: &[crate::database::universe_maps::RoadPath], from: u64, to: u64) -> Option<Vec<[f64; 2]>> {
    let p = paths.iter().find(|p| (p.a, p.b) == (from.min(to), from.max(to)))?;
    let mut points = p.points.clone();
    if from > to {
        points.reverse();
    }
    (points.len() >= 2).then_some(points)
}

#[derive(Debug, Serialize, PartialEq)]
pub struct MapTerrainDto {
    pub cols: u32,
    pub rows: u32,
    pub heights: Vec<u8>,
}

/// Fond de carte (image Azgaar) : URL versionnée par l'empreinte, donc mise en cache sans risque.
#[derive(Debug, Serialize, PartialEq)]
pub struct MapBackgroundDto {
    pub url: String,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Serialize)]
pub struct MapDto {
    pub source: MapSource,
    pub width: f64,
    pub height: f64,
    pub places: Vec<MapPlaceDto>,
    pub roads: Vec<MapRoadDto>,
    pub terrain: Option<MapTerrainDto>,
    pub zone: Option<MapZone>,
    pub background: Option<MapBackgroundDto>,
    pub server_time: u64,
    /// Brouillard (E8) : `None` s'il est désactivé ou pour un modérateur ou un administrateur.
    pub fog: Option<crate::activity::fog::FogDto>,
}

/// Assemble la carte envoyée au joueur. `visible_roads` : `channel_id` des routes qu'il peut voir
/// (voir `travel::service::road_is_visible`).
pub fn build_map_dto(map: &UniverseMap, places: &[Place], roads: &[Road], visible_roads: &HashSet<u64>, always: &[u64], now: u64) -> MapDto {
    let shown = visible_place_ids(places, roads, visible_roads, always);
    let mut place_dtos: Vec<MapPlaceDto> = places
        .iter()
        .filter(|p| shown.contains(&p.category_id))
        .filter_map(|p| {
            let pos = map.positions.iter().find(|m| m.place_id == p.category_id)?;
            Some(MapPlaceDto { id: p.category_id.to_string(), name: p.name.clone(), x: pos.x, y: pos.y, z: pos.z })
        })
        .collect();
    place_dtos.sort_by(|a, b| a.id.cmp(&b.id));
    let placed: HashSet<&str> = place_dtos.iter().map(|p| p.id.as_str()).collect();
    let mut road_dtos: Vec<MapRoadDto> = roads
        .iter()
        .filter(|r| visible_roads.contains(&r.channel_id))
        .filter(|r| placed.contains(r.place_one_id.to_string().as_str()) && placed.contains(r.place_two_id.to_string().as_str()))
        .map(|r| MapRoadDto {
            id: r.channel_id.to_string(),
            name: r.road_name.clone(),
            from: r.place_one_id.to_string(),
            to: r.place_two_id.to_string(),
            distance_km: r.distance as f64,
            secret: r.secret,
            path: oriented_path(&map.road_paths, r.place_one_id, r.place_two_id),
        })
        .collect();
    road_dtos.sort_by(|a, b| a.id.cmp(&b.id));
    MapDto {
        source: map.source,
        width: map.width,
        height: map.height,
        places: place_dtos,
        roads: road_dtos,
        terrain: map.terrain.as_ref().map(|t| MapTerrainDto { cols: t.cols, rows: t.rows, heights: t.heights.clone() }),
        zone: map.zone.clone(),
        background: map.background.as_ref().map(|b| MapBackgroundDto {
            url: format!("/api/map/background?v={}", &b.sha256[..b.sha256.len().min(16)]),
            width: b.width,
            height: b.height,
        }),
        server_time: now,
        fog: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn place(id: u64) -> Place {
        Place { _id: ObjectId::new(), universe_id: ObjectId::new(), server_id: 1, category_id: id, role: 0, name: format!("P{id}"), modifiers: vec![], central_channel_id: None }
    }

    fn road(channel: u64, a: u64, b: u64, distance: u64, secret: bool) -> Road {
        crate::travel::service::tests_support_road(ObjectId::new(), channel, a, b, secret).with_distance(distance)
    }

    trait WithDistance { fn with_distance(self, d: u64) -> Self; }
    impl WithDistance for Road { fn with_distance(mut self, d: u64) -> Self { self.distance = d; self } }

    /// A —100— B —(secrète 50)— C ; D isolé.
    fn world() -> (Vec<Place>, Vec<Road>) {
        (vec![place(1), place(2), place(3), place(4)], vec![road(10, 1, 2, 100, false), road(20, 2, 3, 50, true)])
    }

    #[test]
    fn the_hash_ignores_the_read_order_and_detects_changes() {
        let (places, roads) = world();
        let mut shuffled_places = places.clone();
        shuffled_places.reverse();
        let reversed_roads: Vec<Road> = roads.iter().rev().cloned().collect();
        let h = graph_hash(&places, &roads);
        assert_eq!(h, graph_hash(&shuffled_places, &reversed_roads));
        let mut longer = roads.clone();
        longer[0].distance = 101;
        assert_ne!(h, graph_hash(&places, &longer));
        assert_ne!(h, graph_hash(&places[..3], &roads));
    }

    #[test]
    fn only_a_stale_computed_map_is_recomputed() {
        let (places, roads) = world();
        let hash = graph_hash(&places, &roads);
        let mut map = computed_map(ObjectId::new(), &places, &roads, 0);
        assert!(needs_layout(None, &hash));
        assert!(!needs_layout(Some(&map), &hash));
        assert!(needs_layout(Some(&map), "other"));
        map.source = MapSource::Azgaar;
        assert!(!needs_layout(Some(&map), "other"), "an imported map is never replaced automatically");
    }

    #[test]
    fn the_computed_map_respects_road_lengths() {
        let (places, roads) = world();
        let map = computed_map(ObjectId::new(), &places, &roads, 0);
        let pos = |id| map.positions.iter().find(|p| p.place_id == id).map(|p| (p.x, p.y)).unwrap();
        let d = |a, b| { let ((x1, y1), (x2, y2)) = (pos(a), pos(b)); ((x1 - x2).powi(2) + (y1 - y2).powi(2)).sqrt() };
        assert!((d(1, 2) / d(2, 3) - 2.0).abs() < 0.05, "100 km vs 50 km");
        assert_eq!(map.positions.len(), 4);
    }

    #[test]
    fn a_secret_road_and_its_hidden_place_are_not_sent() {
        let (places, roads) = world();
        let map = computed_map(ObjectId::new(), &places, &roads, 0);
        let dto = build_map_dto(&map, &places, &roads, &HashSet::from([10]), &[1], 0);
        let ids: Vec<&str> = dto.places.iter().map(|p| p.id.as_str()).collect();
        assert_eq!(ids, vec!["1", "2", "4"], "C only reachable by an unknown secret road; D has no road");
        assert_eq!(dto.roads.len(), 1);
        assert_eq!(dto.roads[0].id, "10");
        assert!(!serde_json::to_string(&dto).unwrap().contains("\"20\""));
    }

    #[test]
    fn a_known_secret_road_is_sent_and_flagged() {
        let (places, roads) = world();
        let map = computed_map(ObjectId::new(), &places, &roads, 0);
        let dto = build_map_dto(&map, &places, &roads, &HashSet::from([10, 20]), &[1], 0);
        assert_eq!(dto.places.len(), 4);
        let secret = dto.roads.iter().find(|r| r.id == "20").unwrap();
        assert!(secret.secret);
        assert_eq!(secret.distance_km, 50.0);
    }

    #[test]
    fn the_players_own_place_is_always_visible() {
        let (places, roads) = world();
        let shown = visible_place_ids(&places, &roads, &HashSet::from([10]), &[3]);
        assert!(shown.contains(&3));
    }

    #[test]
    fn the_background_url_carries_its_version() {
        let (places, roads) = world();
        let mut map = computed_map(ObjectId::new(), &places, &roads, 0);
        assert!(build_map_dto(&map, &places, &roads, &HashSet::new(), &[], 0).background.is_none());
        map.background = Some(crate::database::map_backgrounds::MapBackground {
            file_id: ObjectId::new(), content_type: "image/png".into(), width: 3000, height: 2000,
            sha256: "0123456789abcdef0123".into(), bytes: 10,
        });
        let dto = build_map_dto(&map, &places, &roads, &HashSet::new(), &[], 0);
        assert_eq!(dto.background, Some(MapBackgroundDto { url: "/api/map/background?v=0123456789abcdef".into(), width: 3000, height: 2000 }));
    }

    #[test]
    fn road_paths_are_sent_in_the_direction_of_the_road() {
        use crate::database::universe_maps::RoadPath;
        let (places, mut roads) = world();
        let mut map = computed_map(ObjectId::new(), &places, &roads, 0);
        map.road_paths = vec![RoadPath { a: 1, b: 2, points: vec![[0.0, 0.0], [5.0, 3.0], [10.0, 0.0]] }];
        // Route enregistrée de 2 vers 1 : le tracé est inversé.
        roads[0].place_one_id = 2;
        roads[0].place_two_id = 1;
        let dto = build_map_dto(&map, &places, &roads, &HashSet::from([10, 20]), &[], 0);
        let r = dto.roads.iter().find(|r| r.id == "10").unwrap();
        assert_eq!(r.path.as_deref(), Some(&[[10.0, 0.0], [5.0, 3.0], [0.0, 0.0]][..]));
        assert!(dto.roads.iter().find(|r| r.id == "20").unwrap().path.is_none(), "no path: straight line");
    }

    #[test]
    fn places_without_position_are_skipped() {
        let (places, roads) = world();
        let mut map = computed_map(ObjectId::new(), &places, &roads, 0);
        map.positions.retain(|p| p.place_id != 2);
        let dto = build_map_dto(&map, &places, &roads, &HashSet::from([10]), &[], 0);
        assert!(dto.places.iter().all(|p| p.id != "2"));
        assert!(dto.roads.is_empty(), "a road to an unplaced place cannot be drawn");
    }
}
