use futures::TryStreamExt;
use crate::database::db_client::{get_db_client};
use crate::database::db_namespace::{
    CHARACTERS_COLLECTION_NAME, TRAVELS_COLLECTION_NAME, VERSEENGINE_DB_NAME,
    SERVERS_COLLECTION_NAME, STATS_COLLECTION_NAME, UNIVERSES_COLLECTION_NAME,
    PLACES_COLLECTION_NAME, ROADS_COLLECTION_NAME
};
use mongodb::bson::{doc, from_document};
use mongodb::bson::oid::ObjectId;
use mongodb::{Cursor, IndexModel};
use mongodb::options::{IndexOptions};
use mongodb::results::{CreateIndexResult, InsertOneResult};
use serde::{Deserialize, Serialize};
use serde_with::{DisplayFromStr, serde_as};
use tokio::join;
use crate::database::characters::Character;
use crate::database::places::Place;
use crate::database::road::Road;
use crate::database::server::{Server};
use crate::database::stats::Stat;
use crate::database::travel::PlayerMove;
use crate::discord::poise_structs::Error;

pub static FREE_LIMIT_UNIVERSE: usize = 2;

pub static FREE_LIMIT_SERVERS_PER_UNIVERSE: usize = 2;

/// Represents a Universe entity with associated metadata.
///
/// This struct is serializable and deserializable using Serde with custom field attributes.
///
/// # Fields
///
/// * `universe_id` (`ObjectId`):
///   The unique identifier for the universe. Serialized as `_id`.
///
/// * `name` (`String`):
///   The name of the universe.
///
/// * `creator_id` (`u64`):
///   The unique identifier of the creator. Serialized as a string using the `DisplayFromStr` attribute.
///
/// * `global_time_modifier` (`u32`):
///   A global time modifier for the universe. Serialized as a string using the `DisplayFromStr` attribute.
///
/// * `creation_timestamp` (`u128`):
///   The timestamp of when the universe was created.
///   Serialized as a string using the `DisplayFromStr` attribute.
///
/// # Serde Attributes
///
/// * `#[serde_as]`:
///   Enables the use of Serde's custom serialization and deserialization behaviors.
///
/// * `#[serde(rename = "_id")]`:
///   Renames the `universe_id` field to `_id` during serialization/deserialization.
#[serde_as]
#[derive(Serialize, Deserialize, Debug)]
pub struct Universe {
    #[serde(rename = "_id")]
    pub universe_id: ObjectId,

    pub name: String,

    #[serde_as(as = "DisplayFromStr")]
    pub creator_id: u64,

    #[serde_as(as = "DisplayFromStr")]
    pub global_time_modifier: u32,

    #[serde_as(as = "DisplayFromStr")]
    pub time_origin_timestamp: u128,

    #[serde_as(as = "DisplayFromStr")]
    pub creation_timestamp: u128,
}

impl Universe {
    /// Inserts the current `Universe` instance into the MongoDB collection.
    ///
    /// # Returns
    ///
    /// Returns a `Result` containing:
    /// - `InsertOneResult` on success, which includes information about the inserted document (e.g., its ObjectId).
    /// - `mongodb::error::Error` on failure, if there are issues with database connectivity or the insert operation.
    ///
    /// # Errors
    ///
    /// This function will return an error if:
    /// - The connection to the database can't be established.
    /// - The insert operation fails due to constraints or other database issues.
    ///
    /// # Example
    ///
    /// ```rust
    /// let universe = Universe {
    ///     // initialize the `Universe` instance with required fields.
    /// };
    /// match universe.insert_universe().await {
    ///     Ok(result) => println!("Inserted with id: {:?}", result.inserted_id),
    ///     Err(e) => eprintln!("Failed to insert universe: {:?}", e),
    /// }
    /// ```
    ///
    /// # Notes
    ///
    /// - This function relies on a globally initialized MongoDB client `DB_CLIENT`. Ensure the client is properly
    ///   configured before invoking this function.
    /// - The database and collection names are derived from constants `RPBOT_DB_NAME` and `UNIVERSE_COLLECTION_NAME`.
    pub async fn insert_universe(&self) -> mongodb::error::Result<InsertOneResult> {
        let db_client = get_db_client().await;
        db_client
            .database(VERSEENGINE_DB_NAME)
            .collection::<Universe>(UNIVERSES_COLLECTION_NAME)
            .insert_one(self)
            .await
    }

