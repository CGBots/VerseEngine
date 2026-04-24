use mongodb::bson::doc;
use mongodb::bson::oid::ObjectId;
use mongodb::results::InsertOneResult;
use serde::{Deserialize, Serialize};
use serde_with::{serde_as, DisplayFromStr};
use crate::database::db_client::get_db_client;
use crate::database::db_namespace::{AREAS_COLLECTION_NAME, VERSEENGINE_DB_NAME};
use crate::database::modifiers::Modifier;

#[serde_as]
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Area {
    #[serde(rename = "_id")]
    pub _id: ObjectId,
    pub universe_id: ObjectId,
    #[serde_as(as = "DisplayFromStr")]
    pub channel_id: u64,
    pub modifiers: Vec<Modifier>,
}

impl Area {
    pub async fn new(universe_id: ObjectId, channel_id: u64) -> Self {
        Self {
            _id: ObjectId::new(),
            universe_id,
            channel_id,
            modifiers: vec![],
        }
    }

    pub async fn update(&self) -> mongodb::error::Result<mongodb::results::UpdateResult> {
        let db_client = get_db_client().await;
        let filter = doc! {"_id": self._id};
        db_client
            .database(VERSEENGINE_DB_NAME)
            .collection::<Area>(AREAS_COLLECTION_NAME)
            .replace_one(filter, self)
            .await
    }

    pub async fn insert(&self) -> mongodb::error::Result<InsertOneResult> {
        let db_client = get_db_client().await;
        db_client
            .database(VERSEENGINE_DB_NAME)
            .collection::<Area>(AREAS_COLLECTION_NAME)
            .insert_one(self)
            .await
    }
}

pub async fn get_area_by_channel_id(universe_id: ObjectId, channel_id: u64) -> mongodb::error::Result<Option<Area>> {
    let filter = doc! {
        "channel_id": channel_id.to_string(),
        "universe_id": universe_id,
    };
    let db_client = get_db_client().await;
    db_client
        .database(VERSEENGINE_DB_NAME)
        .collection::<Area>(AREAS_COLLECTION_NAME)
        .find_one(filter)
        .await
}
