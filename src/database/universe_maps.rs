//! Carte d'un univers (collection `universe_maps`, un document par univers) : positions des
//! lieux, terrain éventuel et zone explorable. Voir `docs/activity/ARCHITECTURE.md` § 8.2.
//!
//! - `Computed` : placement calculé à partir des routes, mis en cache. `graph_hash` permet de le
//!   recalculer quand un lieu ou une route change.
//! - `Azgaar` / `Manual` : positions importées ou saisies, jamais recalculées automatiquement.

use mongodb::bson::doc;
use mongodb::bson::oid::ObjectId;
use mongodb::options::ReplaceOptions;
use serde::{Deserialize, Serialize};
use serde_with::{serde_as, DisplayFromStr};

use crate::database::db_client::get_db_client;
use crate::database::db_namespace::{UNIVERSE_MAPS_COLLECTION_NAME, VERSEENGINE_DB_NAME};

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum MapSource {
    Azgaar,
    Computed,
    Manual,
}

#[serde_as]
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct MapPosition {
    /// `category_id` du lieu.
    #[serde_as(as = "DisplayFromStr")]
    pub place_id: u64,
    pub x: f64,
    pub y: f64,
    #[serde(default)]
    pub z: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub azgaar_burg_id: Option<u64>,
}

/// Heightmap sous-échantillonnée (`cols × rows` octets, 0–255).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct MapTerrain {
    pub cols: u32,
    pub rows: u32,
    #[serde(with = "serde_bytes_compat")]
    pub heights: Vec<u8>,
}

/// Que faire du terrain hors de la zone explorable (D7).
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum OutsideZone {
    Crop,
    Grey,
    Remove,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct MapZone {
    pub polygon: Vec<[f64; 2]>,
    pub outside: OutsideZone,
}

/// Tracé d'une route entre deux lieux (`category_id`, `a < b`), points de `a` vers `b`.
#[serde_as]
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct RoadPath {
    #[serde_as(as = "DisplayFromStr")]
    pub a: u64,
    #[serde_as(as = "DisplayFromStr")]
    pub b: u64,
    pub points: Vec<[f64; 2]>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct UniverseMap {
    #[serde(rename = "_id", default = "ObjectId::new")]
    pub id: ObjectId,
    pub universe_id: ObjectId,
    pub source: MapSource,
    pub width: f64,
    pub height: f64,
    pub positions: Vec<MapPosition>,
    #[serde(default)]
    pub terrain: Option<MapTerrain>,
    #[serde(default)]
    pub zone: Option<MapZone>,
    /// Empreinte des lieux et routes ayant servi au placement (`Computed` seulement).
    #[serde(default)]
    pub graph_hash: Option<String>,
    /// Tracé réel des routes (import Azgaar) ; sans tracé, une route est une ligne droite.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub road_paths: Vec<RoadPath>,
    /// Image exportée par Azgaar, affichée telle quelle (`map_backgrounds`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub background: Option<crate::database::map_backgrounds::MapBackground>,
    /// Rendu façon Azgaar (biomes, mers, rivières, États), lu dans la sauvegarde à l'import.
    /// Absent : carte générée ou calculée, ou importée avant ce rendu (le front se rabat sur le relief).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub style: Option<MapStyle>,
    pub updated_at: u64,
}

/// Surface de la carte : une valeur par case d'une grille `cols × rows` couvrant la carte.
/// - `0..=199` : terre, identifiant du biome Azgaar ;
/// - `SURFACE_OCEAN + d` (`d` de 1 à 9) : mer, `d` = éloignement de la côte en cellules ;
/// - `SURFACE_LAKE` : lac ;
/// - `SURFACE_HIDDEN` : inconnu (sous le brouillard, rien n'est envoyé).
pub const SURFACE_OCEAN: u8 = 200;
pub const SURFACE_LAKE: u8 = 230;
pub const SURFACE_HIDDEN: u8 = 255;

/// Rendu façon Azgaar d'une carte importée (calculé une fois, à l'import).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct MapStyle {
    pub cols: u32,
    pub rows: u32,
    #[serde(with = "serde_bytes_compat")]
    pub surface: Vec<u8>,
    /// État de chaque case (0 : sans État), vide si la carte n'en a pas.
    #[serde(with = "serde_bytes_compat", default)]
    pub states: Vec<u8>,
    /// Couleur de chaque biome (`#rrggbb`), par identifiant.
    pub biome_colors: Vec<String>,
    /// Couleur de chaque État (`#rrggbb`, vide pour 0), par identifiant.
    #[serde(default)]
    pub state_colors: Vec<String>,
    #[serde(default)]
    pub rivers: Vec<StyleRiver>,
    /// Bourgs : taille et type, pour leur icône.
    #[serde(default)]
    pub burgs: Vec<BurgStyle>,
    /// Vrai si la surface vient des cellules d'Azgaar (biomes et États exacts) ; faux si elle a
    /// été estimée depuis la grille (biomes recalculés, sans États ni rivières).
    #[serde(default)]
    pub exact: bool,
    /// Cellules du pack d'Azgaar, pour les contours vectoriels (`map/isolines.rs`). Absentes des
    /// univers importés avant, ou si le pack n'a pas pu être reconstruit exactement.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pack: Option<PackCells>,
}

/// Cellules du pack d'Azgaar : positions (suivies des points de bordure de la grille, qui ferment
/// les cellules du bord) et, pour chaque cellule réelle, biome, État et nature de l'eau.
/// Stockage compact : octets bruts (positions en `f32` petit-boutiste, x puis y).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct PackCells {
    /// Nombre de cellules réelles ; les points suivants sont la bordure.
    pub real: u32,
    #[serde(with = "serde_bytes_compat")]
    pub points: Vec<u8>,
    #[serde(with = "serde_bytes_compat")]
    pub biome: Vec<u8>,
    /// État (0 : aucun), borné à 255 comme `MapStyle.states`.
    #[serde(with = "serde_bytes_compat")]
    pub state: Vec<u8>,
    /// 0 terre, 1 mer, 2 lac.
    #[serde(with = "serde_bytes_compat")]
    pub water: Vec<u8>,
}