    /// Asynchronously retrieves a list of universes created by a specific user.
    ///
    /// This function connects to the database and queries the `Universe` collection
    /// to retrieve all universes associated with the provided `user_id`.
    /// The function relies on initializing a shared database client if not already initialized.
    ///
    /// # Arguments
    ///
    /// * `user_id` - A `u64` representing the ID of the user whose universes are to be retrieved.
    ///
    /// # Returns
    ///
    /// Returns a `Vec<Universe>` containing the universes created by the specified user.
    ///
    /// # Panics
    ///
    /// This function will panic if:
    /// * The database connection fails to initialize.
    /// * The query to the `Universe` collection fails or encounters an error.
    ///
    /// # Examples
    ///
    /// ```rust
    /// let user_id = 12345;
    /// let universes = get_creator_universes(user_id).await;
    /// println!("{:?}", universes);
    /// ```
    ///
    /// # Dependencies
    ///
    /// * Assumes a global `DB_CLIENT` instance exists, which is initialized with a database client.
    /// * The `RPBOT_DB_NAME` constant specifies the name of the database.
    /// * The `UNIVERSE_COLLECTION_NAME` constant specifies the collection to query.
    /// * Requires the `try_collect()` method to process the query results into a vector.
    ///
    /// # Notes
    ///
    /// Make sure the database is properly configured and accessible, and that `connect_db()`
    /// is implemented to initialize the database connection.
    pub async fn get_creator_universes(user_id: u64) -> Vec<Universe> {
        let db_client = get_db_client().await;
        let filter = doc! { "creator_id": user_id.to_string() };
        db_client
            .database(VERSEENGINE_DB_NAME)
            .collection::<Universe>(UNIVERSES_COLLECTION_NAME)
            .find(filter)
            .await
            .unwrap().try_collect().await.unwrap()
    }

    /// Asynchronously checks whether a user has reached the limit for creating universes.
    ///
    /// # Parameters
    /// - `user_id`: The unique identifier of the user whose universe count needs to be checked.
    ///
    /// # Returns
    /// - `Result<bool, Error>`:
    ///    - `Ok(true)`: If the number of universes created by the user is within the allowed free limit.
    ///    - `Ok(false)`: If the number of universes created by the user exceeds the allowed free limit.
    ///    - `Err(Error)`: If there is an error during the database interaction.
    ///
    /// # Behavior
    /// - Establishes a connection to the database using the globally initialized `DB_CLIENT`.
    /// - Constructs a MongoDB filter to count documents in the `UNIVERSE_COLLECTION_NAME` where `creator_id` matches the given `user_id`.
    /// - Compares the retrieved document count with `FREE_LIMIT_UNIVERSE`.
    ///
    /// # Panics
    /// - This function will panic if the `DB_CLIENT` initialization fails. The `.unwrap()` call during the `connect_db` process assumes successful database connection.
    ///
    /// # Example usage
    /// ```rust
    /// use crate::check_universe_limit;
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///     let user_id: u64 = 12345;
    ///
    ///     match check_universe_limit(user_id).await {
    ///         Ok(true) => println!("User has not exceeded the free limit for universes."),
    ///         Ok(false) => println!("User has exceeded the free limit for universes."),
    ///         Err(e) => eprintln!("An error occurred: {}", e),
    ///     }
    /// }
    /// ```
    ///
    /// # Notes
    /// - The function assumes that `DB_CLIENT`, `RPBOT_DB_NAME`, `UNIVERSE_COLLECTION_NAME`, and `FREE_LIMIT_UNIVERSE`
    ///   are properly defined and accessible globally within the context of this application.
    ///
    /// # Dependencies
    /// - This function depends on MongoDB's `count_documents` API.
    /// - Assumes the application is using the `mongodb` crate and a MongoDB-compatible database.
    pub async fn check_universe_limit(user_id: u64) -> Result<bool, Error> {
        let db_client = get_db_client().await;
        let filter = doc! { "creator_id": user_id.to_string() };
        let result  = db_client
            .database(VERSEENGINE_DB_NAME)
            .collection::<Universe>(UNIVERSES_COLLECTION_NAME)
            .count_documents(filter)
            .await;

        println!("{:?}", result); //got Ok(0)

        match result {
            Ok(count) => Ok(count < FREE_LIMIT_UNIVERSE as u64),
            Err(e) => {
                log::error!("Error counting universes for user {}: {}", user_id, e);
                Err(e.into())
            }
        }
    }

