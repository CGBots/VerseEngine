use futures::TryStreamExt;
use serde_with::DisplayFromStr;
use mongodb::bson::doc;
use mongodb::bson::oid::ObjectId;
use mongodb::results::InsertOneResult;
use serde::{Deserialize, Serialize};
use serde_with::serde_as;
use crate::database::db_client::{get_db_client};
use crate::database::db_namespace::{PLACES_COLLECTION_NAME, ROADS_COLLECTION_NAME, VERSEENGINE_DB_NAME};
use crate::database::modifiers::Modifier;
use crate::database::road::Road;

#[serde_as]
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Place{
    #[serde(rename = "_id")]
    pub _id: ObjectId,
    pub universe_id: ObjectId,
    #[serde_as(as = "DisplayFromStr")]
    pub server_id: u64,
    #[serde_as(as = "DisplayFromStr")]
    pub category_id: u64,
    #[serde_as(as = "DisplayFromStr")]
    pub role: u64,
    pub name: String,
    pub modifiers: Vec<Modifier>,
    /// Salon textuel générique du lieu (`place-centrale` / `central-square`, I5 et Q5). Absent des
    /// lieux créés avant l'import de carte.
    #[serde_as(as = "Option<DisplayFromStr>")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub central_channel_id: Option<u64>,
    /// Fiche du lieu (mode admin de l'activité) : description et posts du forum wiki. Absente
    /// des lieux antérieurs (`default`).
    #[serde(default)]
    pub wiki: PlaceWiki,
}

/// Fiche d'un lieu : texte saisi par l'admin, dernière version publiée et posts du forum wiki.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
pub struct PlaceWiki {
    /// Texte en cours de rédaction (enregistré, pas forcément publié).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Dernier texte publié : c'est lui que voient les joueurs (le brouillon reste privé).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub published: Option<PublishedText>,
    /// Un post par serveur : on le met à jour au lieu d'en recréer un.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub posts: Vec<WikiPost>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct PublishedText {
    pub text: String,
    pub at: u64,
}

/// Post du lieu dans le forum wiki d'un serveur. Un post de forum est un fil : son premier
/// message a le même identifiant que le fil.
#[serde_as]
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
pub struct WikiPost {
    #[serde_as(as = "DisplayFromStr")]
    pub server_id: u64,
    #[serde_as(as = "DisplayFromStr")]
    pub thread_id: u64,
}

async fn places() -> mongodb::Collection<Place> {
    get_db_client().await.database(VERSEENGINE_DB_NAME).collection::<Place>(PLACES_COLLECTION_NAME)
}

/// Enregistre le brouillon de la fiche. Écriture ciblée (`$set`) : ne touche à rien d'autre du lieu.
/// Retourne faux si le lieu n'existe pas (ou plus) dans cet univers.
pub async fn set_place_description(universe_id: ObjectId, category_id: u64, description: Option<&str>) -> mongodb::error::Result<bool> {
    let update = match description {
        Some(d) => doc! {"$set": {"wiki.description": d}},
        None => doc! {"$unset": {"wiki.description": ""}},
    };
    let r = places().await.update_one(doc! {"universe_id": universe_id, "category_id": category_id.to_string()}, update).await?;
    Ok(r.matched_count > 0)
}

/// Mémorise une publication : texte publié et posts du forum (ceux qui existent après coup).
pub async fn set_place_publication(universe_id: ObjectId, category_id: u64, published: &PublishedText, posts: &[WikiPost]) -> mongodb::error::Result<bool> {
    // Même forme que la sérialisation serde (identifiants en chaînes).
    let published = doc! {"text": &published.text, "at": published.at as i64};
    let posts: Vec<mongodb::bson::Document> = posts.iter().map(|p| doc! {"server_id": p.server_id.to_string(), "thread_id": p.thread_id.to_string()}).collect();
    let r = places()
        .await
        .update_one(doc! {"universe_id": universe_id, "category_id": category_id.to_string()}, doc! {"$set": {"wiki.published": published, "wiki.posts": posts}})
        .await?;
    Ok(r.matched_count > 0)
}