/// Nature d'une cellule du pack (`PackCells.water`).
pub const PACK_LAND: u8 = 0;
pub const PACK_OCEAN: u8 = 1;
pub const PACK_LAKE: u8 = 2;
/// Cellule hors de la zone importée (lasso) : jamais dessinée.
pub const PACK_HIDDEN: u8 = 3;

impl PackCells {
    pub fn encode_points(points: &[(f64, f64)]) -> Vec<u8> {
        let mut out = Vec::with_capacity(points.len() * 8);
        for &(x, y) in points {
            out.extend_from_slice(&(x as f32).to_le_bytes());
            out.extend_from_slice(&(y as f32).to_le_bytes());
        }
        out
    }

    /// Positions décodées (cellules réelles puis bordure).
    pub fn points(&self) -> Vec<(f64, f64)> {
        self.points
            .chunks_exact(8)
            .map(|c| (f32::from_le_bytes([c[0], c[1], c[2], c[3]]) as f64, f32::from_le_bytes([c[4], c[5], c[6], c[7]]) as f64))
            .collect()
    }

    /// Données cohérentes : une valeur par cellule réelle, et des positions pour chacune.
    pub fn is_consistent(&self) -> bool {
        let n = self.real as usize;
        self.biome.len() == n && self.state.len() == n && self.water.len() == n && self.points.len() / 8 >= n && self.points.len() % 8 == 0
    }
}

/// Rivière : tracé (centres des cellules traversées) et largeur à la source et à l'embouchure.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct StyleRiver {
    pub points: Vec<[f64; 2]>,
    pub width: [f64; 2],
}

/// Taille d'un bourg : 0 village, 1 bourg, 2 ville ; capitale et port à part.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct BurgStyle {
    pub burg_id: u64,
    pub size: u8,
    pub capital: bool,
    pub port: bool,
    /// Informations du bourg pour la fiche du lieu (mode admin). Jamais envoyées avec la carte :
    /// absentes des univers importés avant la fiche.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub details: Option<BurgDetails>,
}

/// Ce qu'Azgaar sait d'un bourg, lu à l'import (ligne 15 du `.map`, États ligne 14, cultures ligne 13).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct BurgDetails {
    /// Nom du bourg dans Azgaar (le lieu a pu être renommé depuis).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Habitants (population Azgaar × échelle de population des réglages).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub population: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub culture: Option<String>,
    /// Type Azgaar (`River`, `Naval`, `Highland`…).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    /// Groupe Azgaar (`capital`, `city`, `fort`, `village`…).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
    /// Éléments présents : `citadel`, `walls`, `plaza`, `temple`, `shanty`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub features: Vec<String>,
}