    /// Asynchronously adds a server to the universe in the database.
    ///
    /// # Parameters
    /// - `server`: The `Server` instance to be added. The `universe_id` is set to the current universe's ID
    ///   before insertion into the database.
    ///
    /// # Returns
    /// - A `mongodb::error::Result<InsertOneResult>` indicating the outcome of the insertion.
    ///   - On success, returns the `InsertOneResult` containing information about the inserted document.
    ///   - On failure, returns a `mongodb::error::Error` detailing the issue.
    ///
    /// # Behavior
    /// - The function initializes and retrieves a shared database client using `DB_CLIENT`.
    /// - The `universe_id` of the `server` is updated to correspond with the current `universe_id`.
    /// - The function accesses the specified database and collection, and attempts to insert the updated server object.
    ///
    /// # Errors
    /// - This function will return an error if:
    ///   - The database client cannot be initialized or connected.
    ///   - The insertion operation into the database fails.
    ///
    /// # Example
    /// ```rust
    /// let server = Server::new(/* initialize server fields */);
    /// let result = some_instance.add_server_to_universe(server).await;
    ///
    /// match result {
    ///     Ok(insert_result) => println!("Server added successfully: {:?}", insert_result),
    ///     Err(e) => println!("Error adding server: {:?}", e),
    /// }
    /// ```
    ///
    /// # Requirements
    /// - The function assumes the presence of `DB_CLIENT`, which should be a once-initialized global connection pool instance.
    /// - `RPBOT_DB_NAME` and `SERVER_COLLECTION_NAME` must define the database and collection names respectively.
    /// - The `Server` struct must correctly implement serialization and have a method `universe_id` to set the `universe_id`.
    pub async fn add_server_to_universe(
        &self,
        mut server: Server,
    ) -> mongodb::error::Result<InsertOneResult> {
        let db_client = get_db_client().await;

        let serv = server.universe_id(self.universe_id);

        db_client
            .database(VERSEENGINE_DB_NAME)
            .collection::<Server>(SERVERS_COLLECTION_NAME)
            .insert_one(serv)
            .await
    }

    /// Creates a deep copy of the current instance of the object.
    ///
    /// # Returns
    /// A new instance of the object with all fields cloned from the original.
    ///
    /// # Example
    /// ```
    /// let original = MyStruct {
    ///     universe_id: 123,
    ///     name: String::from("Universe"),
    ///     creator_id: 456,
    ///     global_time_modifier: 1.5,
    ///     creation_timestamp: Some(1609459200),
    /// };
    ///
    /// let cloned = original.clone();
    ///
    /// assert_eq!(original.universe_id, cloned.universe_id);
    /// assert_eq!(original.name, cloned.name);
    /// assert_eq!(original.creator_id, cloned.creator_id);
    /// assert_eq!(original.global_time_modifier, cloned.global_time_modifier);
    /// assert_eq!(original.creation_timestamp, cloned.creation_timestamp);
    /// assert_ne!(std::ptr::addr_of!(original), std::ptr::addr_of!(cloned)); // Different memory locations
    /// ```
    #[allow(unused)]
    pub fn clone(&self) -> Self {
        Self {
            universe_id: self.universe_id.clone(),
            name: self.name.clone(),
            creator_id: self.creator_id.clone(),
            global_time_modifier: self.global_time_modifier.clone(),
            time_origin_timestamp: self.time_origin_timestamp.clone(),
            creation_timestamp: self.creation_timestamp.clone(),
        }
    }

