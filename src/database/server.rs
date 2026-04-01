use futures::{TryStreamExt};
use std::cmp::PartialEq;
use log::{log, Level};
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
use crate::database::travel::PlayerMove;
use crate::database::universe::get_servers_from_universe;
use crate::discord::poise_structs::{Context, Error};

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

/// Extension trait for deleting Discord entities represented by `Option<Id>`.
pub trait IdExt {
    async fn delete(&mut self, ctx: &Context<'_>) -> Result<&'static str, Error>;
}

// Implement the trait for Option<Id>
impl IdExt for Option<Id> {
    /// Deletes the Discord entity (role or channel) and sets `self` to `None` on success.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - `self` is `None` (`"id__nothing_to_delete"`)
    /// - Not in a guild context (`"guild_only"`)
    /// - The deletion fails (`"id__role_delete_failed"` or `"id__channel_delete_failed"`)
    async fn delete(&mut self, ctx: &Context<'_>) -> Result<&'static str, Error> {
        match self {
            None => Err("id__nothing_to_delete".into()),
            Some(id) => {
                let guild_id = ctx.guild_id().ok_or_else(|| -> Error { "guild_only".into() })?;
                let http = ctx.http();

                match id.id_type {
                    IdType::Role => {
                        // Use HTTP via GuildId; do NOT borrow ctx.guild() (cache) across await.
                        match guild_id.delete_role(http, id.id).await {
                            Ok(_) => {
                                *self = None;
                                Ok("id__role_delete_success")
                            }
                            Err(_) => Err("id__role_delete_failed".into()),
                        }
                    }
                    _ => {
                        if http.delete_channel(id.id.into(), None).await.is_ok() {
                            *self = None;
                            Ok("id__channel_delete_sucess")
                        } else {
                            Err("id__channel_delete_failed".into())
                        }
                    }
                }
            }
        }
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
            universal_invite_url: self.universal_invite_url.clone(),
        }
    }

