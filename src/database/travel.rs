use mongodb::bson::{doc, to_document};
use mongodb::bson::oid::ObjectId;
use mongodb::results::{DeleteResult, InsertOneResult, UpdateResult};
use serde::{Deserialize, Serialize};
use serde_with::DisplayFromStr;
use serde_with::serde_as;
use crate::database::db_client::{get_db_client};
use crate::database::db_namespace::{TRAVELS_COLLECTION_NAME, VERSEENGINE_DB_NAME};

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub enum SpaceType{
    Road,
    #[default]
    Place
}

#[serde_as]
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct TravelGroup {
    #[serde(rename = "_id")]
    pub _id: ObjectId,
    pub universe_id: ObjectId,

    pub actual_space_id: u64,
    pub actual_space_type: SpaceType,

    pub is_in_move: bool,
    pub is_end: bool,

    pub step_start_timestamp: Option<u64>,
    pub step_end_timestamp: Option<u64>,

    /// Ancien rôle de la route : plus jamais renseigné (Q1), conservé pour relire les anciens
    /// documents. L'accès au salon se fait par permission individuelle.
    #[serde(default)]
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
    #[serde_as(as = "Vec<DisplayFromStr>")]
    pub members: Vec<u64>,
    #[serde_as(as = "DisplayFromStr")]
    pub server_id: u64,
}

/// Groupe de voyage d'un joueur dans un univers.
pub async fn get_group_of_user(universe_id: ObjectId, user_id: u64) -> mongodb::error::Result<Option<TravelGroup>> {
    get_db_client().await
        .database(VERSEENGINE_DB_NAME)
        .collection::<TravelGroup>(TRAVELS_COLLECTION_NAME)
        .find_one(doc! {"members": user_id.to_string(), "universe_id": universe_id})
        .await
}

impl TravelGroup {
    pub async fn insert(self) -> mongodb::error::Result<InsertOneResult> {
        let db_client = get_db_client().await;
        let db = db_client.database(VERSEENGINE_DB_NAME);
        db.collection(TRAVELS_COLLECTION_NAME)
            .insert_one(self)
            .await
    }

    pub async fn insert_with_session(&self, session: &mut mongodb::ClientSession) -> mongodb::error::Result<InsertOneResult> {
        let db_client = get_db_client().await;
        let db = db_client.database(VERSEENGINE_DB_NAME);
        db.collection::<TravelGroup>(TRAVELS_COLLECTION_NAME)
            .insert_one(self)
            .session(session)
            .await
    }

    pub async fn remove(&self) -> Result<DeleteResult, mongodb::error::Error> {
        let db_client = get_db_client().await;
        let db = db_client.database(VERSEENGINE_DB_NAME);
        let filter = doc! {"_id": self._id};
        db.collection::<TravelGroup>(TRAVELS_COLLECTION_NAME)
            .delete_one(filter)
            .await
    }

    pub async fn remove_with_session(&self, session: &mut mongodb::ClientSession) -> Result<DeleteResult, mongodb::error::Error> {
        let db_client = get_db_client().await;
        let db = db_client.database(VERSEENGINE_DB_NAME);
        let filter = doc! {"_id": self._id};
        db.collection::<TravelGroup>(TRAVELS_COLLECTION_NAME)
            .delete_one(filter)
            .session(session)
            .await
    }

    pub async fn upsert(&self) -> mongodb::error::Result<UpdateResult> {
        let mut doc = to_document(self)?;
        doc.remove("_id");
        let filter = doc! {"_id": self._id};
        let update = doc! {"$set": doc};
        let options = mongodb::options::UpdateOptions::builder().upsert(true).build();

        let db_client = get_db_client().await;
        let db = db_client.database(VERSEENGINE_DB_NAME);
        db.collection::<TravelGroup>(TRAVELS_COLLECTION_NAME)
            .update_one(filter, update)
            .with_options(options)
            .await
    }

    pub async fn upsert_with_session(&self, session: &mut mongodb::ClientSession) -> mongodb::error::Result<UpdateResult> {
        let mut doc = to_document(self)?;
        doc.remove("_id");
        let filter = doc! {"_id": self._id};
        let update = doc! {"$set": doc};
        let options = mongodb::options::UpdateOptions::builder().upsert(true).build();

        let db_client = get_db_client().await;
        let db = db_client.database(VERSEENGINE_DB_NAME);
        db.collection::<TravelGroup>(TRAVELS_COLLECTION_NAME)
            .update_one(filter, update)
            .with_options(options)
            .session(session)
            .await
    }

    pub async fn get_active_moves(universe_id: ObjectId) -> mongodb::error::Result<Vec<TravelGroup>> {
        let db_client = get_db_client().await;
        let filter = doc! { "is_in_move": true, "universe_id":  universe_id };
        let mut cursor = db_client.database(VERSEENGINE_DB_NAME)
            .collection::<TravelGroup>(TRAVELS_COLLECTION_NAME)
            .find(filter)
            .await?;

        let mut moves = Vec::new();
        use futures::TryStreamExt;
        while let Some(m) = cursor.try_next().await? {
            moves.push(m);
        }
        Ok(moves)
    }
}