    /// Asynchronously checks if a given user owns the universe associated with the provided server ID.
    ///
    /// # Parameters
    ///
    /// * `server_id` - A `u64` representing the unique identifier of the server.
    /// * `user_id` - A `u64` representing the unique identifier of the user.
    ///
    /// # Returns
    ///
    /// * `Ok(true)` - If the user is the creator/owner of the universe.
    /// * `Ok(false)` - If the user is not the creator/owner of the universe.
    /// * `Err(String)` - If the universe associated with the given `server_id` was not found or
    ///   if an error occurred during the retrieval process. The error string contains the error context:
    ///   - `"check_universe_ownership__universe_not_found"` if the universe couldn't be retrieved.
    ///
    /// # Errors
    ///
    /// This function returns an error if:
    /// - The universe associated with the `server_id` does not exist.
    /// - An error occurs during the execution of `get_universe_by_server_id`.
    ///
    /// # Example
    ///
    /// ```rust
    /// # use your_module::your_struct::check_universe_ownership;
    /// # async fn example() {
    /// let server_id = 12345;
    /// let user_id = 67890;
    ///
    /// match check_universe_ownership(server_id, user_id).await {
    ///     Ok(is_owner) => {
    ///         if is_owner {
    ///             println!("User owns the universe associated with the server.");
    ///         } else {
    ///             println!("User does not own the universe associated with the server.");
    ///         }
    ///     }
    ///     Err(err) => println!("Error occurred: {}", err),
    /// }
    /// # }
    /// ```
    #[allow(unused)]
    pub async fn check_universe_ownership(server_id: u64, user_id: u64) -> Result<bool, String> {
        let result = get_universe_by_server_id(server_id).await;
        match result {
            Ok(cursor) => {
                match cursor {
                    Some(universe) => {
                        if universe.creator_id == user_id { Ok(true) } else { Ok(false) }
                    }
                    None => { Err("check_universe_ownership__universe_not_found".to_string()) }
                }
            }
            Err(_) => { Err("check_universe_ownership__universe_not_found".to_string()) }
        }
    }

    /// Deletes the universe and its associated servers from the database.
    ///
    /// This function performs the following operations:
    /// 1. Connects to the database client using a globally initialized asynchronous client.
    /// 2. Drops the database corresponding to the `universe_id`.
    /// 3. Deletes metadata for the universe from the `Universe` collection.
    /// 4. Deletes metadata for its associated servers from the `Server` collection.
    /// 5. Performs error handling to check if any of the above operations fail.
    ///
    /// ## Returns
    /// - `Ok(&str)` with a success message `"universe_delete__passed"` if all operations succeed.
    /// - `Err(Error)` with an error message `"universe_delete__failed"` if any operation fails.
    ///
    /// ## Errors
    /// This function will return an error if:
    /// - The database cannot be dropped.
    /// - The universe metadata cannot be deleted from the `Universe` collection.
    /// - The server metadata cannot be deleted from the `Server` collection.
    ///
    /// ## Example
    /// ```rust
    /// let result = my_object.delete().await;
    /// match result {
    ///     Ok(message) => println!("Success: {}", message),
    ///     Err(err) => eprintln!("Error: {}", err),
    /// }
    /// ```
    ///
    /// ## Note
    /// - Ensure that `DB_CLIENT`, `RPBOT_DB_NAME`, `UNIVERSE_COLLECTION_NAME`, and `SERVER_COLLECTION_NAME`
    ///   are properly defined and initialized in your application.
    /// - The function assumes that `self.universe_id` is correctly set and corresponds to a valid universe.
    pub async fn delete(&self) -> Result<&str, Error> {
        let db_client = get_db_client().await;
        let db = db_client.database(VERSEENGINE_DB_NAME);
        let filter = doc! {"universe_id": self.universe_id};

        let universes = db.collection::<Universe>(UNIVERSES_COLLECTION_NAME);
        let servers = db.collection::<Server>(SERVERS_COLLECTION_NAME);
        let places = db.collection::<Place>(PLACES_COLLECTION_NAME);
        let stats = db.collection::<Stat>(STATS_COLLECTION_NAME);
        let roads = db.collection::<Road>(ROADS_COLLECTION_NAME);
        let characters = db.collection::<Character>(CHARACTERS_COLLECTION_NAME);
        let travels = db.collection::<PlayerMove>(TRAVELS_COLLECTION_NAME);

        let universe_delete = universes.delete_one(doc! {"_id": self.universe_id});
        let servers_delete = servers.delete_many(filter.clone());
        let places_delete = places.delete_many(filter.clone());
        let stats_delete = stats.delete_many(filter.clone());
        let roads_delete = roads.delete_many(filter.clone());
        let characters_delete = characters.delete_many(filter.clone());
        let travels_delete = travels.delete_many(filter);

        let (universe_res, servers_res, places_res, stats_res, roads_res, characters_res, travels_res) = join!(
            universe_delete,
            servers_delete,
            places_delete,
            stats_delete,
            roads_delete,
            characters_delete,
            travels_delete
        );

        if universe_res.is_err() || servers_res.is_err() || places_res.is_err() || stats_res.is_err() 
            || roads_res.is_err() || characters_res.is_err() || travels_res.is_err() {
            return Err("universe_delete__failed".into());
        }

        Ok("universe_delete__passed")
    }
    
