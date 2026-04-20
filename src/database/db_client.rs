#![allow(unused_doc_comments)]
use std::env;
use mongodb::bson::doc;
use mongodb::IndexModel;
use mongodb::options::IndexOptions;
use tokio::sync::OnceCell;
use urlencoding::encode;
use crate::database::characters::Character;
use crate::database::db_namespace::{VERSEENGINE_DB_NAME, SERVERS_COLLECTION_NAME, ITEM_COLLECTION_NAME, STATS_COLLECTION_NAME, CHARACTERS_COLLECTION_NAME};
use crate::database::items::Item;
use crate::database::server::Server;
use crate::database::recipe::{Recipe, RECIPE_COLLECTION_NAME};
use crate::database::stats::Stat;

/// Establishes an asynchronous connection to a MongoDB database.
///
/// This function retrieves the MongoDB user and password from environment variables
/// (`MONGODB_USER` and `MONGODB_PASSWORD`), encodes them for URL safety, and constructs
/// the connection URL. The function then attempts to establish a connection using the
/// provided credentials and returns a `mongodb::Client` instance upon success.
///
/// # Environment Variables
/// - `MONGODB_USER`: The username for authenticating with the MongoDB instance.
/// - `MONGODB_PASSWORD`: The password for authenticating with the MongoDB instance.
///
/// # Returns
/// - `Ok(mongodb::Client)`: An instance of `mongodb::Client` representing the database connection.
/// - `Err(mongodb::error::Error)`: An error occurred while attempting to establish the connection.
///
/// # Panics
/// - The function will panic if the required environment variables (`MONGODB_USER` or `MONGODB_PASSWORD`)
///   are missing or cannot be accessed.
///
/// # Example
/// ```no_run
/// use std::env;
///
/// #[tokio::main]
/// async fn main() {
///     // Set dummy environment variables for demonstration
///     env::set_var("MONGODB_USER", "admin");
///     env::set_var("MONGODB_PASSWORD", "password123");
///
///     match connect_db().await {
///         Ok(client) => println!("Connected to MongoDB successfully!"),
///         Err(e) => eprintln!("Error connecting to MongoDB: {}", e),
///     }
/// }
/// ```
///
/// # Dependencies
/// This function requires the `mongodb` crate and the `percent-encoding` crate
/// for encoding credentials.
///
/// # Note
/// - The MongoDB server is assumed to be running locally (`localhost`) on the default port `27017`.
/// - The connection is authenticated against the `admin` database.
pub async fn connect_db() -> Result<mongodb::Client, mongodb::error::Error>{
    let user = env::var("MONGODB_USER").expect("Expected a database user in the environment");
    let user = encode(user.as_str());
    let password = env::var("MONGODB_PASSWORD").expect("Expected a database password in the environment");
    let password = encode(password.as_str());
    let host = env::var("MONGODB_HOST").unwrap_or_else(|_| "127.0.0.1".to_string());
    let port = env::var("MONGODB_PORT").unwrap_or_else(|_| "27017".to_string());
    let auth_source = env::var("MONGODB_AUTH_SOURCE").unwrap_or_else(|_| "admin".to_string());
    let url = format!("mongodb://{user}:{password}@{host}:{port}/?authSource={auth_source}");
    println!("Connecting to MongoDB at {host}:{port} with authSource={auth_source}...");
    match mongodb::Client::with_uri_str(&url).await {
        Ok(client) => {
            // Vérifier la connexion en listant les collections pour être sûr qu'on y accède vraiment
            if let Err(e) = client.database(VERSEENGINE_DB_NAME).list_collection_names().await {
                eprintln!("Failed to ping MongoDB: {:?}", e);
                return Err(e);
            }
            println!("Connected to MongoDB successfully!");
            Ok(client)
        }
        Err(e) => {
            eprintln!("Failed to create MongoDB client: {:?}", e);
            Err(e)
        }
    }
}

