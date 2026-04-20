use mongodb::bson::{doc, oid::ObjectId};
use mongodb::results::InsertOneResult;
use serde::{Deserialize, Serialize};
use crate::database::db_client::get_db_client;
use crate::database::db_namespace::{VERSEENGINE_DB_NAME};

pub static RECIPE_COLLECTION_NAME: &str = "recipes";

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Recipe {
    #[serde(rename = "_id", skip_serializing_if = "Option::is_none")]
    pub _id : Option<ObjectId>,
    pub universe_id: ObjectId,
    pub tool_id: Option<ObjectId>,
    pub ingredients: Vec<(u64, ObjectId)>,
    pub result: Vec<(u64, ObjectId)>,
    pub tools_needed: Vec<ObjectId>,
    pub delay: u64,
    pub raw_text: String,
    pub recipe_name: String,
    pub wiki_posts_id: Option<Vec<(u64, u64)>>,
}

impl Recipe {
    pub async fn save(self) -> mongodb::error::Result<InsertOneResult> {
        let db_client = get_db_client().await;
        db_client
            .database(VERSEENGINE_DB_NAME)
            .collection::<Recipe>(RECIPE_COLLECTION_NAME)
            .insert_one(self)
            .await
    }

    pub async fn upsert(self) -> mongodb::error::Result<mongodb::results::UpdateResult> {
        let db_client = get_db_client().await;
        let filter = doc! {
            "universe_id": self.universe_id,
            "recipe_name": &self.recipe_name,
        };
        let options = mongodb::options::ReplaceOptions::builder().upsert(true).build();
        db_client
            .database(VERSEENGINE_DB_NAME)
            .collection::<Recipe>(RECIPE_COLLECTION_NAME)
            .replace_one(filter, self)
            .with_options(options)
            .await
    }

    pub async fn get_by_name(universe_id: ObjectId, name: &str) -> mongodb::error::Result<Option<Recipe>> {
        let db_client = get_db_client().await;
        let filter = doc! {
            "universe_id": universe_id,
            "recipe_name": name,
        };
        db_client
            .database(VERSEENGINE_DB_NAME)
            .collection::<Recipe>(RECIPE_COLLECTION_NAME)
            .find_one(filter)
            .await
    }

    pub async fn get_by_universe(universe_id: ObjectId) -> mongodb::error::Result<Vec<Recipe>> {
        let db_client = get_db_client().await;
        let filter = doc! {"universe_id": universe_id};
        let cursor = db_client
            .database(VERSEENGINE_DB_NAME)
            .collection::<Recipe>(RECIPE_COLLECTION_NAME)
            .find(filter)
            .await?;
        
        use futures::TryStreamExt;
        cursor.try_collect().await
    }
}
