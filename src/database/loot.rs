use mongodb::bson::{doc, to_document, oid::ObjectId};
use mongodb::results::{DeleteResult, InsertOneResult, UpdateResult};
use serde::{Deserialize, Serialize};
use serde_with::{serde_as, DisplayFromStr};
use crate::database::db_client::get_db_client;
use crate::database::db_namespace::{LOOTS_COLLECTION_NAME, VERSEENGINE_DB_NAME};

#[serde_as]
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct PlayerLoot {
    #[serde(rename = "_id", skip_serializing_if = "Option::is_none")]
    pub _id: Option<ObjectId>,
    pub universe_id: ObjectId,
    #[serde_as(as = "DisplayFromStr")]
    pub user_id: u64,
    #[serde_as(as = "DisplayFromStr")]
    pub server_id: u64,
    pub items: Vec<String>, // Noms des items lootés
    pub start_timestamp: u64,
    pub end_timestamp: u64,
    pub is_finished: bool,
}

impl PlayerLoot {
    pub async fn insert(self) -> mongodb::error::Result<InsertOneResult> {
        let db_client = get_db_client().await;
        let db = db_client.database(VERSEENGINE_DB_NAME);
        db.collection(LOOTS_COLLECTION_NAME)
            .insert_one(self)
            .await
    }

    pub async fn remove(&self) -> Result<DeleteResult, mongodb::error::Error> {
        let db_client = get_db_client().await;
        let db = db_client.database(VERSEENGINE_DB_NAME);
        let filter = doc! {"user_id": self.user_id.to_string(), "universe_id": self.universe_id};
        db.collection::<PlayerLoot>(LOOTS_COLLECTION_NAME)
            .delete_one(filter)
            .await
    }

    pub async fn upsert(&self) -> mongodb::error::Result<UpdateResult> {
        let mut doc = to_document(self).unwrap();
        doc.remove("_id");
        let filter = doc! {"user_id": self.user_id.to_string(), "universe_id": self.universe_id};
        let update = doc! {"$set": doc};
        let options = mongodb::options::UpdateOptions::builder().upsert(true).build();

        let db_client = get_db_client().await;
        let db = db_client.database(VERSEENGINE_DB_NAME);
        db.collection::<PlayerLoot>(LOOTS_COLLECTION_NAME)
            .update_one(filter, update)
            .with_options(options)
            .await
    }

    pub async fn get_active_loots(universe_id: ObjectId) -> mongodb::error::Result<Vec<PlayerLoot>> {
        let db_client = get_db_client().await;
        let filter = doc! { "is_finished": false, "universe_id": universe_id };
        let mut cursor = db_client.database(VERSEENGINE_DB_NAME)
            .collection::<PlayerLoot>(LOOTS_COLLECTION_NAME)
            .find(filter)
            .await?;

        let mut loots = Vec::new();
        use futures::TryStreamExt;
        while let Some(l) = cursor.try_next().await? {
            loots.push(l);
        }
        Ok(loots)
    }

    pub async fn get_by_user_id(universe_id: ObjectId, user_id: u64) -> mongodb::error::Result<Option<PlayerLoot>> {
        let db_client = get_db_client().await;
        let filter = doc! {"user_id": user_id.to_string(), "universe_id": universe_id, "is_finished": false};
        db_client.database(VERSEENGINE_DB_NAME)
            .collection::<PlayerLoot>(LOOTS_COLLECTION_NAME)
            .find_one(filter)
            .await
    }
}
