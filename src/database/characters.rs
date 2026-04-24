use mongodb::bson::doc;
use serde_with::DisplayFromStr;
use mongodb::bson::oid::ObjectId;
use serde::{Deserialize, Serialize};
use serde_with::serde_as;
use crate::database::db_client::{get_db_client};
use crate::database::db_namespace::{CHARACTERS_COLLECTION_NAME, TRAVELS_COLLECTION_NAME, VERSEENGINE_DB_NAME};
use crate::database::stats::Stat;
use crate::database::travel::PlayerMove;

#[serde_as]
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Character {
    #[serde(rename = "_id")]
    pub _id: ObjectId,
    #[serde_as(as = "DisplayFromStr")]
    pub user_id: u64,
    pub universe_id: ObjectId,
    pub name: String,
    pub stats: Vec<Stat>,
}

impl Character {
    pub async fn update(self) -> mongodb::error::Result<mongodb::results::UpdateResult> {
        let db_client = get_db_client().await;
        let filter = doc! {"_id": self._id};
        db_client
            .database(VERSEENGINE_DB_NAME)
            .collection::<Character>(CHARACTERS_COLLECTION_NAME)
            .replace_one(filter, self)
            .await
    }

    pub async fn get_player_move(self) -> mongodb::error::Result<Option<PlayerMove>> {
        let filter = doc!{"player_id": self._id, "universe_id": self.universe_id};
        let db_client = get_db_client().await;
        db_client
            .database(VERSEENGINE_DB_NAME)
            .collection::<PlayerMove>(TRAVELS_COLLECTION_NAME)
            .find_one(filter)
            .await
    }

}

pub async fn get_character_by_user_id(universe_id: ObjectId, user_id: u64) -> mongodb::error::Result<Option<Character>> {
         let db_client = get_db_client().await;
         let filter = doc!{"user_id": user_id.to_string(), "universe_id": universe_id};
         db_client
             .database(VERSEENGINE_DB_NAME)
             .collection::<Character>(CHARACTERS_COLLECTION_NAME)
             .find_one(filter)
             .await
     }

pub async fn get_character_by_id(character_id: ObjectId) -> mongodb::error::Result<Option<Character>> {
    let db_client = get_db_client().await;
    let filter = doc!{"_id": character_id};
    db_client
        .database(VERSEENGINE_DB_NAME)
        .collection::<Character>(CHARACTERS_COLLECTION_NAME)
        .find_one(filter)
        .await
}