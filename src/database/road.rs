use mongodb::bson::doc;
use serde_with::DisplayFromStr;
use mongodb::bson::oid::ObjectId;
use mongodb::Cursor;
use mongodb::results::InsertOneResult;
use serde::{Deserialize, Serialize};
use serde_with::serde_as;
use crate::database::db_client::{get_db_client};
use crate::database::db_namespace::{ROADS_COLLECTION_NAME, VERSEENGINE_DB_NAME};
use crate::database::modifiers::Modifier;

#[serde_as]
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Road{
    #[serde(rename = "_id")]
    pub _id: ObjectId,
    pub universe_id: ObjectId,
    #[serde_as(as = "DisplayFromStr")]
    pub server_id: u64,
    pub server_two_id: Option<String>,
    pub road_name: String,
    #[serde_as(as = "DisplayFromStr")]
    pub role_id: u64,
    #[serde_as(as = "DisplayFromStr")]
    pub channel_id: u64,
    #[serde_as(as = "DisplayFromStr")]
    pub place_one_id: u64,
    #[serde_as(as = "DisplayFromStr")]
    pub place_two_id: u64,
    #[serde_as(as = "DisplayFromStr")]
    pub distance: u64,
    pub secret: bool,
    pub modifiers: Vec<Modifier>
}

impl Road{
    pub async fn update(self) -> mongodb::error::Result<mongodb::results::UpdateResult> {
        let db_client = get_db_client().await;
        let filter = doc! {"_id": self._id};
        db_client
            .database(VERSEENGINE_DB_NAME)
            .collection::<Road>(ROADS_COLLECTION_NAME)
            .replace_one(filter, self)
            .await
    }

    pub async fn insert(&self) -> mongodb::error::Result<InsertOneResult> {
        let db_client = get_db_client().await;
        db_client
            .database(VERSEENGINE_DB_NAME)
            .collection::<Road>(ROADS_COLLECTION_NAME)
            .insert_one(self)
            .await
    }
}

pub async fn get_road_by_channel_id(universe_id: ObjectId, channel_id: u64) -> mongodb::error::Result<Option<Road>> {
    let filter = doc!{"channel_id": channel_id.to_string().as_str(), "universe_id": universe_id};
    let db_client = get_db_client().await;
    db_client
        .database(VERSEENGINE_DB_NAME)
        .collection::<Road>(ROADS_COLLECTION_NAME)
        .find_one(filter)
        .await
}

pub async fn get_road_by_source(universe_id: ObjectId, destination_id: u64) -> mongodb::error::Result<Cursor<Road>> {
    let db_client = get_db_client().await;
    let filter = doc! {
        "$or": [
            { "place_one_id": destination_id.to_string(), "universe_id": universe_id },
            { "place_two_id": destination_id.to_string(), "universe_id": universe_id },
        ],
        "secret": false
    };
    db_client.database(VERSEENGINE_DB_NAME)
        .collection::<Road>(ROADS_COLLECTION_NAME)
        .find(filter)
        .await
}

pub async fn get_road(universe_id: ObjectId, place_one: u64, place_two: u64) -> mongodb::error::Result<Option<Road>> {
    let db_client = get_db_client().await;
    let filter = doc! {
        "$or": [
            {
                "place_one_id": place_one.to_string(),
                "place_two_id": place_two.to_string(),
                "universe_id": universe_id,
            },
            {
                "place_one_id": place_two.to_string(),
                "place_two_id": place_one.to_string(),
                "universe_id": universe_id,
            }
        ]
    };
    db_client.database(VERSEENGINE_DB_NAME)
        .collection::<Road>(ROADS_COLLECTION_NAME)
        .find_one(filter)
        .await
}

pub async fn count_non_secret_roads_for_place(universe_id: ObjectId, place_id: u64) -> mongodb::error::Result<u64> {
    let db_client = get_db_client().await;
    let filter = doc! {
        "$or": [
            { "place_one_id": place_id.to_string(), "universe_id": universe_id },
            { "place_two_id": place_id.to_string(), "universe_id": universe_id },
        ],
        "secret": false
    };
    db_client.database(VERSEENGINE_DB_NAME)
        .collection::<Road>(ROADS_COLLECTION_NAME)
        .count_documents(filter)
        .await
}