    /// Sets up a unique index on the `name` field within the `STATS_COLLECTION_NAME` 
    /// collection of the MongoDB database corresponding to the `universe_id`.
    ///
    /// This function performs the following:
    /// - Initializes a MongoDB client (`DB_CLIENT`) if it's not already initialized.
    /// - Constructs an index with the `name` field as the key and specifies it as unique.
    /// - Builds the index model using the key and options.
    /// - Applies the index to the corresponding collection in the specified database.
    ///
    /// # Returns
    /// 
    /// - `mongodb::error::Result<CreateIndexResult>`: Returns the result of the `create_index` operation,
    ///   which includes information about the newly created index or any error that occurs.
    ///
    /// # Errors
    ///
    /// This function returns a `mongodb::error::Error` in case of:
    /// - Issues with database connection initialization.
    /// - Errors occurring during the process of creating the index in the database.
    ///
    /// # Example
    ///
    /// ```rust
    /// let result = your_instance.setup_constraints().await?;
    /// println!("Index successfully created: {:?}", result);
    /// ```
    ///
    /// # Notes
    /// - This function assumes that `connect_db()` establishes a valid connection to the MongoDB instance.
    /// - The index enforces uniqueness on the `name` field, ensuring no duplicate values exist for this field 
    ///   across the collection.
    pub async fn setup_constraints(&self) -> mongodb::error::Result<CreateIndexResult> {
        let db_client = get_db_client().await;
        let index_keys = doc! {"name": 1, "universe_id": 1};
        let index_options = IndexOptions::builder().unique(true).build();
        let index_model = IndexModel::builder()
            .keys(index_keys)
            .options(index_options)
            .build();
        db_client
            .database(VERSEENGINE_DB_NAME)
            .collection::<Stat>(STATS_COLLECTION_NAME)
            .create_index(index_model)
            .await
    }

