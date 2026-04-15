use mongodb::bson::doc;
use mongodb::bson::oid::ObjectId;
use mongodb::results::InsertOneResult;
use serde::{Deserialize, Serialize};
use crate::database::db_client::get_db_client;
use crate::database::db_namespace::{PLACED_ITEMS_COLLECTION_NAME, VERSEENGINE_DB_NAME};
use futures::TryStreamExt;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Tool{
    #[serde(rename = "_id", skip_serializing_if = "Option::is_none")]
    pub _id: Option<ObjectId>,
    pub universe_id: ObjectId,
    pub server_id: u64,
    pub owner_id: Option<ObjectId>,
    pub category_id: u64,
    pub channel_id: u64,
    pub original_item: ObjectId,
    pub name: String,
    pub chained: Option<ObjectId>,
    pub inventory_id: Option<ObjectId>,
    pub inventory_size: u64,
}

impl Tool {
    pub async fn save(self) -> mongodb::error::Result<InsertOneResult> {
        let db_client = get_db_client().await;
        db_client
            .database(VERSEENGINE_DB_NAME)
            .collection::<Tool>(PLACED_ITEMS_COLLECTION_NAME)
            .insert_one(self)
            .await
    }

    pub async fn get_by_channel_id(universe_id: ObjectId, channel_id: u64) -> mongodb::error::Result<Vec<Tool>> {
        let db_client = get_db_client().await;
        let collection = db_client
            .database(VERSEENGINE_DB_NAME)
            .collection::<Tool>(PLACED_ITEMS_COLLECTION_NAME);

        let filter = doc! {
            "universe_id": universe_id,
            "channel_id": channel_id as i64,
        };

        let cursor = collection.find(filter).await?;
        cursor.try_collect().await
    }

    pub async fn get_by_id(tool_id: ObjectId) -> mongodb::error::Result<Option<Tool>> {
        let db_client = get_db_client().await;
        let collection = db_client
            .database(VERSEENGINE_DB_NAME)
            .collection::<Tool>(PLACED_ITEMS_COLLECTION_NAME);

        let filter = doc! {
            "_id": tool_id,
        };

        collection.find_one(filter).await
    }
}