    /// Inserts this server configuration into the database.
    ///
    /// # Errors
    ///
    /// Returns a MongoDB error if the insert operation fails.
    pub async fn insert_server(&self) -> mongodb::error::Result<InsertOneResult> {
        let db_client = get_db_client().await;
        db_client
            .database(VERSEENGINE_DB_NAME)
            .collection::<Server>(SERVERS_COLLECTION_NAME)
            .insert_one(self)
            .await
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

    /// Rolls back the current server configuration to a previous snapshot state.
    ///
    /// This asynchronous method compares the current server configuration with a provided
    /// snapshot and deletes any roles, categories, or channels that differ between them.
    /// All deletion operations are executed concurrently using `join_all`, and any errors
    /// encountered during deletion are logged.
    ///
    /// # Parameters
    ///
    /// - `ctx`: A reference to the Discord context, used for performing Discord API operations
    ///   such as deleting roles and channels.
    /// - `snapshot`: A previous state of the `Server` instance to which the current state should
    ///   be rolled back. Fields that differ from the snapshot will be deleted.
    ///
    /// # Behavior
    ///
    /// The method performs the following steps:
    /// 1. Creates an array of mutable references to the current server's role/channel fields
    ///    paired with their corresponding snapshot values.
    /// 2. Filters the fields to identify those that differ between the current state and the snapshot.
    /// 3. Collects futures for deleting the differing fields using the `IdExt::delete` method.
    /// 4. Executes all deletion operations concurrently using `join_all`.
    /// 5. Logs any errors that occur during deletion with error-level logging, including
    ///    the universe ID, server ID, and error message.
    ///
    /// # Fields Rolled Back
    ///
    /// The following fields are checked and potentially rolled back:
    /// - `admin_role_id`
    /// - `moderator_role_id`
    /// - `spectator_role_id`
    /// - `player_role_id`
    /// - `admin_category_id`
    /// - `nrp_category_id`
    /// - `rp_category_id`
    /// - `road_category_id`
    /// - `rp_wiki_channel_id`
    /// - `log_channel_id`
    /// - `moderation_channel_id`
    /// - `commands_channel_id`
    /// - `nrp_general_channel_id`
    /// - `rp_character_channel_id`
    ///
    /// # Example
    ///
    /// ```rust
    /// let mut server = Server::default();
    /// let snapshot = server.clone();
    /// // ... modify server ...
    /// server.rollback(&ctx, snapshot).await;
    /// ```
    ///
    /// # Errors
    ///
    /// Errors during deletion operations are logged but do not cause the function to return
    /// an error. This allows the rollback to proceed even if some deletions fail.
    ///
    /// # Notes
    ///
    /// - This method requires the `futures` crate for `join_all`.
    /// - Errors are logged using the `log` crate at the `Error` level.
    pub async fn rollback(&mut self, ctx: &Context<'_>, snapshot: Self) {
        use futures::future::join_all;

        let mut fields = [
            (&mut self.admin_role_id, snapshot.admin_role_id),
            (&mut self.moderator_role_id, snapshot.moderator_role_id),
            (&mut self.spectator_role_id, snapshot.spectator_role_id),
            (&mut self.player_role_id, snapshot.player_role_id),
            (&mut self.admin_category_id, snapshot.admin_category_id),
            (&mut self.nrp_category_id, snapshot.nrp_category_id),
            (&mut self.rp_category_id, snapshot.rp_category_id),
            (&mut self.road_category_id, snapshot.road_category_id),
            (&mut self.rp_wiki_channel_id, snapshot.rp_wiki_channel_id),
            (&mut self.log_channel_id, snapshot.log_channel_id),
            (&mut self.moderation_channel_id, snapshot.moderation_channel_id),
            (&mut self.commands_channel_id, snapshot.commands_channel_id),
            (&mut self.nrp_general_channel_id, snapshot.nrp_general_channel_id),
            (&mut self.rp_character_channel_id, snapshot.rp_character_channel_id),
            (&mut self.universal_time_channel_id, snapshot.universal_time_channel_id),
        ];

        let delete_futures: Vec<_> = fields
            .iter_mut()
            .filter_map(|(field, snapshot_field)| {
                if **field != *snapshot_field {
                    Some(field.delete(ctx))
                } else {
                    None
                }
            })
            .collect();

        let results = join_all(delete_futures).await;
        results.iter().for_each(|r| {
            if let Err(err) = r {
                log!(
                    Level::Error,
                    "Error during setup and rollback.\nuniverse_id: {}\nserver_id: {}\n error: {}",
                    self.universe_id,
                    self.server_id,
                    err
                );
            }
        });
    }

    /// Creates a validated snapshot of the current server configuration.
    ///
    /// Verifies that all role and channel IDs still exist in the Discord guild.
    /// Any IDs that no longer exist are set to `None` in the returned snapshot.
    ///
    /// # Panics
    ///
    /// Panics if the guild ID cannot be retrieved or if the HTTP requests to
    /// retrieve guild roles or channels fail.
    pub async fn snaphot(self, ctx: &Context<'_>) -> Self {
        let mut snapshot = self.clone();
        let guild_id = ctx.guild_id().unwrap();
        let roles = ctx.http().get_guild_roles(guild_id.into()).await.unwrap();
        let channels = ctx.http().get_channels(guild_id.into()).await.unwrap();


        let role_exists = |id: u64| roles.iter().any(|r| r.id.get() == id);

        if snapshot.admin_role_id.map(|x| role_exists(x.id)) == Some(false) {
            snapshot.admin_role_id = None;
        }
        if snapshot.moderator_role_id.map(|x| role_exists(x.id)) == Some(false) {
            snapshot.moderator_role_id = None;
        }
        if snapshot.spectator_role_id.map(|x| role_exists(x.id)) == Some(false) {
            snapshot.spectator_role_id = None;
        }
        if snapshot.player_role_id.map(|x| role_exists(x.id)) == Some(false) {
            snapshot.player_role_id = None;
        }
        if snapshot.everyone_role_id.map(|x| role_exists(x.id)) == Some(false) {
            snapshot.everyone_role_id = None;
        }

        let channel_exists = |id: u64| channels.iter().any(|r| r.id.get() == id);

        // ---- Channels/categories: check via get_channel ----

        if snapshot.road_category_id.map(|x| channel_exists(x.id)) == Some(false) {
            snapshot.road_category_id = None;
        }
        if snapshot.admin_category_id.map(|x| channel_exists(x.id)) == Some(false) {
            snapshot.admin_category_id = None;
        }
        if snapshot.nrp_category_id.map(|x| channel_exists(x.id)) == Some(false) {
            snapshot.nrp_category_id = None;
        }
        if snapshot.rp_category_id.map(|x| channel_exists(x.id)) == Some(false) {
            snapshot.rp_category_id = None;
        }
        if snapshot.log_channel_id.map(|x| channel_exists(x.id)) == Some(false) {
            snapshot.log_channel_id = None;
        }
        if snapshot.commands_channel_id.map(|x| channel_exists(x.id)) == Some(false) {
            snapshot.commands_channel_id = None;
        }
        if snapshot.moderation_channel_id.map(|x| channel_exists(x.id)) == Some(false) {
            snapshot.moderation_channel_id = None;
        }
        if snapshot.nrp_general_channel_id.map(|x| channel_exists(x.id)) == Some(false) {
            snapshot.nrp_general_channel_id = None;
        }
        if snapshot.rp_character_channel_id.map(|x| channel_exists(x.id)) == Some(false) {
            snapshot.rp_character_channel_id = None;
        }
        if snapshot.rp_wiki_channel_id.map(|x| channel_exists(x.id)) == Some(false) {
            snapshot.rp_wiki_channel_id = None;
        }
        if snapshot.universal_time_channel_id.map(|x| channel_exists(x.id)) == Some(false) {
            snapshot.universal_time_channel_id = None;
        }

        snapshot
    }

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

    pub async fn get_player_move(self, user_id: u64) -> mongodb::error::Result<Option<PlayerMove>> {
        let db_client = get_db_client().await;
        let filter = doc!{"user_id": user_id.to_string().as_str(), "universe_id": self.universe_id};
        db_client
            .database(VERSEENGINE_DB_NAME)
            .collection::<PlayerMove>(TRAVELS_COLLECTION_NAME)
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

#[cfg(test)]
mod test {
    use crate::database::universe::Universe;
    use std::time::SystemTime;
    use lazy_static::lazy_static;
    use super::*;

    static SERVER_ID: u64 = 1;

    lazy_static! {
        pub static ref UNIVERSE_ID: ObjectId = ObjectId::new();
    }

    async fn insert_universe() -> Result<InsertOneResult, String> {
        let _ = get_db_client().await;
        let universe = Universe {
            universe_id: *UNIVERSE_ID,
            name: "test".to_string(),
            creator_id: 0,
            global_time_modifier: 100,
            time_origin_timestamp: SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap()
                .as_millis(),
            creation_timestamp: SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap()
                .as_millis(),
        };
        match universe.insert_universe().await {
            Ok(universe) => Ok(universe),
            Err(e) => {
                println!("{}", e);
                Err(e.to_string())
            }
        }
    }

    #[tokio::test]
    async fn test_insert_server() {
        insert_universe().await.unwrap();
        let result = Server::default()
            .insert_server()
            .await;

        assert!(result.is_ok());
    }
}
