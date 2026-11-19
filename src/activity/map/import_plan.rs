//! Plan d'import d'une carte Azgaar (étape E3 de `docs/activity/IMPORT_SPEC.md`) : à partir de la
//! carte et des choix de l'admin, ce qu'il faut créer, garder, mettre à jour ou supprimer, et sur
//! quel serveur. Logique pure : aucun appel à Discord ni à la base. L'aperçu (E4, E5) affiche ce
//! plan ; la création (E4) l'exécute avec le `Provisioner`.
//!
//! Trois modes :
//! - [`ImportMode::Replace`] (« Supprimer les lieux existants », I15) : tout l'existant est
//!   listé pour suppression, tout est créé ;
//! - [`ImportMode::Merge`] (« Créer en plus », I16) : les lieux existants rattachés à un bourg
//!   par l'admin sont gardés (`Attach`), les autres bourgs sont créés ;
//! - [`ImportMode::Reimport`] (I10) : les lieux issus d'un import précédent (bourg connu) sont
//!   gardés, avec leur nom ; ce qui a disparu de la carte est retiré ; rien n'est recréé.
//!
//! Distances : celles du tracé Azgaar, arrondies au km, 1 km au moins (I8 : aucun
//! multiplicateur).

use std::collections::{HashMap, HashSet};

use serde_with::{serde_as, DisplayFromStr};

use crate::activity::map::azgaar::AzgaarMap;
use crate::activity::map::network::{road_candidates, Zone};
use crate::activity::map::partition::{partition, Edge, Node, PartitionError, PartitionInput, ServerBudget, ServerLoad, DEFAULT_MAX_PLACES_PER_SERVER};

/// Distance maximale (pixels) entre un bourg et le point de route le plus proche pour l'y relier.
pub const SNAP_PX: f64 = 6.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImportMode {
    Replace,
    Merge,
    Reimport,
}

/// Lieu déjà présent dans l'univers.
#[derive(Debug, Clone, PartialEq)]
pub struct ExistingPlace {
    pub category_id: u64,
    pub name: String,
    pub server_id: u64,
    /// Bourg Azgaar d'origine (import précédent, `universe_maps.positions`).
    pub burg_id: Option<u64>,
}

/// Route déjà présente dans l'univers (lieux désignés par `category_id`).
#[derive(Debug, Clone, PartialEq)]
pub struct ExistingRoad {
    pub channel_id: u64,
    pub place_a: u64,
    pub place_b: u64,
    pub distance: u64,
    pub secret: bool,
    pub server_id: u64,
}

