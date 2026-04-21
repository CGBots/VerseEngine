use log::log;
use mongodb::bson::{doc, to_document};
use mongodb::bson::oid::ObjectId;
use mongodb::results::{DeleteResult, InsertOneResult, UpdateResult};
use serde::{Deserialize, Serialize};
use serde_with::DisplayFromStr;
use serde_with::serde_as;
use crate::database::db_client::{get_db_client};
use crate::database::db_namespace::{TRAVELS_COLLECTION_NAME, VERSEENGINE_DB_NAME};
use crate::database::modifiers::Modifier;
use crate::database::stats::{get_stat_by_name, StatValue, SPEED_STAT};

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub enum SpaceType{
    Road,
    #[default]
    Place
}

#[serde_as]
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct PlayerMove{
    #[serde(rename = "_id")]
    pub _id: ObjectId,
    pub universe_id: ObjectId,

    pub actual_space_id: u64,
    pub actual_space_type: SpaceType,

    pub is_in_move: bool,
    pub is_end: bool,

    pub step_start_timestamp: Option<u64>,
    pub step_end_timestamp: Option<u64>,

    pub road_role_id: Option<u64>,
    pub road_id: Option<u64>,
    pub road_server_id: Option<u64>,

    pub destination_id: Option<u64>,
    pub destination_role_id: Option<u64>,
    pub destination_server_id: Option<u64>,

    pub source_id: Option<u64>,
    pub source_role_id: Option<u64>,
    pub source_server_id: Option<u64>,

    pub modified_speed: f64,
    pub distance_traveled: f64,
    #[serde_as(as = "DisplayFromStr")]
    pub user_id: u64,
    #[serde_as(as = "DisplayFromStr")]
    pub server_id: u64,
}

impl PlayerMove {
    pub async fn insert(self) -> mongodb::error::Result<InsertOneResult> {
        let db_client = get_db_client().await;
        let db = db_client.database(VERSEENGINE_DB_NAME);
        db.collection(TRAVELS_COLLECTION_NAME)
            .insert_one(self)
            .await
    }

    pub async fn remove(&self) -> Result<DeleteResult, mongodb::error::Error> {
        let db_client = get_db_client().await;
        let db = db_client.database(VERSEENGINE_DB_NAME);
        let filter = doc! {"user_id": self.user_id.to_string(), "universe_id":  self.universe_id};
        db.collection::<PlayerMove>(TRAVELS_COLLECTION_NAME)
            .delete_one(filter)
            .await
    }

    pub async fn upsert(&self) -> mongodb::error::Result<UpdateResult> {
        let mut doc = to_document(self).unwrap();
        doc.remove("_id");
        let filter = doc! {"user_id": self.user_id.to_string(), "universe_id":  self.universe_id};
        let update = doc! {"$set": doc};
        let options = mongodb::options::UpdateOptions::builder().upsert(true).build();

        let db_client = get_db_client().await;
        let db = db_client.database(VERSEENGINE_DB_NAME);
        db.collection::<PlayerMove>(TRAVELS_COLLECTION_NAME)
            .update_one(filter, update)
            .with_options(options)
            .await
    }

    pub async fn get_active_moves(universe_id: ObjectId) -> mongodb::error::Result<Vec<PlayerMove>> {
        let db_client = get_db_client().await;
        let filter = doc! { "is_in_move": true, "universe_id":  universe_id };
        let mut cursor = db_client.database(VERSEENGINE_DB_NAME)
            .collection::<PlayerMove>(TRAVELS_COLLECTION_NAME)
            .find(filter)
            .await?;

        let mut moves = Vec::new();
        use futures::TryStreamExt;
        while let Some(m) = cursor.try_next().await? {
            moves.push(m);
        }
        Ok(moves)
    }

    pub async fn next_step(self) -> Option<(StatValue, Option<Modifier>)>{
        if !self.is_end{
            let Ok(Some(stat)) = get_stat_by_name(self.universe_id, SPEED_STAT).await else {
                log!(log::Level::Error, "Failed to get stat. [{:?}]", self);
                return None;
            };
            let Ok((_speed, _shortest_modifier)) = stat.resolve(self.actual_space_id, self.user_id).await else { todo!() };
        };
        None
    }
}