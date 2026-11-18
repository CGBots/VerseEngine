use std::env;
use anyhow::Context as _;
use mongodb::bson::{doc, Document};
use mongodb::IndexModel;
use mongodb::options::IndexOptions;
use tokio::sync::OnceCell;
use urlencoding::encode;
use crate::database::db_namespace::{VERSEENGINE_DB_NAME, SERVERS_COLLECTION_NAME, ITEM_COLLECTION_NAME, STATS_COLLECTION_NAME, CHARACTERS_COLLECTION_NAME};
use crate::database::recipe::RECIPE_COLLECTION_NAME;

/// Reads a mandatory environment variable and turns its absence into a readable error
/// (instead of panicking).
fn required_env(name: &str) -> anyhow::Result<String> {
    env::var(name).map_err(|_| anyhow::anyhow!("missing environment variable {name}"))
}

/// Builds the MongoDB connection string from already-read connection parameters.
///
/// The user and the password are percent-encoded so special characters cannot break the URI.
fn build_mongo_uri(user: &str, password: &str, host: &str, port: &str, auth_source: &str) -> String {
    format!(
        "mongodb://{}:{}@{host}:{port}/?authSource={}",
        encode(user),
        encode(password),
        encode(auth_source),
    )
}

/// Opens a connection to MongoDB using the `MONGODB_*` environment variables and checks that the
/// database is really reachable.
///
/// # Environment
/// - `MONGODB_USER` and `MONGODB_PASSWORD` (required)
/// - `MONGODB_HOST` (default `127.0.0.1`), `MONGODB_PORT` (default `27017`),
///   `MONGODB_AUTH_SOURCE` (default `admin`)
///
/// # Errors
/// Returns an error if a required variable is missing, if the client cannot be created or if the
/// database does not answer. The password is never part of the error message.
pub async fn connect_db() -> anyhow::Result<mongodb::Client> {
    let user = required_env("MONGODB_USER")?;
    let password = required_env("MONGODB_PASSWORD")?;
    let host = env::var("MONGODB_HOST").unwrap_or_else(|_| "127.0.0.1".to_string());
    let port = env::var("MONGODB_PORT").unwrap_or_else(|_| "27017".to_string());
    let auth_source = env::var("MONGODB_AUTH_SOURCE").unwrap_or_else(|_| "admin".to_string());
    let url = build_mongo_uri(&user, &password, &host, &port, &auth_source);

    println!("Connecting to MongoDB at {host}:{port} with authSource={auth_source}...");
    let client = mongodb::Client::with_uri_str(&url)
        .await
        .with_context(|| format!("cannot create the MongoDB client for {host}:{port}"))?;

    // Verify the connection by really talking to the server.
    client
        .database(VERSEENGINE_DB_NAME)
        .list_collection_names()
        .await
        .with_context(|| format!("cannot reach MongoDB at {host}:{port} (authSource={auth_source})"))?;

    println!("Connected to MongoDB successfully!");
    Ok(client)
}

/// Initializes the global MongoDB client, or returns the already initialized one.
///
/// To be called once at startup so a connection problem is reported as a normal error
/// (see `main`) rather than as a panic deep inside a command.
pub async fn init_db_client() -> anyhow::Result<mongodb::Client> {
    DB_CLIENT.get_or_try_init(connect_db).await.cloned()
}

/// Creates the unique indexes the data model relies on.
///
/// A failure to create one index (for instance because existing documents already violate it) is
/// reported on stderr but does not stop the bot, as before. Only a failure to reach the database
/// is returned as an error.
pub async fn constraint() -> anyhow::Result<()> {
    let db_client = init_db_client().await?;
    let db = db_client.database(VERSEENGINE_DB_NAME);
    let unique = IndexOptions::builder().unique(true).build();

    let unique_indexes: [(&str, Document); 5] = [
        (SERVERS_COLLECTION_NAME, doc! {"server_id": 1}),
        (ITEM_COLLECTION_NAME, doc! {"universe_id": 1, "item_name": 1}),
        (STATS_COLLECTION_NAME, doc! {"universe_id": 1, "name": 1}),
        (CHARACTERS_COLLECTION_NAME, doc! {"universe_id": 1, "user_id": 1}),
        (RECIPE_COLLECTION_NAME, doc! {"universe_id": 1, "recipe_name": 1}),
    ];

    for (collection, keys) in unique_indexes {
        let model = IndexModel::builder()
            .keys(keys)
            .options(unique.clone())
            .build();
        if let Err(e) = db.collection::<Document>(collection).create_index(model).await {
            eprintln!("Could not create the unique index on `{collection}`: {e:?}");
        }
    }
    Ok(())
}

/// Returns the global MongoDB client.
///
/// `main` calls [`init_db_client`] at startup, so in production this is a plain read. If it is
/// called before (as the tests do), the client is created lazily.
///
/// # Panics
/// Only if the lazy initialization fails, i.e. when the database is unreachable and
/// [`init_db_client`] was never called. Call [`init_db_client`] first to get an error instead.
pub async fn get_db_client() -> mongodb::Client {
    DB_CLIENT
        .get_or_init(|| async {
            connect_db()
                .await
                .expect("MongoDB is not initialized: call init_db_client() at startup")
        })
        .await
        .clone()
}

pub static DB_CLIENT: OnceCell<mongodb::Client> = OnceCell::const_new();

#[cfg(test)]
mod test {
    use super::build_mongo_uri;
    use crate::database::db_client::{DB_CLIENT, connect_db};
    use crate::database::db_namespace::VERSEENGINE_DB_NAME;

    /// Ensures that the database connection initializes correctly.
    /// (Needs a reachable MongoDB configured through the `MONGODB_*` variables.)
    #[tokio::test]
    #[ignore = "integration test: needs a running MongoDB (MONGODB_* env vars); run with --ignored"]
    async fn test_connect_db() {
        let client = DB_CLIENT.get_or_init(|| async { connect_db().await.unwrap() }).await;
        match client.database(VERSEENGINE_DB_NAME).list_collection_names().await {
            Ok(_) => {assert!(true)}
            Err(_) => {assert!(false)}
        };
    }

    #[test]
    fn uri_percent_encodes_credentials() {
        let uri = build_mongo_uri("admin", "p@ss:w/rd", "db", "27017", "admin");
        assert_eq!(uri, "mongodb://admin:p%40ss%3Aw%2Frd@db:27017/?authSource=admin");
    }

    #[test]
    fn uri_uses_given_host_port_and_auth_source() {
        let uri = build_mongo_uri("u", "p", "mongo", "27018", "verse");
        assert_eq!(uri, "mongodb://u:p@mongo:27018/?authSource=verse");
    }

    #[test]
    fn required_env_reports_the_missing_variable() {
        let err = super::required_env("VERSEENGINE_TEST_SURELY_UNSET_VARIABLE").unwrap_err();
        assert!(err.to_string().contains("VERSEENGINE_TEST_SURELY_UNSET_VARIABLE"));
    }
}
