//! Routes déjà empruntées par chaque joueur (décision D10) : un joueur voit une route secrète
//! (menu `/travel_start`, carte de l'activité) seulement s'il l'a déjà empruntée ou s'il est dessus.
//!
//! Un document par `(universe_id, user_id)` (index unique), alimenté par `$addToSet` à chaque
//! départ sur une route, dans le service de voyage partagé par les commandes slash et l'activité.

use std::collections::HashSet;

use mongodb::bson::doc;
use mongodb::bson::oid::ObjectId;
use mongodb::options::UpdateOptions;
use serde::{Deserialize, Serialize};
use serde_with::{serde_as, DisplayFromStr};

use crate::database::db_client::get_db_client;
use crate::database::db_namespace::{KNOWN_ROADS_COLLECTION_NAME, VERSEENGINE_DB_NAME};

#[serde_as]
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct KnownRoads {
    pub universe_id: ObjectId,
    #[serde_as(as = "DisplayFromStr")]
    pub user_id: u64,
    /// `channel_id` des routes empruntées.
    #[serde_as(as = "Vec<DisplayFromStr>")]
    #[serde(default)]
    pub road_ids: Vec<u64>,
}

fn collection() -> impl std::future::Future<Output = mongodb::Collection<KnownRoads>> {
    async { get_db_client().await.database(VERSEENGINE_DB_NAME).collection::<KnownRoads>(KNOWN_ROADS_COLLECTION_NAME) }
}

/// Mémorise que chacun de `user_ids` a emprunté la route `road_id`. Idempotent.
pub async fn remember_road(universe_id: ObjectId, user_ids: &[u64], road_id: u64) -> mongodb::error::Result<()> {
    let coll = collection().await;
    for user_id in user_ids {
        coll.update_one(
            doc! {"universe_id": universe_id, "user_id": user_id.to_string()},
            doc! {"$addToSet": {"road_ids": road_id.to_string()}},
        )
        .with_options(UpdateOptions::builder().upsert(true).build())
        .await?;
    }
    Ok(())
}

/// Routes (`channel_id`) déjà empruntées par `user_id` dans cet univers.
pub async fn known_road_ids(universe_id: ObjectId, user_id: u64) -> mongodb::error::Result<HashSet<u64>> {
    let found = collection().await.find_one(doc! {"universe_id": universe_id, "user_id": user_id.to_string()}).await?;
    Ok(found.map(|k| k.road_ids.into_iter().collect()).unwrap_or_default())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{block_on, random_id};

    #[test]
    fn documents_without_roads_are_read() {
        let doc = doc! {"universe_id": ObjectId::new(), "user_id": "12"};
        let known: KnownRoads = mongodb::bson::from_document(doc).unwrap();
        assert_eq!(known.user_id, 12);
        assert!(known.road_ids.is_empty());
    }

    #[test]
    #[ignore = "integration test: needs a running MongoDB (MONGODB_* env vars); run with --ignored"]
    fn roads_are_remembered_once_per_player() {
        block_on(async {
            let universe_id = ObjectId::new();
            let (a, b) = (random_id(), random_id());
            remember_road(universe_id, &[a, b], 10).await.unwrap();
            remember_road(universe_id, &[a], 10).await.unwrap();
            remember_road(universe_id, &[a], 20).await.unwrap();

            let known_a = known_road_ids(universe_id, a).await.unwrap();
            let known_b = known_road_ids(universe_id, b).await.unwrap();
            let unknown = known_road_ids(universe_id, random_id()).await.unwrap();
            let stored = collection().await.find_one(doc! {"universe_id": universe_id, "user_id": a.to_string()}).await.unwrap().unwrap();
            collection().await.delete_many(doc! {"universe_id": universe_id}).await.unwrap();

            assert_eq!(known_a, HashSet::from([10, 20]));
            assert_eq!(known_b, HashSet::from([10]));
            assert!(unknown.is_empty());
            assert_eq!(stored.road_ids.len(), 2, "no duplicate road id");
        });
    }
}
