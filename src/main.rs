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
mod craft;
mod travel;
mod tip;
mod item;
mod loot_table;
mod loot;
mod recipe;
mod activity;
mod provisioning;
mod music;
#[cfg(test)]
mod test_support;

use discord::poise_structs::{Context, Data, Error};
use crate::database::db_client::{constraint, init_db_client};
use std::process::ExitCode;
use crate::discord::connect_bot::connect_bot;
use dotenv::dotenv;

/// Entry point: connects to MongoDB, makes sure the indexes exist, then starts the Discord bot.
///
/// Any startup failure (missing environment variable, unreachable database, bot failure) is
/// reported on stderr and ends the process with a non-zero exit code, instead of panicking.
#[tokio::main(flavor= "multi_thread")]
async fn main() -> ExitCode {
    dotenv().ok();

    if let Err(e) = init_db_client().await {
        eprintln!("Fatal: cannot initialize the database connection: {e:#}");
        return ExitCode::FAILURE;
    }

    if let Err(e) = constraint().await {
        eprintln!("Fatal: cannot prepare the database: {e:#}");
        return ExitCode::FAILURE;
    }

    if connect_bot().await.is_err() {
        eprintln!("Fatal: the Discord bot could not be started.");
        return ExitCode::FAILURE;
    }

    ExitCode::SUCCESS
}
