use futures::TryStreamExt;
use mongodb::bson::doc;
use mongodb::bson::oid::ObjectId;
use serde::{Deserialize, Serialize};
use crate::database::db_client::get_db_client;
use crate::database::db_namespace::{INVENTORY_COLLECTION_NAME, VERSEENGINE_DB_NAME};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Inventory {
    #[serde(rename = "_id", skip_serializing_if = "Option::is_none")]
    pub _id: Option<ObjectId>,
    pub universe_id: ObjectId,
    pub character_id: ObjectId,
    pub item_id: ObjectId,
    pub quantity: u64,
}

impl Inventory {
    pub async fn add_item(universe_id: ObjectId, character_id: ObjectId, item_id: ObjectId, amount: u64) -> mongodb::error::Result<()> {
        let db_client = get_db_client().await;
        let collection = db_client
            .database(VERSEENGINE_DB_NAME)
            .collection::<Inventory>(INVENTORY_COLLECTION_NAME);

        let filter = doc! {
            "universe_id": universe_id,
            "character_id": character_id,
            "item_id": item_id,
        };

        let update = doc! {
            "$inc": { "quantity": amount as i64 }
        };

        let options = mongodb::options::UpdateOptions::builder().upsert(true).build();

        collection.update_one(filter, update).with_options(options).await?;

        Ok(())
    }

    pub async fn get_by_character_id(universe_id: ObjectId, character_id: ObjectId) -> mongodb::error::Result<Vec<Inventory>> {
        let db_client = get_db_client().await;
        let collection = db_client
            .database(VERSEENGINE_DB_NAME)
            .collection::<Inventory>(INVENTORY_COLLECTION_NAME);

        let filter = doc! {
            "universe_id": universe_id,
            "character_id": character_id,
            "quantity": { "$gt": 0 }
        };

        let cursor = collection.find(filter).await?;
        cursor.try_collect().await
    }

    pub async fn get_by_id(id: ObjectId) -> mongodb::error::Result<Option<Inventory>> {
        let db_client = get_db_client().await;
        let collection = db_client
            .database(VERSEENGINE_DB_NAME)
            .collection::<Inventory>(INVENTORY_COLLECTION_NAME);

        let filter = doc! {
            "_id": id,
            "quantity": { "$gt": 0 }
        };

        collection.find_one(filter).await
    }
}