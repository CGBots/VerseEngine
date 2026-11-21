use futures::{TryStreamExt};
use std::cmp::PartialEq;
use mongodb::bson::{doc, to_document};
use mongodb::bson::oid::ObjectId;
use mongodb::Cursor;
use mongodb::results::{InsertOneResult, UpdateResult};
use serde::{Deserialize, Serialize};
use serde_with::{DisplayFromStr, serde_as};
use crate::database::db_client::{get_db_client};
use crate::database::db_namespace::{VERSEENGINE_DB_NAME, SERVERS_COLLECTION_NAME, ROADS_COLLECTION_NAME, TRAVELS_COLLECTION_NAME};
use crate::database::characters::{get_character_by_user_id, Character};
use crate::database::road::{get_road, Road};
use crate::database::travel::TravelGroup;
use crate::database::universe::get_servers_from_universe;

/// Represents the type of a Discord identifier.
///
/// Used to distinguish between different Discord entity types
/// when storing and managing IDs.
#[serde_as]
#[derive(Serialize, Deserialize, Debug, Clone, Copy)]
pub enum IdType {
    Role,
    Channel,
    Category
}

/// Represents a Discord identifier with an associated type.
///
/// Combines a Discord snowflake ID (`u64`) with its corresponding
/// entity type (role, channel, or category).
#[serde_as]
#[derive(Serialize, Deserialize, Debug, Clone, Copy)]
pub struct Id{
    pub id: u64,
    pub id_type: IdType
}

impl From<(u64, IdType)> for Id {
    fn from((id, id_type): (u64, IdType)) -> Self {
        Id { id, id_type }
    }
}

impl From<u64> for Id {
    fn from(value: u64) -> Self {
        Id { id: value, id_type: IdType::Channel }  // Default to Channel type
    }
}

/// Represents a Discord server's configuration and associated universe.
///
/// Stores the server's Discord guild ID, associated universe, and optional
/// IDs for roles, categories, and channels used by the bot for roleplay
/// and moderation features.
#[serde_as]
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Server {
    /// MongoDB ObjectId for this document.
    #[serde(rename = "_id")]
    pub _id: ObjectId,

    /// Reference to the universe document `_id` (stored as string).
    pub universe_id: ObjectId,

    /// Discord guild ID.
    #[serde_as(as = "DisplayFromStr")]
    pub server_id: u64,

    /// Optional role IDs used by the bot.
    pub admin_role_id: Option<Id>,

    pub moderator_role_id: Option<Id>,

    pub spectator_role_id: Option<Id>,

    pub player_role_id: Option<Id>,

    pub everyone_role_id: Option<Id>,

    /// Optional category / channel IDs used as configuration anchors.
    pub admin_category_id: Option<Id>,

    pub nrp_category_id: Option<Id>,

    pub rp_category_id: Option<Id>,

    pub road_category_id: Option<Id>,

    pub rp_wiki_channel_id: Option<Id>,

    pub log_channel_id: Option<Id>,

    pub moderation_channel_id: Option<Id>,

    pub commands_channel_id: Option<Id>,

    pub nrp_general_channel_id: Option<Id>,

    pub rp_character_channel_id: Option<Id>,

    pub universal_time_channel_id: Option<Id>,

    /// Identifier of the message the bot keeps editing in the universal time channel
    /// (the day/night announcement). `None` until the first announcement, and for servers
    /// created before this field existed.
    #[serde_as(as = "Option<DisplayFromStr>")]
    #[serde(default)]
    pub universal_time_message_id: Option<u64>,

    pub universal_invite_url: Option<String>,
}

impl Default for Server {
    fn default() -> Self {
        Server{
            _id: Default::default(),
            universe_id: Default::default(),
            server_id: 0,
            admin_role_id: None,
            moderator_role_id: None,
            spectator_role_id: None,
            player_role_id: None,
            everyone_role_id: None,
            admin_category_id: None,
            nrp_category_id: None,
            rp_category_id: None,
            road_category_id: None,
            rp_wiki_channel_id: None,
            log_channel_id: None,
            moderation_channel_id: None,
            commands_channel_id: None,
            nrp_general_channel_id: None,
            rp_character_channel_id: None,
            universal_time_channel_id: None,
            universal_time_message_id: None,
            universal_invite_url: None,
        }
    }
}

