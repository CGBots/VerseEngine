use mongodb::bson::doc;
use mongodb::bson::oid::ObjectId;
use mongodb::results::InsertOneResult;
use serde::{Deserialize, Serialize};
use serde_with::serde_as;
use crate::database::db_client::get_db_client;
use crate::database::db_namespace::{ITEM_COLLECTION_NAME, VERSEENGINE_DB_NAME};
use crate::database::modifiers::Modifier;
use crate::item::ItemUsage;

#[serde_as]
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Item {
    pub _id: ObjectId,
    pub universe_id: ObjectId,
    pub item_name: String,
    pub item_usage: ItemUsage,
    pub effects: Vec<Modifier>,
    pub description: Option<String>,
    pub secret_informations: Option<String>, //Only displayed when player got the item and look the item sheet
    pub image: Option<String>,
    pub wiki_post_id: Option<ObjectId>,
}

impl Item {
    pub async fn save(self) -> mongodb::error::Result<InsertOneResult> {
        let db_client = get_db_client().await;
        db_client
            .database(VERSEENGINE_DB_NAME)
            .collection::<Item>(ITEM_COLLECTION_NAME)
            .insert_one(self)
            .await
    }
}

pub async fn get_item_by_name(universe_id: ObjectId, item_name: &str) -> mongodb::error::Result<Option<Item>> {
    let db_client = get_db_client().await;
    let filter = doc! {"universe_id": universe_id, "item_name": item_name};
    db_client
        .database(VERSEENGINE_DB_NAME)
        .collection::<Item>(ITEM_COLLECTION_NAME)
        .find_one(filter)
        .await
}

pub async fn get_item_by_id(item_id: ObjectId) -> mongodb::error::Result<Option<Item>> {
    let db_client = get_db_client().await;
    let filter = doc! {"_id": item_id};
    db_client
        .database(VERSEENGINE_DB_NAME)
        .collection::<Item>(ITEM_COLLECTION_NAME)
        .find_one(filter)
        .await
}