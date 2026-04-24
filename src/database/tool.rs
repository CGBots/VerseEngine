use mongodb::bson::doc;
use mongodb::bson::oid::ObjectId;
use mongodb::results::InsertOneResult;
use serde::{Deserialize, Serialize};
use crate::database::db_client::get_db_client;
use crate::database::db_namespace::{AREAS_COLLECTION_NAME, PLACED_ITEMS_COLLECTION_NAME, VERSEENGINE_DB_NAME};
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
    pub area_id: Option<ObjectId>,
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
            .collection::<mongodb::bson::Document>(PLACED_ITEMS_COLLECTION_NAME);

        let pipeline = vec![
            doc! {
                "$match": {
                    "universe_id": universe_id,
                    "channel_id": channel_id as i64,
                }
            },
            doc! {
                "$lookup": {
                    "from": AREAS_COLLECTION_NAME,
                    "let": { "tool_channel": { "$toString": "$channel_id" }, "tool_universe": "$universe_id" },
                    "pipeline": [
                        {
                            "$match": {
                                "$expr": {
                                    "$and": [
                                        { "$eq": ["$channel_id", "$$tool_channel"] },
                                        { "$eq": ["$universe_id", "$$tool_universe"] }
                                    ]
                                }
                            }
                        }
                    ],
                    "as": "area"
                }
            }
        ];

        let mut cursor = collection.aggregate(pipeline).await?;
        let mut results = Vec::new();
        while let Some(doc) = cursor.try_next().await? {
            let tool: Tool = mongodb::bson::from_document(doc)?;
            results.push(tool);
        }
        Ok(results)
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