impl PartialEq for Id {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl Server {
    /// Creates a deep clone of the server configuration.
    #[allow(unused)]
    pub fn clone(&self) -> Self {
        Self {
            _id: self._id.clone(),
            universe_id: self.universe_id.clone(),
            server_id: self.server_id.clone(),
            admin_role_id: self.admin_role_id.clone(),
            moderator_role_id: self.moderator_role_id.clone(),
            spectator_role_id: self.spectator_role_id.clone(),
            player_role_id: self.player_role_id.clone(),
            everyone_role_id: self.everyone_role_id.clone(),
            admin_category_id: self.admin_category_id.clone(),
            nrp_category_id: self.nrp_category_id.clone(),
            rp_category_id: self.rp_category_id.clone(),
            road_category_id: self.road_category_id.clone(),
            rp_wiki_channel_id: self.rp_wiki_channel_id.clone(),
            log_channel_id: self.log_channel_id.clone(),
            moderation_channel_id: self.moderation_channel_id.clone(),
            commands_channel_id: self.commands_channel_id.clone(),
            nrp_general_channel_id: self.nrp_general_channel_id.clone(),
            rp_character_channel_id: self.rp_character_channel_id.clone(),
            universal_time_channel_id: self.universal_time_channel_id.clone(),
            universal_time_message_id: self.universal_time_message_id,
            universal_invite_url: self.universal_invite_url.clone(),
        }
    }

    /// Inserts this server configuration into the database.
    ///
    /// # Errors
    ///
    /// Returns a MongoDB error if the insert operation fails.
    pub async fn insert_server(&self) -> mongodb::error::Result<InsertOneResult> {
        self.insert_server_with_optional_session(None).await
    }

    pub async fn insert_server_with_session(&self, session: &mut mongodb::ClientSession) -> mongodb::error::Result<InsertOneResult> {
        self.insert_server_with_optional_session(Some(session)).await
    }

    pub async fn insert_server_with_optional_session(&self, session: Option<&mut mongodb::ClientSession>) -> mongodb::error::Result<InsertOneResult> {
        let db_client = get_db_client().await;
        let coll = db_client
            .database(VERSEENGINE_DB_NAME)
            .collection::<Server>(SERVERS_COLLECTION_NAME);
        match session {
            Some(s) => coll.insert_one(self).session(s).await,
            None => coll.insert_one(self).await,
        }
    }

    /// Updates this server configuration in the database.
    ///
    /// Uses the `_id` field to locate and update the document.
    ///
    /// # Errors
    ///
    /// Returns a MongoDB error if the update operation fails.
    pub async fn update(&self) -> mongodb::error::Result<UpdateResult> {
        let mut doc = to_document(self).unwrap();
        doc.remove("_id");
        let filter = doc! {"_id": &self._id};
        let update = doc! {"$set": doc};

        let db_client = get_db_client().await;
        db_client
            .database(VERSEENGINE_DB_NAME)
            .collection::<Server>(SERVERS_COLLECTION_NAME)
            .update_one(filter, update).await
    }

    /// Sets the universe ID. Returns `self` for method chaining.
    pub fn universe_id(&mut self, universe_id: impl Into<ObjectId>) -> &mut Self {self.universe_id = universe_id.into(); self}

    /// Sets the Discord guild ID. Returns `self` for method chaining.
    pub fn server_id(&mut self, server_id: impl Into<u64>) -> &mut Self {self.server_id = server_id.into(); self}

    /// Sets the admin role ID. Returns `self` for method chaining.
    pub fn admin_role_id(&mut self, admin_role_id: impl Into<Id>) -> &mut Self {self.admin_role_id = Some(admin_role_id.into()); self}