    /// Asynchronously checks if the number of servers within a specified universe
    /// has reached the predefined limit.
    ///
    /// # Parameters
    /// - `self`: The instance of the struct containing the `universe_id` to be checked.
    ///
    /// # Returns
    /// - `Ok(true)`: If the number of servers is below the specified free limit.
    /// - `Ok(false)`: If the number of servers has reached or exceeded the specified free limit.
    /// - `Err(&'static str)`: If there is a failure in querying the database, an error message is returned.
    ///
    /// # Errors
    /// - Returns `"universe__check_server_limit_failed"` if there is an issue when attempting to query
    ///   the database for the number of servers in the specified universe.
    ///
    /// # Database Connection
    /// - This function establishes or uses an existing connection to the database (`db_client`).
    /// - The database and collection names are defined by the constants `RPBOT_DB_NAME` and 
    ///   `SERVER_COLLECTION_NAME`.
    ///
    /// # Notes
    /// - The limit for the number of servers per universe is defined by the constant `FREE_LIMIT_UNIVERSE`.
    /// - This function clones the database client for use in querying.
    ///
    /// # Example
    /// ```rust
    /// let universe_checker = UniverseChecker { universe_id: "some_universe_id".to_string() };
    /// match universe_checker.check_server_limit().await {
    ///     Ok(is_within_limit) => {
    ///         if is_within_limit {
    ///             println!("Server limit not reached.");
    ///         } else {
    ///             println!("Server limit reached.");
    ///         }
    ///     }
    ///     Err(error) => {
    ///         eprintln!("Error checking server limit: {}", error);
    ///     }
    /// }
    /// ```
    pub async fn check_server_limit(self) -> Result<bool, &'static str> {
        let db_client = get_db_client().await;
        let filter = doc!{"universe_id": self.universe_id};
        let servers_result_request = db_client
            .database(VERSEENGINE_DB_NAME)
            .collection::<Server>(SERVERS_COLLECTION_NAME)
            .count_documents(filter)
            .await;

        match servers_result_request {
            Ok(server_count) => {
                Ok(server_count < FREE_LIMIT_SERVERS_PER_UNIVERSE as u64)
            }
            Err(e) => { println!("{:?}", e); return Err("universe__check_server_limit_failed".into()) }
        }
    }

    pub async fn get_stats(self) -> mongodb::error::Result<Cursor<Stat>> {
        let db_client = get_db_client().await;
        let filter = doc!{"universe_id": self.universe_id};
        db_client
            .database(VERSEENGINE_DB_NAME)
            .collection::<Stat>(STATS_COLLECTION_NAME)
            .find(filter)
            .await
    }

    pub async fn get_player_by_user_id(self, user_id: u64) -> mongodb::error::Result<Option<Character>> {
        let db_client = get_db_client().await;
        let filter = doc!{"user_id": user_id.to_string(), "universe_id": self.universe_id};
        db_client
            .database(VERSEENGINE_DB_NAME)
            .collection::<Character>(CHARACTERS_COLLECTION_NAME)
            .find_one(filter)
            .await
    }

    pub async fn has_character(self, user_id: u64) -> mongodb::error::Result<Option<Character>> {
        let player_result = self.get_player_by_user_id(user_id).await;
        match player_result {
            Ok(None) => { Ok(None) }
            Ok(Some(character)) => { Ok(Some(character)) }
            Err(e) => { Err(e) }
        }
    }

    pub async fn get_all_universes() -> mongodb::error::Result<Vec<Universe>> {
        let db_client = get_db_client().await;
        let mut cursor = db_client
            .database(VERSEENGINE_DB_NAME)
            .collection::<Universe>(UNIVERSES_COLLECTION_NAME)
            .find(doc!{})
            .await?;

        let mut universes = Vec::new();
        while let Some(universe) = cursor.try_next().await? {
            universes.push(universe);
        }
        Ok(universes)
    }

}

pub async fn get_servers_from_universe(universe_id: &ObjectId) -> mongodb::error::Result<Cursor<Server>> {
    let db_client = get_db_client().await;
    let filter = doc! { "universe_id": universe_id};
    db_client
        .database(VERSEENGINE_DB_NAME)
        .collection::<Server>(SERVERS_COLLECTION_NAME)
        .find(filter)
        .await
}

