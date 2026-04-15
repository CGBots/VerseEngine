mod ping_command;
mod translation;
mod database;
mod discord;
mod bson_modifiers;
mod start_command;
pub mod universe;
mod utility;
mod place;
mod roads;
mod characters;
mod travel;
mod tip;
mod item;
mod loot_table;
mod loot;
mod recipe;

use discord::poise_structs::{Context, Data, Error};
use crate::database::db_client::constraint;
use crate::discord::connect_bot::connect_bot;
use dotenv::dotenv;

/// The asynchronous entry point of the application.
///
/// This function initializes the database connection and performs two key tasks:
/// 1. It ensures a singleton database client instance (`DB_CLIENT`) is asynchronously initialized using `get_or_init`.
///    - The initialization involves invoking `connect_db()` from the `database::db_client` module, which attempts to establish a connection to the database.
///    - The program will panic with an error message "Failed to connect to database" if the database initialization fails.
/// 2. It calls two asynchronous functions:
///    - `constraint()`
///      - This appears to perform some constraints or precondition checks. Implementation details are contained in the respective function definition.
///    - `connect_bot()`
///      - This function is presumably responsible for establishing a connection to a bot or initializing bot functionality.
///
/// ## Notes
/// - The `#[tokio::main(flavor = "multi_thread")]` attribute indicates that the Tokio runtime is configured with a multi-threaded flavor, allowing concurrent execution of tasks.
/// - The function leverages async/await functionality to handle asynchronous operations.
///
/// ## Panics
/// - Panics if the database connection fails during the asynchronous `connect_db()` call in the initialization of `DB_CLIENT`.
///
/// ## Example
/// This function serves as the main entry point for the application. It sets up the necessary connection to the database and takes care of initializing bot functionality.
///
/// ```rust
/// #[tokio::main(flavor = "multi_thread")]
/// async fn main() {
///     // Initializations for database and bot connection.
/// }
/// ```
#[tokio::main(flavor= "multi_thread")]
async fn main() {
    dotenv().ok();
    let _ = database::db_client::get_db_client().await;

    constraint().await;
    let _ = connect_bot().await;
}