    /// Sets the moderator role ID. Returns `self` for method chaining.
    pub fn moderator_role_id(&mut self, moderator_role_id: impl Into<Id>) -> &mut Self {self.moderator_role_id = Some(moderator_role_id.into()); self}
    /// Sets the spectator role ID. Returns `self` for method chaining.
    pub fn spectator_role_id(&mut self, spectator_role_id: impl Into<Id>) -> &mut Self {
        self.spectator_role_id = Some(spectator_role_id.into());
        self
    }
    /// Sets the player role ID. Returns `self` for method chaining.
    pub fn player_role_id(&mut self, player_role_id: impl Into<Id>) -> &mut Self {
        self.player_role_id = Some(player_role_id.into());
        self
    }
    /// Sets the everyone role ID. Returns `self` for method chaining.
    pub fn everyone_role_id(&mut self, everyone_role_id: impl Into<Id>) -> &mut Self {
        self.everyone_role_id = Some(everyone_role_id.into());
        self
    }
    /// Sets the admin category ID. Returns `self` for method chaining.
    pub fn admin_category_id(&mut self, admin_category_id: impl Into<Id>) -> &mut Self {
        self.admin_category_id = Some(admin_category_id.into());
        self
    }
    /// Sets the NRP (non-roleplay) category ID. Returns `self` for method chaining.
    pub fn nrp_category_id(&mut self, nrp_category_id: impl Into<Id>) -> &mut Self {
        self.nrp_category_id = Some(nrp_category_id.into());
        self
    }
    /// Sets the RP (roleplay) category ID. Returns `self` for method chaining.
    pub fn rp_category_id(&mut self, rp_category_id: impl Into<Id>) -> &mut Self {
        self.rp_category_id = Some(rp_category_id.into());
        self
    }
    /// Sets the road category ID. Returns `self` for method chaining.
    pub fn road_category_id(&mut self, road_category_id: impl Into<Id>) -> &mut Self {
        self.road_category_id = Some(road_category_id.into());
        self
    }
    /// Sets the RP wiki channel ID. Returns `self` for method chaining.
    pub fn rp_wiki_channel_id(&mut self, rp_wiki_channel_id: impl Into<Id>) -> &mut Self {
        self.rp_wiki_channel_id = Some(rp_wiki_channel_id.into());
        self
    }
    /// Sets the log channel ID. Returns `self` for method chaining.
    pub fn log_channel_id(&mut self, log_channel_id: impl Into<Id>) -> &mut Self {
        self.log_channel_id = Some(log_channel_id.into());
        self
    }
    /// Sets the moderation channel ID. Returns `self` for method chaining.
    pub fn moderation_channel_id(&mut self, moderation_channel_id: impl Into<Id>) -> &mut Self {
        self.moderation_channel_id = Some(moderation_channel_id.into());
        self
    }
    /// Sets the commands channel ID. Returns `self` for method chaining.
    pub fn commands_channel_id(&mut self, commands_channel_id: impl Into<Id>) -> &mut Self {
        self.commands_channel_id = Some(commands_channel_id.into());
        self
    }
    /// Sets the NRP general channel ID. Returns `self` for method chaining.
    pub fn nrp_general_channel_id(&mut self, nrp_general_channel_id: impl Into<Id>) -> &mut Self {
        self.nrp_general_channel_id = Some(nrp_general_channel_id.into());
        self
    }

    /// Sets the universal time channel ID. Returns `self` for method chaining.
    pub fn universal_time_channel_id(&mut self, universal_time_channel_id: impl Into<Id>) -> &mut Self {
        self.universal_time_channel_id = Some(universal_time_channel_id.into());
        self
    }
    /// Sets the RP character channel ID. Returns `self` for method chaining.
    pub fn rp_character_channel_id(&mut self, rp_character_channel_id: impl Into<Id>) -> &mut Self {self.rp_character_channel_id = Some(rp_character_channel_id.into()); self}

    pub async fn get_character_by_user_id(self, user_id: u64) -> mongodb::error::Result<Option<Character>> {
        get_character_by_user_id(self.universe_id, user_id).await
    }

    pub async fn has_character(self, user_id: u64) -> mongodb::error::Result<Option<Character>> {
        let player_result = self.get_character_by_user_id(user_id).await;
        match player_result {
            Ok(None) => { Ok(None) }
            Ok(Some(character)) => { Ok(Some(character)) }
            Err(e) => { Err(e) }
        }
    }

    pub async fn get_player_move(self, user_id: u64) -> mongodb::error::Result<Option<TravelGroup>> {
        let db_client = get_db_client().await;
        let filter = doc!{"members": user_id.to_string(), "universe_id": self.universe_id};
        db_client
            .database(VERSEENGINE_DB_NAME)
            .collection::<TravelGroup>(TRAVELS_COLLECTION_NAME)
            .find_one(filter)
            .await
    }

    pub async fn get_roads(self, place_id: u64) -> Result<Vec<Road>, mongodb::error::Error> {
        let db_client = get_db_client().await;
        let filter = doc!{
            "$or": [
                doc!{"place_one_id": place_id.to_string(), "universe_id": self.universe_id},
                doc!{"place_two_id": place_id.to_string(), "universe_id": self.universe_id},
            ]

        };
        let cursor = db_client.database(VERSEENGINE_DB_NAME)
            .collection::<Road>(ROADS_COLLECTION_NAME)
            .find(filter)
            .await;
        cursor.expect("get_roads__collect_failed").try_collect().await
    }
    
    pub async fn get_road(self, place_one: u64, place_two: u64) -> mongodb::error::Result<Option<Road>> {
        get_road(self.universe_id, place_one, place_two).await
    }
    
