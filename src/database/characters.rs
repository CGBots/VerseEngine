use mongodb::bson::doc;
use serde_with::DisplayFromStr;
use mongodb::bson::oid::ObjectId;
use serde::{Deserialize, Serialize};
use serde_with::serde_as;
use crate::database::db_client::{get_db_client};
use crate::database::db_namespace::{CHARACTERS_COLLECTION_NAME, TRAVELS_COLLECTION_NAME, VERSEENGINE_DB_NAME};
use crate::database::stats::Stat;
use crate::database::travel::TravelGroup;

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
    /// Description publique de la fiche, copiée depuis l'embed à l'acceptation.
    /// Absente des documents antérieurs (lecture compatible grâce à `default`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Image du personnage : référence vers la pièce jointe d'un message Discord
    /// (fiche ou fil de la fiche). Les URL du CDN Discord expirent : on stocke la
    /// référence et on régénère l'URL à la demande ; supprimer le message retire l'image.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image: Option<CharacterImage>,
}

#[serde_as]
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct CharacterImage {
    #[serde_as(as = "DisplayFromStr")]
    pub channel_id: u64,
    #[serde_as(as = "DisplayFromStr")]
    pub message_id: u64,
    #[serde_as(as = "DisplayFromStr")]
    pub attachment_id: u64,
}

impl Character {
    pub async fn update(self) -> mongodb::error::Result<mongodb::results::UpdateResult> {
        self.update_with_optional_session(None).await
    }

    pub async fn update_with_session(self, session: &mut mongodb::ClientSession) -> mongodb::error::Result<mongodb::results::UpdateResult> {
        self.update_with_optional_session(Some(session)).await
    }

    pub async fn update_with_optional_session(self, session: Option<&mut mongodb::ClientSession>) -> mongodb::error::Result<mongodb::results::UpdateResult> {
        let db_client = get_db_client().await;
        let filter = doc! {"_id": self._id};
        let coll = db_client
            .database(VERSEENGINE_DB_NAME)
            .collection::<Character>(CHARACTERS_COLLECTION_NAME);
        match session {
            Some(s) => coll.replace_one(filter, self).session(s).await,
            None => coll.replace_one(filter, self).await,
        }
    }

    pub async fn upsert(self) -> mongodb::error::Result<mongodb::results::UpdateResult> {
        self.upsert_with_optional_session(None).await
    }

    pub async fn upsert_with_session(self, session: &mut mongodb::ClientSession) -> mongodb::error::Result<mongodb::results::UpdateResult> {
        self.upsert_with_optional_session(Some(session)).await
    }

    pub async fn upsert_with_optional_session(mut self, session: Option<&mut mongodb::ClientSession>) -> mongodb::error::Result<mongodb::results::UpdateResult> {
        let db_client = get_db_client().await;
        let filter = doc! {
            "user_id": self.user_id.to_string(),
            "universe_id": self.universe_id,
        };
        let options = mongodb::options::ReplaceOptions::builder().upsert(true).build();
        let coll = db_client
            .database(VERSEENGINE_DB_NAME)
            .collection::<Character>(CHARACTERS_COLLECTION_NAME);
        // Le personnage existe déjà (fiche acceptée une deuxième fois) : MongoDB refuse un
        // remplacement qui change `_id` (« immutable field '_id' ») ; on garde le sien.
        match session {
            Some(s) => {
                if let Some(existing) = coll.find_one(filter.clone()).session(&mut *s).await? {
                    self._id = existing._id;
                }
                coll.replace_one(filter, self).with_options(options).session(s).await
            }
            None => {
                if let Some(existing) = coll.find_one(filter.clone()).await? {
                    self._id = existing._id;
                }
                coll.replace_one(filter, self).with_options(options).await
            }
        }
    }

    pub async fn get_player_move(self) -> mongodb::error::Result<Option<TravelGroup>> {
        let filter = doc!{"members": self.user_id.to_string(), "universe_id": self.universe_id};
        let db_client = get_db_client().await;
        db_client
            .database(VERSEENGINE_DB_NAME)
            .collection::<TravelGroup>(TRAVELS_COLLECTION_NAME)
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
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "integration test: needs a running MongoDB (MONGODB_* env vars); run with --ignored"]
    fn accepting_a_sheet_twice_updates_the_same_character() {
        crate::test_support::block_on(async {
            let universe_id = ObjectId::new();
            let character = |name: &str| Character {
                _id: ObjectId::new(),
                user_id: 42,
                universe_id,
                name: name.into(),
                stats: vec![],
                description: None,
                image: None,
            };
            character("Premier").upsert_with_optional_session(None).await.unwrap();
            // Nouvel `_id` en mémoire, comme à chaque acceptation : ne doit pas faire échouer l'écriture.
            character("Second").upsert_with_optional_session(None).await.unwrap();
            let stored = get_character_by_user_id(universe_id, 42).await.unwrap().unwrap();
            let coll = get_db_client().await.database(VERSEENGINE_DB_NAME).collection::<Character>(CHARACTERS_COLLECTION_NAME);
            let count = coll.count_documents(doc! {"universe_id": universe_id}).await.unwrap();
            coll.delete_many(doc! {"universe_id": universe_id}).await.unwrap();
            assert_eq!((stored.name.as_str(), count), ("Second", 1));
        });
    }
}
