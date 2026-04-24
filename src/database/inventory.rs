use futures::TryStreamExt;
use mongodb::bson::doc;
use mongodb::bson::oid::ObjectId;
use mongodb::ClientSession;
use serde::{Deserialize, Serialize};
use crate::database::db_client::get_db_client;
use crate::database::db_namespace::{INVENTORY_COLLECTION_NAME, VERSEENGINE_DB_NAME};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum HolderType{
    Character,
    Item
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Holder{
    pub holder_type: HolderType,
    pub holder_id: ObjectId
}

impl PartialEq<ObjectId> for Holder {
    fn eq(&self, other: &ObjectId) -> bool {
        self.holder_id == *other
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Inventory {
    #[serde(rename = "_id", skip_serializing_if = "Option::is_none")]
    pub _id: Option<ObjectId>,
    pub universe_id: ObjectId,
    pub holder: Holder,
    pub item_id: ObjectId,
    pub quantity: u64,
}

impl Inventory {
    pub async fn _add_item(universe_id: ObjectId, character_id: ObjectId, item_id: ObjectId, amount: u64) -> mongodb::error::Result<()> {
        let db_client = get_db_client().await;
        let collection = db_client
            .database(VERSEENGINE_DB_NAME)
            .collection::<Inventory>(INVENTORY_COLLECTION_NAME);

        let filter = doc! {
            "universe_id": universe_id,
            "holder.holder_id": character_id,
            "holder.holder_type": "Character",
            "item_id": item_id,
        };

        let update = doc! {
            "$inc": { "quantity": amount as i64 },
            "$setOnInsert": {
                "universe_id": universe_id,
                "holder.holder_id": character_id,
                "holder.holder_type": "Character",
                "item_id": item_id,
            }
        };

        let options = mongodb::options::UpdateOptions::builder().upsert(true).build();

        let _ = collection.update_one(filter, update).with_options(options).await?;

        Ok(())
    }

    pub async fn add_item_to_inventory_with_session(session: &mut ClientSession, universe_id: ObjectId, holder_id: ObjectId, holder_type: HolderType, item_id: ObjectId, amount: u64) -> mongodb::error::Result<ObjectId> {
        let db_client = get_db_client().await;
        let collection = db_client
            .database(VERSEENGINE_DB_NAME)
            .collection::<Inventory>(INVENTORY_COLLECTION_NAME);

        let filter = doc! {
            "universe_id": universe_id,
            "holder.holder_id": holder_id,
            "holder.holder_type": match holder_type {
                HolderType::Character => "Character",
                HolderType::Item => "Item",
            },
            "item_id": item_id,
        };

        let update = doc! {
            "$inc": { "quantity": amount as i64 },
            "$setOnInsert": {
                "universe_id": universe_id,
                "holder.holder_id": holder_id,
                "holder.holder_type": match holder_type {
                    HolderType::Character => "Character",
                    HolderType::Item => "Item",
                },
                "item_id": item_id,
            }
        };

        let options = mongodb::options::FindOneAndUpdateOptions::builder().upsert(true).build();

        let result = collection.find_one_and_update(filter, update).with_options(options).session(&mut *session).await?;

        if let Some(inv) = result {
            Ok(inv._id.unwrap())
        } else {
            let filter = doc! {
                "universe_id": universe_id,
                "holder.holder_id": holder_id,
                "holder.holder_type": match holder_type {
                    HolderType::Character => "Character",
                    HolderType::Item => "Item",
                },
                "item_id": item_id,
            };
            let inv = collection.find_one(filter).session(session).await?.unwrap();
            Ok(inv._id.unwrap())
        }
    }

    pub async fn remove_item_from_holder_with_session(session: &mut ClientSession, universe_id: ObjectId, holder_id: ObjectId, holder_type: HolderType, item_id: ObjectId, amount: u64) -> mongodb::error::Result<bool> {
        let db_client = get_db_client().await;
        let collection = db_client
            .database(VERSEENGINE_DB_NAME)
            .collection::<Inventory>(INVENTORY_COLLECTION_NAME);

        let filter = doc! {
            "universe_id": universe_id,
            "holder.holder_id": holder_id,
            "holder.holder_type": match holder_type {
                HolderType::Character => "Character",
                HolderType::Item => "Item",
            },
            "item_id": item_id,
            "quantity": { "$gte": amount as i64 }
        };

        let update = doc! {
            "$inc": { "quantity": -(amount as i64) }
        };

        let result = collection.update_one(filter, update).session(session).await?;
        Ok(result.modified_count > 0)
    }

    pub async fn add_item_to_inventory(universe_id: ObjectId, holder_id: ObjectId, holder_type: HolderType, item_id: ObjectId, amount: u64) -> mongodb::error::Result<ObjectId> {
        let db_client = get_db_client().await;
        let collection = db_client
            .database(VERSEENGINE_DB_NAME)
            .collection::<Inventory>(INVENTORY_COLLECTION_NAME);

        let filter = doc! {
            "universe_id": universe_id,
            "holder.holder_id": holder_id,
            "holder.holder_type": match holder_type {
                HolderType::Character => "Character",
                HolderType::Item => "Item",
            },
            "item_id": item_id,
        };

        let update = doc! {
            "$inc": { "quantity": amount as i64 },
            "$setOnInsert": {
                "universe_id": universe_id,
                "holder.holder_id": holder_id,
                "holder.holder_type": match holder_type {
                    HolderType::Character => "Character",
                    HolderType::Item => "Item",
                },
                "item_id": item_id,
            }
        };

        let options = mongodb::options::FindOneAndUpdateOptions::builder().upsert(true).build();

        let result = collection.find_one_and_update(filter, update).with_options(options).await?;
        
        if let Some(inv) = result {
            Ok(inv._id.unwrap())
        } else {
            // If it was an upsert and didn't return the document, we might need to find it
            let filter = doc! {
                "universe_id": universe_id,
                "holder.holder_id": holder_id,
                "holder.holder_type": match holder_type {
                    HolderType::Character => "Character",
                    HolderType::Item => "Item",
                },
                "item_id": item_id,
            };
            let inv = collection.find_one(filter).await?.unwrap();
            Ok(inv._id.unwrap())
        }
    }

    pub async fn remove_item(inventory_id: ObjectId, amount: u64) -> mongodb::error::Result<bool> {
        let db_client = get_db_client().await;
        let collection = db_client
            .database(VERSEENGINE_DB_NAME)
            .collection::<Inventory>(INVENTORY_COLLECTION_NAME);

        let filter = doc! {
            "_id": inventory_id,
            "quantity": { "$gte": amount as i64 }
        };

        let update = doc! {
            "$inc": { "quantity": -(amount as i64) }
        };

        let result = collection.update_one(filter, update).await?;
        Ok(result.modified_count > 0)
    }

    pub async fn remove_item_from_holder(universe_id: ObjectId, holder_id: ObjectId, holder_type: HolderType, item_id: ObjectId, amount: u64) -> mongodb::error::Result<bool> {
        let db_client = get_db_client().await;
        let collection = db_client
            .database(VERSEENGINE_DB_NAME)
            .collection::<Inventory>(INVENTORY_COLLECTION_NAME);

        let filter = doc! {
            "universe_id": universe_id,
            "holder.holder_id": holder_id,
            "holder.holder_type": match holder_type {
                HolderType::Character => "Character",
                HolderType::Item => "Item",
            },
            "item_id": item_id,
            "quantity": { "$gte": amount as i64 }
        };

        let update = doc! {
            "$inc": { "quantity": -(amount as i64) }
        };

        let result = collection.update_one(filter, update).await?;
        Ok(result.modified_count > 0)
    }

    pub async fn create_empty_inventory(universe_id: ObjectId, holder_type: HolderType, holder_id: ObjectId) -> mongodb::error::Result<ObjectId> {
        let db_client = get_db_client().await;
        let collection = db_client
            .database(VERSEENGINE_DB_NAME)
            .collection::<Inventory>(INVENTORY_COLLECTION_NAME);

        // Dans ce système, un inventaire semble être une ligne par item.
        // Si on veut un inventaire "vide", peut-être qu'on n'a rien à insérer ?
        // Mais Tool attend un inventory_id (ObjectId).
        // Si on regarde la structure Inventory, elle contient item_id et quantity.
        // Un inventaire "vide" n'a pas vraiment de sens avec cette structure si on veut un ID unique qui représente le contenant.
        // Cependant, le Tool demande un inventory_id.
        // Je vais créer une entrée "factice" avec une quantité 0 pour obtenir un ID, ou repenser l'ID.
        // En regardant Item, il a aussi un inventory_id: Option<ObjectId>.
        
        // Si je crée une entrée avec quantity 0 et un item_id nul (ou arbitraire), c'est moche.
        // Peut-être que le Tool devrait avoir son propre ID et l'inventaire pointe vers lui ?
        // "Lorsqu'un item est créé un inventaire qui lui est propre est créé (nouvel ObjectId)."
        // Cette phrase suggère que l'objet (Tool) possède son propre inventaire.
        
        // Utilisons un ObjectId aléatoire pour l'instant si on ne peut pas créer d'entrée vide,
        // ou insérons une entrée avec une quantité 0.
        
        let inv = Inventory {
            _id: Some(ObjectId::new()),
            universe_id,
            holder: Holder {
                holder_type,
                holder_id,
            },
            item_id: ObjectId::from_bytes([0; 12]), // Dummy item_id
            quantity: 0,
        };
        
        let res = collection.insert_one(inv.clone()).await?;
        Ok(res.inserted_id.as_object_id().unwrap())
    }

    pub async fn get_by_character_id(universe_id: ObjectId, character_id: ObjectId) -> mongodb::error::Result<Vec<Inventory>> {
        let db_client = get_db_client().await;
        let collection = db_client
            .database(VERSEENGINE_DB_NAME)
            .collection::<Inventory>(INVENTORY_COLLECTION_NAME);

        let filter = doc! {
            "universe_id": universe_id,
            "holder.holder_id": character_id,
            "holder.holder_type": "Character",
            "quantity": { "$gt": 0 }
        };

        let cursor = collection.find(filter).await?;
        cursor.try_collect().await
    }

    pub async fn get_by_id(id: ObjectId) -> mongodb::error::Result<Option<Inventory>> {
        let db_client = get_db_client().await;
        let collection = db_client
            .database(VERSEENGINE_DB_NAME)
            .collection::<Inventory>(INVENTORY_COLLECTION_NAME);

        let filter = doc! {
            "_id": id,
            "quantity": { "$gt": 0 }
        };

        collection.find_one(filter).await
    }

    pub async fn get_by_holder(universe_id: ObjectId, holder_id: ObjectId, holder_type: HolderType) -> mongodb::error::Result<Vec<Inventory>> {
        let db_client = get_db_client().await;
        let collection = db_client
            .database(VERSEENGINE_DB_NAME)
            .collection::<Inventory>(INVENTORY_COLLECTION_NAME);

        let filter = doc! {
            "universe_id": universe_id,
            "holder.holder_id": holder_id,
            "holder.holder_type": match holder_type {
                HolderType::Character => "Character",
                HolderType::Item => "Item",
            },
            "quantity": { "$gt": 0 }
        };

        let cursor = collection.find(filter).await?;
        cursor.try_collect().await
    }

    pub async fn remove_all_by_item_id(universe_id: ObjectId, item_id: ObjectId) -> mongodb::error::Result<u64> {
        let db_client = get_db_client().await;
        let filter = doc! {
            "universe_id": universe_id,
            "item_id": item_id
        };

        let result = db_client
            .database(VERSEENGINE_DB_NAME)
            .collection::<Inventory>(INVENTORY_COLLECTION_NAME)
            .delete_many(filter)
            .await?;

        Ok(result.deleted_count)
    }

    pub async fn get_all_holders_by_item_id(universe_id: ObjectId, item_id: ObjectId) -> mongodb::error::Result<Vec<Inventory>> {
        let db_client = get_db_client().await;
        let filter = doc! {
            "universe_id": universe_id,
            "item_id": item_id,
            "quantity": { "$gt": 0 }
        };

        let cursor = db_client
            .database(VERSEENGINE_DB_NAME)
            .collection::<Inventory>(INVENTORY_COLLECTION_NAME)
            .find(filter)
            .await?;

        cursor.try_collect().await
    }
}