impl Place{
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
            .collection::<Place>(PLACES_COLLECTION_NAME);
        match session {
            Some(s) => coll.replace_one(filter, self).session(s).await,
            None => coll.replace_one(filter, self).await,
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
            .collection::<Place>(PLACES_COLLECTION_NAME);
        match session {
            Some(s) => coll.insert_one(self).session(s).await,
            None => coll.insert_one(self).await,
        }
    }

    #[allow(dead_code)]
    pub async fn get_roads(self) -> Result<Vec<Road>, mongodb::error::Error>{
        let filter = doc!{
            "$or": [
                doc!{"place_one_id": self.category_id.to_string(),
                    "universe_id": self.universe_id,
                },
                doc!{"place_two_id": self.category_id.to_string(),
                    "universe_id": self.universe_id,
                }
            ]
        };
        let db_client = get_db_client().await;
        let cursor = db_client.database(VERSEENGINE_DB_NAME)
            .collection::<Road>(ROADS_COLLECTION_NAME)
            .find(filter)
            .await;
        cursor.expect("get_roads__collect_failed").try_collect().await
    }
}

#[allow(dead_code)]
pub async fn get_places_by_universe_id(universe_id: ObjectId) -> mongodb::error::Result<mongodb::Cursor<Place>> {
    let db_client = get_db_client().await;
    db_client
        .database(VERSEENGINE_DB_NAME)
        .collection::<Place>(PLACES_COLLECTION_NAME)
        .find(doc!{
            "universe_id": universe_id,
        })
        .await
}

pub async fn check_existing_place(universe_id: ObjectId, category_id: u64) -> mongodb::error::Result<Option<Place>> {
    let filter = doc!{"category_id": category_id.to_string(),
        "universe_id": universe_id,
    };
    let db_client = get_db_client().await;
    db_client
        .database(VERSEENGINE_DB_NAME)
        .collection::<Place>(PLACES_COLLECTION_NAME)
        .find_one(filter)
        .await
}

#[allow(dead_code)]
pub async fn get_place_by_role_id(universe_id: ObjectId, role_id: u64) -> mongodb::error::Result<Option<Place>> {
    let filter = doc!{"role": role_id.to_string(), "universe_id": universe_id,};
    let db_client = get_db_client().await;
    db_client
        .database(VERSEENGINE_DB_NAME)
        .collection::<Place>(PLACES_COLLECTION_NAME)
        .find_one(filter)
        .await
}

#[cfg(test)]
mod wiki_tests {
    use super::*;

    #[test]
    fn a_place_without_sheet_still_reads_and_the_sheet_round_trips() {
        let mut d = doc! {"_id": ObjectId::new(), "universe_id": ObjectId::new(), "server_id": "1", "category_id": "2", "role": "3", "name": "Arane", "modifiers": []};
        let p: Place = mongodb::bson::from_document(d.clone()).unwrap();
        assert_eq!(p.wiki, PlaceWiki::default());
        d.insert("wiki", doc! {"description": "Une ville.", "published": {"text": "Une ville.", "at": 5_i64}, "posts": [{"server_id": "10", "thread_id": "20"}]});
        let p: Place = mongodb::bson::from_document(d).unwrap();
        assert_eq!(p.wiki.posts, vec![WikiPost { server_id: 10, thread_id: 20 }]);
        assert_eq!(p.wiki.published, Some(PublishedText { text: "Une ville.".into(), at: 5 }));
        let back: Place = mongodb::bson::from_document(mongodb::bson::to_document(&p).unwrap()).unwrap();
        assert_eq!(back.wiki, p.wiki);
    }
}

pub async fn get_place_by_category_id(universe_id: ObjectId, category_id: u64) -> mongodb::error::Result<Option<Place>> {
    let filter = doc!{"category_id": category_id.to_string(), "universe_id": universe_id,};
    let db_client = get_db_client().await;
    db_client
        .database(VERSEENGINE_DB_NAME)
        .collection::<Place>(PLACES_COLLECTION_NAME)
        .find_one(filter)
        .await
}