/// Asynchronously sets a unique constraint on the `server_id` field in the specified collection.
///
/// This function is responsible for ensuring data integrity by creating a unique index on the 
/// `server_id` field of the `SERVER_COLLECTION_NAME` collection in the database.
///
/// # Behavior
/// - Establishes a connection to the database using a singleton client `DB_CLIENT`.
/// - Constructs an index with the specified key (`server_id`) and sets it as unique using `IndexOptions`.
/// - Applies the index to the collection within the database.
///
/// # Parameters
/// This function has no parameters. However, it uses the following constants:
/// - `RPBOT_DB_NAME`: The name of the database.
/// - `SERVER_COLLECTION_NAME`: The name of the collection targeted for the index.
///
/// # Returns
/// This is an async function that does not directly return a value. Any errors encountered during
/// the creation of the index (e.g., database connection issues or a failure to apply the index)
/// are propagated as part of the `Result` returned by the async tasks.
///
/// # Examples
/// ```rust
/// // Call the async function to enforce a unique constraint on `server_id`
/// constraint().await;
/// ```
///
/// # Errors
/// An error will occur if:
/// - The database connection fails.
/// - The index creation fails (e.g., permission issues or invalid configurations).
///
/// # Dependencies
/// - Uses the `mongodb` crate for database operations.
/// - Requires the `IndexModel` and `IndexOptions` for constructing the unique index.
///
/// Note: Ensure that the constants `RPBOT_DB_NAME`, `SERVER_COLLECTION_NAME`, and the database client
/// (`DB_CLIENT`) are defined and properly initialized before invoking this function.

pub async fn constraint(){
    let db_client = DB_CLIENT .get_or_init(|| async { connect_db().await.unwrap() }) .await .clone();
    let index_keys = doc! {"server_id": 1};
    let index_options = IndexOptions::builder().unique(true).build();
    let index_model = IndexModel::builder()
        .keys(index_keys)
        .options(index_options.clone())
        .build();
    let _ = db_client
        .database(VERSEENGINE_DB_NAME)
        .collection::<Server>(SERVERS_COLLECTION_NAME)
        .create_index(index_model.clone())
        .await;


    let index_keys = doc! {
        "universe_id": 1,
        "item_name": 1,
    };
    let index_model = IndexModel::builder()
        .keys(index_keys)
        .options(index_options.clone())
        .build();
    let _ = db_client
        .database(VERSEENGINE_DB_NAME)
        .collection::<Item>(ITEM_COLLECTION_NAME)
        .create_index(index_model.clone())
        .await;

    let index_keys = doc! {
        "universe_id": 1,
        "name": 1,
    };
    let index_model = IndexModel::builder()
        .keys(index_keys)
        .options(index_options.clone())
        .build();
    let _ = db_client
        .database(VERSEENGINE_DB_NAME)
        .collection::<Stat>(STATS_COLLECTION_NAME)
        .create_index(index_model.clone())
        .await;

    let index_keys = doc! {
        "universe_id": 1,
        "user_id": 1,
    };
    let index_model = IndexModel::builder()
        .keys(index_keys)
        .options(index_options.clone())
        .build();
    let _ = db_client
        .database(VERSEENGINE_DB_NAME)
        .collection::<Character>(CHARACTERS_COLLECTION_NAME)
        .create_index(index_model.clone())
        .await;

    let index_keys = doc! {
        "universe_id": 1,
        "recipe_name": 1,
    };
    let index_model = IndexModel::builder()
        .keys(index_keys)
        .options(index_options.clone())
        .build();
    let _ = db_client
        .database(VERSEENGINE_DB_NAME)
        .collection::<Recipe>(RECIPE_COLLECTION_NAME)
        .create_index(index_model.clone())
        .await;
}

pub async fn get_db_client() -> mongodb::Client {
    DB_CLIENT.get_or_init(|| async { connect_db().await.expect("Failed to connect to database") }).await.clone()
}

pub static DB_CLIENT: OnceCell<mongodb::Client> = OnceCell::const_new();

#[cfg(test)]
mod test {
    use crate::database::db_client::{DB_CLIENT, connect_db};
    use crate::database::db_namespace::VERSEENGINE_DB_NAME;

    /// Ensures that the database connection initializes correctly.
    #[tokio::test]
    async fn test_connect_db() {
        let client = DB_CLIENT.get_or_init(|| async { connect_db().await.unwrap() }).await;
        match client.database(VERSEENGINE_DB_NAME).list_collection_names().await {
            Ok(_) => {assert!(true)}
            Err(_) => {assert!(false)}
        };
    }
}