use std::collections::HashMap;
use chrono::{DateTime, Utc};
use mongodb::bson::doc;
use mongodb::bson::oid::ObjectId;
use mongodb::results::UpdateResult;
use serde::{Deserialize, Serialize};
use serde_with::{serde_as, DisplayFromStr};
use crate::database::db_client::get_db_client;
use crate::database::db_namespace::{LOOT_TABLES_COLLECTION_NAME, VERSEENGINE_DB_NAME};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct LootTableItem {
    pub name: String,
    pub probability: f64,
    pub min: u32,
    pub max: u32,
    pub stock: Option<u32>,
    pub secret: bool,
}

impl LootTableItem {
    pub fn is_out_of_stock(&self) -> bool {
        self.stock.map_or(false, |s| s == 0)
    }

    pub fn decrement_stock(&mut self) -> bool {
        if let Some(stock) = self.stock.as_mut() {
            if *stock > 0 {
                *stock -= 1;
                return true;
            }
        }
        false
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct LootTableSet {
    pub name: String,
    pub probability: f64,
    pub min: u32,
    pub max: u32,
    pub stock: Option<u32>,
    pub items: Vec<LootTableItem>,
    pub secret: bool,
}

impl LootTableSet {
    pub fn is_out_of_stock(&self) -> bool {
        self.stock.map_or(false, |s| s == 0)
    }

    pub fn decrement_stock(&mut self) -> bool {
        if let Some(stock) = self.stock.as_mut() {
            if *stock > 0 {
                *stock -= 1;
                return true;
            }
        }
        false
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(untagged)]
pub enum LootTableEntry {
    Item(LootTableItem),
    Set(LootTableSet),
}

impl LootTableEntry {
    pub fn is_out_of_stock(&self) -> bool {
        match self {
            LootTableEntry::Item(i) => i.is_out_of_stock(),
            LootTableEntry::Set(s) => s.is_out_of_stock(),
        }
    }
}

#[serde_as]
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct LootTable {
    #[serde(rename = "_id", skip_serializing_if = "Option::is_none")]
    pub _id: Option<ObjectId>,
    pub universe_id: ObjectId,
    #[serde_as(as = "DisplayFromStr")]
    pub channel_id: u64,
    pub entries: Vec<LootTableEntry>,
    pub raw_text: String,
    pub rate_limit: Option<u64>,
    pub last_loot: Option<HashMap<String, DateTime<Utc>>>,
}

impl LootTable {
    pub async fn save_or_update(&self) -> mongodb::error::Result<UpdateResult> {
        let db_client = get_db_client().await;
        let filter = doc! {
            "universe_id": self.universe_id,
            "channel_id": self.channel_id.to_string()
        };
        
        let mut doc = mongodb::bson::to_document(self).unwrap();
        doc.remove("_id");

        let options = mongodb::options::UpdateOptions::builder().upsert(true).build();
        
        db_client
            .database(VERSEENGINE_DB_NAME)
            .collection::<LootTable>(LOOT_TABLES_COLLECTION_NAME)
            .update_one(filter, doc! { "$set": doc })
            .with_options(options)
            .await
    }

    pub async fn delete(&self) -> mongodb::error::Result<mongodb::results::DeleteResult> {
        let db_client = get_db_client().await;
        let filter = doc! {
            "universe_id": self.universe_id,
            "channel_id": self.channel_id.to_string()
        };

        db_client
            .database(VERSEENGINE_DB_NAME)
            .collection::<LootTable>(LOOT_TABLES_COLLECTION_NAME)
            .delete_one(filter)
            .await
    }
}

pub async fn get_loot_table_by_channel_id(universe_id: ObjectId, channel_id: u64) -> mongodb::error::Result<Option<LootTable>> {
    let db_client = get_db_client().await;
    let filter = doc! {
        "universe_id": universe_id,
        "channel_id": channel_id.to_string()
    };
    db_client
        .database(VERSEENGINE_DB_NAME)
        .collection::<LootTable>(LOOT_TABLES_COLLECTION_NAME)
        .find_one(filter)
        .await
}