/// Asynchronously retrieves a `Universe` document from the database by its unique identifier.
///
/// # Arguments
///
/// * `universe_id` - A `String` representing the unique identifier of the universe to be retrieved.
///                   This ID should be a valid string representation of a MongoDB ObjectId.
///
/// # Returns
///
/// Returns a `Result` wrapping an `Option<Universe>`.
/// * `Ok(Some<Universe>)` - If a document with the specified ID exists in the database.
/// * `Ok(None)` - If no document was found with the specified ID.
/// * `Err(mongodb::error::Error)` - If an error occurs during the database operation.
///
/// # Errors
///
/// This function can fail in the following scenarios:
/// * If the provided `universe_id` is not a valid ObjectId format.
/// * If the database connection fails to initialize.
/// * If the database query encounters an error.
///
/// # Panics
///
/// This function panics if the `ObjectId` parsing error (`object_id.unwrap()`) is not properly handled.
/// It is recommended to handle the error instead of unwrapping.
///
/// # Example
///
/// ```rust
/// use crate::get_universe_by_id;
///
/// #[tokio::main]
/// async fn main() {
///     let universe_id = "645aef1234567890abcdef12".to_string();
///
///     match get_universe_by_id(universe_id).await {
///         Ok(Some(universe)) => println!("Universe found: {:?}", universe),
///         Ok(None) => println!("No universe found with the given ID."),
///         Err(err) => eprintln!("Error occurred: {}", err),
///     }
/// }
/// ```
///
/// # Notes
///
/// * This function uses a singleton pattern with `DB_CLIENT` to manage the MongoDB connection.
/// * Ensure that `connect_db()`, `RPBOT_DB_NAME`, and `UNIVERSE_COLLECTION_NAME` are properly configured.
/// * The database client and collections should match the expected schema for the `Universe` struct.
pub async fn get_universe_by_id(
    universe_id: ObjectId,
) -> mongodb::error::Result<Option<Universe>> {
    let db_client = get_db_client().await;
    let filter = doc! { "_id": universe_id};
    db_client
        .database(VERSEENGINE_DB_NAME)
        .collection::<Universe>(UNIVERSES_COLLECTION_NAME)
        .find_one(filter)
        .await
}

/// Retrieves a `Universe` document by its associated `server_id` from a MongoDB database.
///
/// The function performs the following steps:
/// 1. Establishes a connection to the database if it hasn't already been initialized.
/// 2. Builds an aggregation pipeline that:
///    - Matches the server document with the provided `server_id`.
///    - Performs a `$lookup` operation to join the `Server` collection with the `Universe` collection
///      based on the `universe_id` field.
///    - Uses `$unwind` to flatten the resulting array of joined universe data.
/// 3. Executes the aggregation pipeline and processes the resulting cursor to extract the universe information.
///
/// # Arguments
/// * `server_id` - A `u64` representing the ID of the server.
///
/// # Returns
/// - `Ok(Some(Universe))` if a matching universe is found.
/// - `Ok(None)` if no matching server or universe is found.
/// - Returns an error (`mongodb::error::Result`) for any database or deserialization issues.
///
/// # Errors
/// This function may return an error if:
/// - Connection to the database fails.
/// - The aggregation process encounters issues.
/// - Document deserialization fails due to schema mismatches.
///
/// # MongoDB Collections
/// - `SERVER_COLLECTION_NAME`: The collection containing server documents.
/// - `UNIVERSE_COLLECTION_NAME`: The collection containing universe documents.
///
/// # Example
/// ```rust
/// let server_id = 123456789;
/// match get_universe_by_server_id(server_id).await {
///     Ok(Some(universe)) => println!("Universe found: {:?}", universe),
///     Ok(None) => println!("No universe found for the provided server ID."),
///     Err(e) => eprintln!("Error occurred: {:?}", e),
/// }
/// ```
///
/// # Dependencies
/// This function depends on the following:
/// - `mongodb` crate for database interaction.
/// - `futures` for asynchronous streams and utilities.
/// - A `Server` document schema and `Universe` document schema.
/// - Helper functions:
///   - `connect_db()`: For initializing the database client.
///   - `from_document()`: For deserializing documents into Rust data structures.
///
/// # Notes
/// - The `DB_CLIENT` is assumed to be a globally available static reference to the MongoDB client initialized using `tokio::sync::OnceCell`.
/// - Ensure the `UNIVERSE_COLLECTION_NAME` and `RPBOT_DB_NAME` constants are configured correctly to match the database schema.
pub async fn get_universe_by_server_id(
    server_id: u64,
) -> mongodb::error::Result<Option<Universe>> {
    let db_client = get_db_client().await;

    let pipeline = vec![
        doc! { "$match": { "server_id": server_id.to_string() } },
        doc! { "$lookup": {
            "from": UNIVERSES_COLLECTION_NAME,   // the UNIVERSE collection
            "localField": "universe_id",        // field in SERVER
            "foreignField": "_id",              // field in UNIVERSE
            "as": "universe"
        }},
        doc! { "$unwind": "$universe" }         // flatten the array
    ];

    let mut cursor = db_client
        .database(VERSEENGINE_DB_NAME)
        .collection::<Server>(SERVERS_COLLECTION_NAME)
        .aggregate(pipeline)
        .await?;


    if let Some(doc) = cursor.try_next().await? {
        // Extract the joined universe document
        let universe_doc = doc.get_document("universe").unwrap();
        let universe: Universe = from_document(universe_doc.clone())?;
        return Ok(Some(universe));
    }

    Ok(None)
}
#[cfg(test)]
mod test {
    use crate::database::db_client::{get_db_client};
    use crate::database::db_namespace::{VERSEENGINE_DB_NAME, UNIVERSES_COLLECTION_NAME};
    use crate::database::universe::{get_universe_by_id, get_universe_by_server_id, Universe};
    use mongodb::bson::doc;
    use mongodb::results::{DeleteResult, InsertOneResult};
    use std::time::SystemTime;