/// Serveur choisi par l'admin (I9), avec ce qu'il contient déjà.
#[derive(Debug, Clone, PartialEq)]
pub struct KnownServer {
    pub guild_id: u64,
    pub load: ServerLoad,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ImportOptions {
    pub mode: ImportMode,
    pub zone: Zone,
    /// Routes maritimes incluses (I7, décoché par défaut).
    pub include_sea: bool,
    /// Bourgs retenus ; `None` : tous ceux de la zone.
    pub burgs: Option<Vec<u64>>,
    /// Routes secrètes choisies dans l'aperçu (paires de bourgs).
    pub secret: HashSet<(u64, u64)>,
    /// Mode `Merge` : bourg → lieu existant, choisi par l'admin (proposé par `match_places`).
    pub attach: HashMap<u64, u64>,
    pub servers: Vec<KnownServer>,
    /// Charge d'un serveur à ajouter (son installation, `setup_budget`).
    pub new_server_load: ServerLoad,
    pub max_places: usize,
}

impl Default for ImportOptions {
    fn default() -> Self {
        Self {
            mode: ImportMode::Replace,
            zone: Zone::default(),
            include_sea: false,
            burgs: None,
            secret: HashSet::new(),
            attach: HashMap::new(),
            servers: vec![],
            new_server_load: ServerLoad { channels: 11, roles: 4 },
            max_places: DEFAULT_MAX_PLACES_PER_SERVER,
        }
    }
}

#[serde_as]
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PlaceAction {
    Create,
    /// Lieu d'un import précédent, gardé (son nom ne change pas).
    Keep {
        #[serde_as(as = "DisplayFromStr")]
        category_id: u64,
    },
    /// Lieu existant rattaché à ce bourg.
    Attach {
        #[serde_as(as = "DisplayFromStr")]
        category_id: u64,
    },
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct PlannedPlace {
    pub burg_id: u64,
    pub name: String,
    pub x: f64,
    pub y: f64,
    pub capital: bool,
    pub action: PlaceAction,
    /// Index du serveur dans [`ImportPlan::servers`].
    pub server: usize,
}

#[serde_as]
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RoadAction {
    Create,
    Keep {
        #[serde_as(as = "DisplayFromStr")]
        channel_id: u64,
    },
    /// Route existante dont la distance ou le secret change.
    Update {
        #[serde_as(as = "DisplayFromStr")]
        channel_id: u64,
    },
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct PlannedRoad {
    pub burg_a: u64,
    pub burg_b: u64,
    pub distance: u64,
    pub uses_sea: bool,
    pub secret: bool,
    pub path: Vec<(f64, f64)>,
    pub action: RoadAction,
    /// Serveur qui héberge le salon.
    pub host: usize,
}

#[serde_as]
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct PlannedServer {
    /// `None` : serveur à créer par l'admin (un bot ne peut pas créer de serveur).
    #[serde_as(as = "Option<DisplayFromStr>")]
    pub guild_id: Option<u64>,
    pub budget: ServerBudget,
}

#[serde_as]
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct ImportPlan {
    pub places: Vec<PlannedPlace>,
    pub roads: Vec<PlannedRoad>,
    /// Lieux existants à supprimer (`category_id`).
    #[serde_as(as = "Vec<DisplayFromStr>")]
    pub remove_places: Vec<u64>,
    /// Routes existantes à supprimer (`channel_id`).
    #[serde_as(as = "Vec<DisplayFromStr>")]
    pub remove_roads: Vec<u64>,
    pub servers: Vec<PlannedServer>,
    /// Routes entre deux serveurs.
    pub cut_roads: usize,
    /// Bourgs retenus qu'aucune route ne relie.
    pub isolated: Vec<u64>,
}

impl ImportPlan {
    /// Nombre de serveurs que l'admin doit encore créer et où inviter le bot.
    pub fn servers_to_create(&self) -> usize {
        self.servers.iter().filter(|s| s.guild_id.is_none()).count()
    }

    /// Objets Discord à créer : 3 par lieu (rôle, catégorie, salon), 1 par route, plus les
    /// catégories de routes. Sert au total de la barre de progression.
    pub fn discord_objects_to_create(&self) -> u32 {
        let places = self.places.iter().filter(|p| p.action == PlaceAction::Create).count() as u32;
        let roads = self.roads.iter().filter(|r| r.action == RoadAction::Create).count() as u32;
        let cats: u32 = self.servers.iter().map(|s| s.budget.road_categories).sum();
        3 * places + roads + cats
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlanError {
    /// Un lieu existant est sur un serveur que l'admin n'a pas choisi.
    ServerNotSelected(u64),
    /// Le rattachement désigne un lieu qui n'existe pas.
    UnknownPlace(u64),
    /// Deux bourgs rattachés au même lieu.
    PlaceAttachedTwice(u64),
    /// Trop de lieux existants sur un serveur pour la limite de lieux.
    ServerOverflow(u64),
    NoBurg,
}

impl PlanError {
    pub fn key(&self) -> &'static str {
        match self {
            PlanError::ServerNotSelected(_) => "activity__import_server_not_selected",
            PlanError::UnknownPlace(_) => "activity__import_unknown_place",
            PlanError::PlaceAttachedTwice(_) => "activity__import_place_attached_twice",
            PlanError::ServerOverflow(_) => "activity__import_server_overflow",
            PlanError::NoBurg => "activity__import_no_burg",
        }
    }
}

/// Distance enregistrée pour une route : km arrondis, 1 au moins.
pub fn road_distance(km: f64) -> u64 {
    if km.is_finite() { km.round().max(1.0) as u64 } else { 1 }
}

fn pair(a: u64, b: u64) -> (u64, u64) {
    (a.min(b), a.max(b))
}

/// Construit le plan. Voir le module.
pub fn build_plan(map: &AzgaarMap, options: &ImportOptions, existing_places: &[ExistingPlace], existing_roads: &[ExistingRoad]) -> Result<ImportPlan, PlanError> {
    // Bourgs retenus, dans la zone, dans l'ordre de la carte.
    let wanted: Option<HashSet<u64>> = options.burgs.as_ref().map(|v| v.iter().copied().collect());
    let burgs: Vec<_> = map
        .burgs
        .iter()
        .filter(|b| wanted.as_ref().is_none_or(|w| w.contains(&b.id)) && options.zone.contains(b.x, b.y))
        .collect();
    if burgs.is_empty() {
        return Err(PlanError::NoBurg);
    }
    let burg_ids: Vec<u64> = burgs.iter().map(|b| b.id).collect();
    let retained: HashSet<u64> = burg_ids.iter().copied().collect();

    // Lieu existant associé à chaque bourg.
    let by_cat: HashMap<u64, &ExistingPlace> = existing_places.iter().map(|p| (p.category_id, p)).collect();
    let mut linked: HashMap<u64, (u64, bool)> = HashMap::new(); // bourg → (catégorie, import précédent)
    match options.mode {
        ImportMode::Replace => {}
        ImportMode::Reimport => {
            for p in existing_places {
                if let Some(b) = p.burg_id.filter(|b| retained.contains(b)) {
                    linked.insert(b, (p.category_id, true));
                }
            }
        }
        ImportMode::Merge => {
            let mut used = HashSet::new();
            for (&burg, &cat) in &options.attach {
                if !retained.contains(&burg) {
                    continue;
                }
                if !by_cat.contains_key(&cat) {
                    return Err(PlanError::UnknownPlace(cat));
                }
                if !used.insert(cat) {
                    return Err(PlanError::PlaceAttachedTwice(cat));
                }
                linked.insert(burg, (cat, false));
            }
        }
    }

    let server_index: HashMap<u64, usize> = options.servers.iter().enumerate().map(|(i, s)| (s.guild_id, i)).collect();
    let server_of_place = |cat: u64| -> Result<usize, PlanError> {
        let guild = by_cat[&cat].server_id;
        server_index.get(&guild).copied().ok_or(PlanError::ServerNotSelected(guild))
    };

    // Routes candidates.
    let candidates = road_candidates(map, &burg_ids, &options.zone, options.include_sea, SNAP_PX);
    let index: HashMap<u64, usize> = burg_ids.iter().enumerate().map(|(i, b)| (*b, i)).collect();
    let road_by_places: HashMap<(u64, u64), &ExistingRoad> = existing_roads.iter().map(|r| (pair(r.place_a, r.place_b), r)).collect();

    let mut nodes = Vec::with_capacity(burgs.len());
    for b in &burgs {
        let (pinned, existing) = match linked.get(&b.id) {
            Some(&(cat, _)) => (Some(server_of_place(cat)?), true),
            None => (None, false),
        };
        nodes.push(Node { x: b.x, y: b.y, pinned, existing });
    }
    let mut edges = Vec::with_capacity(candidates.len());
    let mut existing_of_candidate = Vec::with_capacity(candidates.len());
    for c in &candidates {
        let existing = match (linked.get(&c.burg_a), linked.get(&c.burg_b)) {
            (Some(&(ca, _)), Some(&(cb, _))) => road_by_places.get(&pair(ca, cb)).copied(),
            _ => None,
        };
        let existing_host = match existing {
            Some(r) => Some(server_index.get(&r.server_id).copied().ok_or(PlanError::ServerNotSelected(r.server_id))?),
            None => None,
        };
        edges.push(Edge { a: index[&c.burg_a], b: index[&c.burg_b], length: c.distance_km, existing_host });
        existing_of_candidate.push(existing);
    }

    let input = PartitionInput {
        nodes,
        edges,
        servers: options.servers.iter().map(|s| s.load).collect(),
        new_server_load: options.new_server_load,
        max_places: options.max_places,
    };
    let part = partition(&input).map_err(|e| match e {
        PartitionError::PinnedOverflow(s) => PlanError::ServerOverflow(options.servers[s].guild_id),
        PartitionError::BadPin | PartitionError::NoSolution => PlanError::ServerOverflow(0),
    })?;

    let places = burgs
        .iter()
        .enumerate()
        .map(|(i, b)| PlannedPlace {
            burg_id: b.id,
            name: match linked.get(&b.id) {
                Some(&(cat, _)) => by_cat[&cat].name.clone(),
                None => b.name.clone(),
            },
            x: b.x,
            y: b.y,
            capital: b.capital,
            action: match linked.get(&b.id) {
                Some(&(cat, true)) => PlaceAction::Keep { category_id: cat },
                Some(&(cat, false)) => PlaceAction::Attach { category_id: cat },
                None => PlaceAction::Create,
            },
            server: part.server_of[i],
        })
        .collect();

    let mut kept_roads = HashSet::new();
    let roads = candidates
        .iter()
        .zip(&existing_of_candidate)
        .enumerate()
        .map(|(i, (c, existing))| {
            let distance = road_distance(c.distance_km);
            let secret = options.secret.contains(&pair(c.burg_a, c.burg_b));
            let action = match existing {
                None => RoadAction::Create,
                Some(r) => {
                    kept_roads.insert(r.channel_id);
                    if r.distance == distance && r.secret == secret { RoadAction::Keep { channel_id: r.channel_id } } else { RoadAction::Update { channel_id: r.channel_id } }
                }
            };
            PlannedRoad { burg_a: c.burg_a, burg_b: c.burg_b, distance, uses_sea: c.uses_sea, secret, path: c.path.clone(), action, host: part.host_of[i] }
        })
        .collect();

    // Ce qui disparaît.
    let kept_places: HashSet<u64> = linked.values().map(|(c, _)| *c).collect();
    let (remove_places, remove_roads) = match options.mode {
        ImportMode::Replace => (existing_places.iter().map(|p| p.category_id).collect(), existing_roads.iter().map(|r| r.channel_id).collect()),
        ImportMode::Merge => (vec![], vec![]),
        ImportMode::Reimport => {
            let imported: HashSet<u64> = existing_places.iter().filter(|p| p.burg_id.is_some()).map(|p| p.category_id).collect();
            let gone: Vec<u64> = existing_places.iter().filter(|p| p.burg_id.is_some() && !kept_places.contains(&p.category_id)).map(|p| p.category_id).collect();
            let gone_set: HashSet<u64> = gone.iter().copied().collect();
            // Routes d'import : entre deux lieux importés. Retirées si elles ne sont plus dans
            // la carte, ou si l'un de leurs lieux disparaît.
            let roads = existing_roads
                .iter()
                .filter(|r| {
                    let touches_gone = gone_set.contains(&r.place_a) || gone_set.contains(&r.place_b);
                    let import_road = imported.contains(&r.place_a) && imported.contains(&r.place_b);
                    touches_gone || (import_road && !kept_roads.contains(&r.channel_id))
                })
                .map(|r| r.channel_id)
                .collect();
            (gone, roads)
        }
    };

    let servers = part
        .budgets
        .iter()
        .enumerate()
        .map(|(i, b)| PlannedServer { guild_id: options.servers.get(i).map(|s| s.guild_id), budget: *b })
        .collect();
    let linked_burgs: HashSet<u64> = candidates.iter().flat_map(|c| [c.burg_a, c.burg_b]).collect();
    let isolated = burg_ids.iter().copied().filter(|b| !linked_burgs.contains(b)).collect();

    Ok(ImportPlan { places, roads, remove_places, remove_roads, servers, cut_roads: part.cut, isolated })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::activity::map::azgaar::parse;

    /// A(0,0) — B(20,0) — D(30,0) sur une route ; C(10,10) par une bifurcation ; E isolé ;
    /// une route maritime A–D.
    const MAP: &str = r#"{
      "info": {"width": 100, "height": 100},
      "settings": {"distanceScale": 2, "distanceUnit": "km"},
      "pack": {
        "burgs": [{},
          {"i":1,"name":"A","x":0,"y":0,"cell":1,"capital":1},
          {"i":2,"name":"B","x":20,"y":0,"cell":3},
          {"i":3,"name":"C","x":10,"y":10,"cell":4},
          {"i":4,"name":"D","x":30,"y":0,"cell":5},
          {"i":5,"name":"E","x":90,"y":90,"cell":9}
        ],
        "routes": [
          {"i":0,"group":"roads","points":[[0,0,1],[10,0,2],[20,0,3],[30,0,5]]},
          {"i":1,"group":"trails","points":[[10,0,2],[10,10,4]]},
          {"i":2,"group":"searoutes","points":[[0,0,1],[0,40,7],[30,0,5]]}
        ]
      }
    }"#;

    fn server(guild_id: u64) -> KnownServer {
        KnownServer { guild_id, load: ServerLoad { channels: 11, roles: 4 } }
    }

    fn opts() -> ImportOptions {
        ImportOptions { servers: vec![server(100)], ..Default::default() }
    }

    fn road<'a>(p: &'a ImportPlan, a: u64, b: u64) -> Option<&'a PlannedRoad> {
        p.roads.iter().find(|r| (r.burg_a, r.burg_b) == pair(a, b))
    }

    #[test]
    fn a_new_map_creates_every_place_and_road() {
        let m = parse(MAP).unwrap();
        let p = build_plan(&m, &opts(), &[], &[]).unwrap();
        assert_eq!(p.places.len(), 5);
        assert!(p.places.iter().all(|x| x.action == PlaceAction::Create && x.server == 0));
        assert!(p.places[0].capital);
        // A–B, A–C, B–C, B–D (sans maritime) ; A–B passe par la bifurcation.
        assert_eq!(road(&p, 1, 2).unwrap().distance, 40);
        assert_eq!(road(&p, 2, 4).unwrap().distance, 20);
        assert!(road(&p, 1, 4).is_none(), "no sea road by default");
        assert_eq!(p.isolated, vec![5]);
        assert_eq!(p.servers_to_create(), 0);
        assert_eq!(p.discord_objects_to_create(), 3 * 5 + p.roads.len() as u32 + 1);
    }

    #[test]
    fn sea_roads_and_secrets_follow_the_options() {
        let m = parse(MAP).unwrap();
        let mut o = opts();
        o.include_sea = true;
        o.secret.insert((2, 4));
        let p = build_plan(&m, &o, &[], &[]).unwrap();
        assert!(road(&p, 2, 4).unwrap().secret);
        assert!(!road(&p, 1, 2).unwrap().secret);
        // La route maritime A–D (2 × (40 + 50) = 180 km) est bien plus longue que A–B–D (60 km) :
        // elle est retirée comme détour inutile, même incluse.
        assert!(road(&p, 1, 4).is_none());
    }

    #[test]
    fn a_selection_and_a_zone_restrict_the_burgs() {
        let m = parse(MAP).unwrap();
        let mut o = opts();
        o.burgs = Some(vec![1, 2, 4]);
        let p = build_plan(&m, &o, &[], &[]).unwrap();
        assert_eq!(p.places.iter().map(|x| x.burg_id).collect::<Vec<_>>(), vec![1, 2, 4]);
        o.burgs = None;
        o.zone = Zone { polygon: vec![(-5.0, -5.0), (25.0, -5.0), (25.0, 15.0), (-5.0, 15.0)] };
        let p = build_plan(&m, &o, &[], &[]).unwrap();
        assert_eq!(p.places.iter().map(|x| x.burg_id).collect::<Vec<_>>(), vec![1, 2, 3]);
        o.burgs = Some(vec![5]);
        assert_eq!(build_plan(&m, &o, &[], &[]), Err(PlanError::NoBurg));
    }

    #[test]
    fn discord_ids_are_serialized_as_strings() {
        let a = serde_json::to_value(PlaceAction::Keep { category_id: u64::MAX }).unwrap();
        assert_eq!(a, serde_json::json!({"kind": "keep", "category_id": u64::MAX.to_string()}));
        let s = serde_json::to_value(PlannedServer { guild_id: Some(1080809463201677332), budget: crate::activity::map::partition::budget(ServerLoad::default(), 0, 0) }).unwrap();
        assert_eq!(s["guild_id"], "1080809463201677332");
    }

    #[test]
    fn short_roads_are_at_least_one_km() {
        assert_eq!(road_distance(0.2), 1);
        assert_eq!(road_distance(12.5), 13);
        assert_eq!(road_distance(f64::NAN), 1);
    }

    fn existing(cat: u64, name: &str, burg: Option<u64>) -> ExistingPlace {
        ExistingPlace { category_id: cat, name: name.into(), server_id: 100, burg_id: burg }
    }

    #[test]
    fn replace_removes_everything_that_exists() {
        let m = parse(MAP).unwrap();
        let places = [existing(10, "Vieux", None)];
        let roads = [ExistingRoad { channel_id: 50, place_a: 10, place_b: 11, distance: 3, secret: false, server_id: 100 }];
        let p = build_plan(&m, &opts(), &places, &roads).unwrap();
        assert_eq!(p.remove_places, vec![10]);
        assert_eq!(p.remove_roads, vec![50]);
        assert!(p.places.iter().all(|x| x.action == PlaceAction::Create));
    }

    #[test]
    fn merge_attaches_chosen_places_and_keeps_their_roads() {
        let m = parse(MAP).unwrap();
        let places = [existing(10, "Ancien A", None), existing(11, "Ancien B", None), existing(12, "Autre", None)];
        let roads = [ExistingRoad { channel_id: 50, place_a: 11, place_b: 10, distance: 40, secret: false, server_id: 100 }];
        let mut o = opts();
        o.mode = ImportMode::Merge;
        o.attach = [(1, 10), (2, 11)].into_iter().collect();
        let p = build_plan(&m, &o, &places, &roads).unwrap();
        let a = p.places.iter().find(|x| x.burg_id == 1).unwrap();
        assert_eq!(a.action, PlaceAction::Attach { category_id: 10 });
        assert_eq!(a.name, "Ancien A", "an attached place keeps its name");
        assert_eq!(road(&p, 1, 2).unwrap().action, RoadAction::Keep { channel_id: 50 });
        assert!(p.remove_places.is_empty() && p.remove_roads.is_empty(), "merge never deletes");
        // Un lieu ne peut pas servir deux fois.
        o.attach = [(1, 10), (2, 10)].into_iter().collect();
        assert_eq!(build_plan(&m, &o, &places, &roads), Err(PlanError::PlaceAttachedTwice(10)));
        o.attach = [(1, 99)].into_iter().collect();
        assert_eq!(build_plan(&m, &o, &places, &roads), Err(PlanError::UnknownPlace(99)));
    }

    #[test]
    fn reimport_keeps_known_burgs_updates_changed_roads_and_removes_what_disappeared() {
        let m = parse(MAP).unwrap();
        // Import précédent : A, B, et un bourg 77 qui n'existe plus ; un lieu manuel.
        let places = [existing(10, "Mon A", Some(1)), existing(11, "B", Some(2)), existing(12, "Disparu", Some(77)), existing(13, "Manuel", None)];
        let roads = [
            // A–B a changé de distance.
            ExistingRoad { channel_id: 50, place_a: 10, place_b: 11, distance: 35, secret: false, server_id: 100 },
            // Vers le bourg disparu.
            ExistingRoad { channel_id: 51, place_a: 11, place_b: 12, distance: 5, secret: false, server_id: 100 },
            // Route manuelle vers un lieu manuel : gardée.
            ExistingRoad { channel_id: 52, place_a: 10, place_b: 13, distance: 5, secret: false, server_id: 100 },
        ];
        let mut o = opts();
        o.mode = ImportMode::Reimport;
        let p = build_plan(&m, &o, &places, &roads).unwrap();
        let a = p.places.iter().find(|x| x.burg_id == 1).unwrap();
        assert_eq!(a.action, PlaceAction::Keep { category_id: 10 });
        assert_eq!(a.name, "Mon A", "a place keeps its name");
        assert_eq!(p.places.iter().find(|x| x.burg_id == 3).unwrap().action, PlaceAction::Create);
        assert_eq!(road(&p, 1, 2).unwrap().action, RoadAction::Update { channel_id: 50 });
        assert_eq!(p.remove_places, vec![12]);
        assert_eq!(p.remove_roads, vec![51]);
    }

    #[test]
    fn a_place_on_a_server_not_selected_is_reported() {
        let m = parse(MAP).unwrap();
        let places = [ExistingPlace { category_id: 10, name: "A".into(), server_id: 999, burg_id: Some(1) }];
        let mut o = opts();
        o.mode = ImportMode::Reimport;
        assert_eq!(build_plan(&m, &o, &places, &[]), Err(PlanError::ServerNotSelected(999)));
    }

    #[test]
    fn too_many_places_ask_for_more_servers() {
        let m = parse(MAP).unwrap();
        let mut o = opts();
        o.max_places = 2;
        let p = build_plan(&m, &o, &[], &[]).unwrap();
        assert_eq!(p.servers.len(), 3);
        assert_eq!(p.servers_to_create(), 2);
        assert_eq!(p.servers[0].guild_id, Some(100));
        assert!(p.cut_roads >= 1);
    }

    /// Plan de la vraie carte Moria entière (fichier fourni par `AZGAAR_MAP_FILE`).
    #[test]
    #[ignore = "integration test: needs AZGAAR_MAP_FILE (a real Azgaar .map save)"]
    fn a_real_map_is_planned_over_several_servers() {
        let path = std::env::var("AZGAAR_MAP_FILE").expect("set AZGAAR_MAP_FILE");
        let f = crate::activity::map::azgaar_file::parse_map_file(&std::fs::read(&path).unwrap()).unwrap();
        let o = ImportOptions::default();
        let start = std::time::Instant::now();
        let p = build_plan(&f.map, &o, &[], &[]).unwrap();
        let elapsed = start.elapsed();
        assert_eq!(p.places.len(), f.map.burgs.len());
        assert_eq!(p.servers.len(), f.map.burgs.len().div_ceil(60), "the minimum number of servers");
        assert!(p.servers.iter().all(|s| s.budget.fits(60)), "{:?}", p.servers);
        assert_eq!(p, build_plan(&f.map, &o, &[], &[]).unwrap(), "deterministic");
        eprintln!(
            "Moria : {} lieux, {} routes, {} serveurs, {} routes entre serveurs ({:.1} %), {} bourgs isolés, {} objets Discord, calcul {:?}",
            p.places.len(), p.roads.len(), p.servers.len(), p.cut_roads, 100.0 * p.cut_roads as f64 / p.roads.len() as f64,
            p.isolated.len(), p.discord_objects_to_create(), elapsed
        );
        for (i, s) in p.servers.iter().enumerate() {
            eprintln!("  serveur {i} : {:?}", s.budget);
        }
    }
}
