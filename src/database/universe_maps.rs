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
    pub updated_at: u64,
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
            updated_at: 1,
        }
    }

    #[test]
    fn a_map_round_trips_through_bson() {
        let map = sample(ObjectId::new());
        let doc = mongodb::bson::to_document(&map).unwrap();
        assert_eq!(doc.get_array("positions").unwrap()[0].as_document().unwrap().get_str("place_id").unwrap(), "12345678901234567");
        assert!(matches!(doc.get_document("terrain").unwrap().get("heights"), Some(mongodb::bson::Bson::Binary(_))));
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
