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
        self.update_with_optional_session(None).await
    }

    pub async fn update_with_session(&self, session: &mut mongodb::ClientSession) -> mongodb::error::Result<mongodb::results::UpdateResult> {
        self.update_with_optional_session(Some(session)).await
    }

    pub async fn update_with_optional_session(&self, session: Option<&mut mongodb::ClientSession>) -> mongodb::error::Result<mongodb::results::UpdateResult> {
        let db_client = get_db_client().await;
        let filter = doc! {"_id": self._id};
        let coll = db_client
            .database(VERSEENGINE_DB_NAME)
            .collection::<Area>(AREAS_COLLECTION_NAME);
        let options = mongodb::options::ReplaceOptions::builder().upsert(true).build();
        match session {
            Some(s) => coll.replace_one(filter, self).with_options(options).session(s).await,
            None => coll.replace_one(filter, self).with_options(options).await,
        }
    }

    pub async fn insert(&self) -> mongodb::error::Result<InsertOneResult> {
        self.insert_with_optional_session(None).await
    }

    pub async fn insert_with_session(&self, session: &mut mongodb::ClientSession) -> mongodb::error::Result<InsertOneResult> {
        self.insert_with_optional_session(Some(session)).await
    }

    pub async fn insert_with_optional_session(&self, session: Option<&mut mongodb::ClientSession>) -> mongodb::error::Result<InsertOneResult> {
        let db_client = get_db_client().await;
        let coll = db_client
            .database(VERSEENGINE_DB_NAME)
            .collection::<Area>(AREAS_COLLECTION_NAME);
        match session {
            Some(s) => coll.insert_one(self).session(s).await,
            None => coll.insert_one(self).await,
        }
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