    static SERVER_ID: u64 = 1;

    async fn insert_universe() -> Result<InsertOneResult, String> {
        let _ = get_db_client().await;
        let universe = Universe {
            universe_id: Default::default(),
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

    async fn delete_previously_setup() -> DeleteResult {
        let db_client = get_db_client().await;
        let filter = doc! { "server_ids": {"$in": [SERVER_ID.to_string()] } };
        db_client
            .database(VERSEENGINE_DB_NAME)
            .collection::<Universe>(UNIVERSES_COLLECTION_NAME)
            .delete_many(filter)
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn test_create_universe() {
        let insertion_result = insert_universe().await;
        match insertion_result {
            Ok(_) => {
                assert!(true)
            }
            Err(e) => {
                println!("{}", e);
                assert!(false)
            }
        }
        delete_previously_setup().await;
    }

    #[tokio::test]
    async fn test_delete_previously_setup() {
        let _ = insert_universe().await;
        let result = delete_previously_setup().await;
        assert_ne!(result.deleted_count, 0);
    }

    #[tokio::test]
    async fn test_recover_universe_data() {
        let _ = insert_universe().await;
        let result = get_universe_by_server_id(1).await;
        delete_previously_setup().await;
        match result {
            Ok(data) => {
                match data{
                    None => {assert!(false, "no universe found")}
                    Some(universe_data) => {println!("{:?}", universe_data)}
                }
            }
            Err(_) => {
                assert!(false, "get data failed")
            }
        }
    }

    /// Tests that universes can be retrieved by their creator ID.
    #[tokio::test]
    async fn test_recover_universe_by_creator_id() {
        let _ = insert_universe().await;
        let result = Universe::get_creator_universes(0).await;
        delete_previously_setup().await;
        if result.is_empty(){
            println!("no universes found");
            assert!(false)
        }
        println!("{:?}", result)
    }

    /// Tests that a universe can be retrieved by its ObjectId.
    #[tokio::test]
    async fn test_recover_universe_by_id() {
        let universe = insert_universe().await;
        let id = universe
            .unwrap()
            .inserted_id
            .as_object_id()
            .unwrap();
        let result = get_universe_by_id(id).await;
        println!("{:?}", result);
        delete_previously_setup().await;
        match result {
            Ok(data) => {
                let universe_data = data.unwrap();
                println!("{:?}", universe_data)
            }
            Err(_) => {
                assert!(false, "get data failed")
            }
        }
    }

    #[tokio::test]
    async fn test_recover_unexisting_universe_by_id() {
        let _ = insert_universe().await;
        let result = Universe::get_creator_universes(1).await;
        if !result.is_empty(){
            println!("universes found {:?}", result);
            assert!(false)
        }
        delete_previously_setup().await;
    }
}