    pub async fn get_other_servers(&self) -> mongodb::error::Result<Cursor<Server>> {
        get_servers_from_universe(&self.universe_id).await
    }

    pub fn all_ids(&self) -> Vec<&Id> {
        [
            self.admin_role_id.as_ref(),
            self.moderator_role_id.as_ref(),
            self.spectator_role_id.as_ref(),
            self.player_role_id.as_ref(),
            self.everyone_role_id.as_ref(),
            self.admin_category_id.as_ref(),
            self.nrp_category_id.as_ref(),
            self.rp_category_id.as_ref(),
            self.road_category_id.as_ref(),
            self.rp_wiki_channel_id.as_ref(),
            self.log_channel_id.as_ref(),
            self.moderation_channel_id.as_ref(),
            self.commands_channel_id.as_ref(),
            self.nrp_general_channel_id.as_ref(),
            self.rp_character_channel_id.as_ref(),
            self.universal_time_channel_id.as_ref(),
        ]
            .into_iter()
            .flatten()
            .collect()
    }

    pub fn contains_id(&self, id: u64) -> bool {
        self.all_ids().iter().any(|entry| entry.id == id)
    }
}

/// Retrieves a server configuration by Discord guild ID.
///
/// # Errors
///
/// Returns a MongoDB error if the query fails.
pub async fn get_server_by_id(
    server_id: u64,
) -> mongodb::error::Result<Option<Server>> {
    let db_client = get_db_client().await;
    let filter = doc! {"server_id": server_id.to_string()};
    db_client
        .database(VERSEENGINE_DB_NAME)
        .collection::<Server>(SERVERS_COLLECTION_NAME)
        .find_one(filter)
        .await
}

/// Remembers (or forgets, with `None`) the id of the day/night announcement message of a server.
///
/// Only this field is written (`$set`): the rest of the server document is left untouched, so this
/// can safely run while a setup command is updating the same server.
///
/// # Errors
///
/// Returns a MongoDB error if the update fails.
pub async fn set_universal_time_message_id(
    server_id: u64,
    message_id: Option<u64>,
) -> mongodb::error::Result<()> {
    let db_client = get_db_client().await;
    let filter = doc! {"server_id": server_id.to_string()};
    let update = doc! {"$set": {"universal_time_message_id": message_id.map(|id| id.to_string())}};
    db_client
        .database(VERSEENGINE_DB_NAME)
        .collection::<Server>(SERVERS_COLLECTION_NAME)
        .update_one(filter, update)
        .await?;
    Ok(())
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn time_message_id_is_optional_in_existing_documents() {
        // Un serveur enregistré avant l'ajout du champ doit toujours pouvoir être lu.
        let mut document = to_document(&Server::default()).unwrap();
        document.remove("universal_time_message_id");
        let server: Server = mongodb::bson::from_document(document).unwrap();
        assert_eq!(server.universal_time_message_id, None);
    }

    #[test]
    fn time_message_id_is_stored_as_a_string_like_the_other_discord_ids() {
        let server = Server { universal_time_message_id: Some(1_234_567_890_123_456_789), ..Server::default() };
        let document = to_document(&server).unwrap();
        assert_eq!(document.get_str("universal_time_message_id").unwrap(), "1234567890123456789");

        let read_back: Server = mongodb::bson::from_document(document).unwrap();
        assert_eq!(read_back.universal_time_message_id, Some(1_234_567_890_123_456_789));
    }

    #[test]
    #[ignore = "integration test: needs a running MongoDB (MONGODB_* env vars); run with --ignored"]
    fn the_time_message_id_is_saved_without_touching_the_rest() {
        use crate::test_support::{block_on, random_id};
        block_on(async {
            let server_id = random_id();
            let mut server = Server::default();
            server.server_id(server_id).universal_time_channel_id(42u64);
            server.insert_server().await.expect("insert the test server");

            set_universal_time_message_id(server_id, Some(7)).await.unwrap();
            let saved = get_server_by_id(server_id).await.unwrap().unwrap();
            set_universal_time_message_id(server_id, None).await.unwrap();
            let cleared = get_server_by_id(server_id).await.unwrap().unwrap();

            get_db_client()
                .await
                .database(VERSEENGINE_DB_NAME)
                .collection::<Server>(SERVERS_COLLECTION_NAME)
                .delete_one(doc! {"server_id": server_id.to_string()})
                .await
                .unwrap();

            assert_eq!(saved.universal_time_message_id, Some(7));
            assert_eq!(saved.universal_time_channel_id.map(|c| c.id), Some(42));
            assert_eq!(cleared.universal_time_message_id, None);
        });
    }
}