/// Stockage des octets en binaire BSON plutôt qu'en tableau d'entiers (64 Kio max).
mod serde_bytes_compat {
    use mongodb::bson::Binary;
    use mongodb::bson::spec::BinarySubtype;
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    pub fn serialize<S: Serializer>(bytes: &[u8], s: S) -> Result<S::Ok, S::Error> {
        Binary { subtype: BinarySubtype::Generic, bytes: bytes.to_vec() }.serialize(s)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<u8>, D::Error> {
        Ok(Binary::deserialize(d)?.bytes)
    }
}

async fn collection() -> mongodb::Collection<UniverseMap> {
    get_db_client().await.database(VERSEENGINE_DB_NAME).collection::<UniverseMap>(UNIVERSE_MAPS_COLLECTION_NAME)
}

pub async fn get_universe_map(universe_id: ObjectId) -> mongodb::error::Result<Option<UniverseMap>> {
    collection().await.find_one(doc! {"universe_id": universe_id}).await
}

/// Remplace (ou crée) la carte de l'univers en une écriture atomique.
pub async fn save_universe_map(map: &UniverseMap) -> mongodb::error::Result<()> {
    let mut stored = map.clone();
    if let Some(existing) = get_universe_map(map.universe_id).await? {
        stored.id = existing.id;
    }
    collection()
        .await
        .replace_one(doc! {"universe_id": map.universe_id}, &stored)
        .with_options(ReplaceOptions::builder().upsert(true).build())
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::block_on;

    fn sample(universe_id: ObjectId) -> UniverseMap {
        UniverseMap {
            id: ObjectId::new(),
            universe_id,
            source: MapSource::Computed,
            width: 1000.0,
            height: 1000.0,
            positions: vec![MapPosition { place_id: 12_345_678_901_234_567, x: 1.0, y: 2.0, z: 0.0, azgaar_burg_id: None }],
            terrain: Some(MapTerrain { cols: 2, rows: 1, heights: vec![0, 255] }),
            zone: Some(MapZone { polygon: vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0]], outside: OutsideZone::Grey }),
            graph_hash: Some("abc".into()),
            road_paths: vec![RoadPath { a: 12_345_678_901_234_567, b: 2, points: vec![[1.0, 2.0], [3.0, 4.0]] }],
            background: None,
            style: Some(MapStyle {
                cols: 2, rows: 1, surface: vec![4, SURFACE_OCEAN + 1], states: vec![1, 0], biome_colors: vec!["#466eab".into()],
                state_colors: vec![String::new(), "#ff0000".into()], rivers: vec![StyleRiver { points: vec![[0.0, 0.0], [1.0, 1.0]], width: [0.3, 1.0] }],
                burgs: vec![BurgStyle { burg_id: 3, size: 2, capital: true, port: false, details: None }], exact: true,
                pack: None,
            }),
            updated_at: 1,
        }
    }

    #[test]
    fn a_map_round_trips_through_bson() {
        let map = sample(ObjectId::new());
        let doc = mongodb::bson::to_document(&map).unwrap();
        assert_eq!(doc.get_array("positions").unwrap()[0].as_document().unwrap().get_str("place_id").unwrap(), "12345678901234567");
        assert!(matches!(doc.get_document("terrain").unwrap().get("heights"), Some(mongodb::bson::Bson::Binary(_))));
        assert!(matches!(doc.get_document("style").unwrap().get("surface"), Some(mongodb::bson::Bson::Binary(_))));
        let back: UniverseMap = mongodb::bson::from_document(doc).unwrap();
        assert_eq!(back, map);
    }

    #[test]
    #[ignore = "integration test: needs a running MongoDB (MONGODB_* env vars); run with --ignored"]
    fn saving_twice_keeps_a_single_map_per_universe() {
        block_on(async {
            let universe_id = ObjectId::new();
            let mut map = sample(universe_id);
            save_universe_map(&map).await.unwrap();
            map.source = MapSource::Manual;
            map.id = ObjectId::new();
            save_universe_map(&map).await.unwrap();

            let count = collection().await.count_documents(doc! {"universe_id": universe_id}).await.unwrap();
            let stored = get_universe_map(universe_id).await.unwrap().unwrap();
            collection().await.delete_many(doc! {"universe_id": universe_id}).await.unwrap();

            assert_eq!(count, 1);
            assert_eq!(stored.source, MapSource::Manual);
        });
    }
}
