#[allow(unused_imports)]
use std::collections::VecDeque;
#[allow(unused_imports)]
use tokio::sync::Mutex as Mut;
#[allow(unused_imports)]
use std::time::Duration;
#[allow(unused_imports)]
use std::sync::{Arc, Mutex};
use std::{env};
use poise::serenity_prelude::ClientBuilder;
use serenity::Client;
use poise::serenity_prelude::GatewayIntents;
use crate::{translation};
use crate::characters::character;
use crate::place::place;
use crate::roads::road;
use crate::discord::handler::Handler;
use crate::ping_command::handler::ping;
use crate::start_command::handler::start;
use crate::discord::poise_structs::Data;
use crate::item::item;
use crate::loot::loot;
use crate::loot_table::loot_table;
use crate::tip::support_command::support_command;
use crate::universe::universe;
use crate::travel::travel__sub_command::travel;

#[cfg(not(test))]
static SHARD_NUMBER: u32 = 1;

#[cfg(test)]
pub(crate) static TEST_PASSED: Mutex<VecDeque<bool>> = Mutex::new(VecDeque::new());

/// Establishes and configures a Discord bot client, initializing the necessary components and
/// handling both environment-specific behavior (e.g., test vs. production) and translations.
///
/// # Returns
///
/// - In production mode (`#[cfg(not(test))]`), the function returns an `Ok(Client)` that is ready 
///   to handle Discord events.
/// - In test mode (`#[cfg(test)]`), the function sleeps briefly and returns an `Err(())`.
///
/// # Steps
/// 1. Initializes logging using `tracing_subscriber`.
/// 2. Prepares a list of commands using `ping()`, `universe()`, and `start()`.
/// 3. Reads and applies translation files to the commands.
/// 4. Retrieves the Discord bot token from the `DISCORD_TOKEN` environment variable.
/// 5. Builds `FrameworkOptions` for the bot framework, registering global commands and setting up app data.
/// 6. Handles two build modes:
///     - **Production**:
///       - Creates a `Client` with the specified token, event handler, intents, and framework.
///       - Starts the Discord client's shards.
///       - Returns the configured client.
///     - **Test**:
///       - Creates and locks a `Client` wrapped in an `Arc<Mutex<>>` for asynchronous use.
///       - Spawns a task to simulate shard handling in testing.
///       - Waits asynchronously for a brief duration using Tokio's sleep.
///       - Returns an error to indicate test-specific flow.
///
/// # Panics
/// - If it fails to read the translation files.
/// - If the `DISCORD_TOKEN` is missing in the environment.
/// - If the framework or client creation fails.
///
/// # Configuration
/// - `GatewayIntents` are configured to include `GUILD_MESSAGES`, `DIRECT_MESSAGES`, and `MESSAGE_CONTENT`.
/// - Translations are applied using the `apply_translations` function with the data read by `read_ftl`.
///
/// # Environment Variables
/// - **DISCORD_TOKEN**: The bot token required to connect to Discord.
///
/// # Framework Options
/// - Commands are registered globally during setup.
///
/// # Platforms
/// - Includes both testing and production configurations under relevant `cfg` attributes.
///
/// # Note
/// - The test mode includes instrumentation to ensure the bot initializes correctly without requiring a connection
///   to an actual Discord gateway.
///
/// # Example
/// ```rust
/// #[tokio::main]
/// async fn main() {
///     if let Err(err) = connect_bot().await {
///         eprintln!("Failed to connect the bot: {:?}", err);
///     }
/// }
/// ```
use crate::recipe::recipe;

pub async fn connect_bot() -> Result<Client, ()>{
    tracing_subscriber::fmt::init();
    
    
    let mut commands= vec![ping(), universe(), start(), place(), road(), character(), travel(), support_command(), item(), loot_table(), loot(), recipe()];
    
    
    let translations = translation::read_ftl().expect("failed to read translation files");
    translation::apply_translations(&translations, &mut commands);
    
    let token = env::var("DISCORD_TOKEN").expect("Expected a token in the environment");
    let intents = GatewayIntents::GUILD_MESSAGES
        | GatewayIntents::DIRECT_MESSAGES
        | GatewayIntents::MESSAGE_CONTENT
        | GatewayIntents::GUILD_MEMBERS;

    let framework = poise::Framework::builder()
        .options(poise::FrameworkOptions {
            commands,
            ..Default::default()
        })
        .setup(|ctx, _ready, framework| {
            Box::pin(async move {
                poise::builtins::register_globally(ctx, &framework.options().commands).await?;
                Ok(Data {translations})
            })
        })
        .build();


    #[cfg(test)]
    #[allow(unused_results)]
    {
        let client = Arc::new(Mut::new(
            ClientBuilder::new(token, intents)
                .framework(framework)
                .event_handler(Handler)
                .await
                .expect("Err creating client"),
        ));

        TEST_PASSED.lock().unwrap().push_back(false);
        println!("start shards");

        let client_clone = Arc::clone(&client);

        tokio::spawn(async move {
            let client = client_clone.lock().await.start_shard(0, 1).await;
            if let Err(why) = client {
                println!("Client error: {why:?}");
            }
        });

        tokio::time::sleep(Duration::from_secs(3)).await; // Use async sleep
        return Err(())
    };

    #[cfg(not(test))]
    {
        let mut client = ClientBuilder::new(token, intents)
                .framework(framework)
                .event_handler(Handler)
                .await
                .expect("Err creating client");
        
        {
            let mut http_client = crate::travel::logic::HTTP_CLIENT.lock().await;
            *http_client = Some(client.http.clone());
        }
        {
            let mut http_client = crate::craft::logic::HTTP_CLIENT.lock().await;
            *http_client = Some(client.http.clone());
        }
        {
            let mut http_client = crate::loot::logic::HTTP_CLIENT.lock().await;
            *http_client = Some(client.http.clone());
        }

        if let Err(why) = client.start_shards(SHARD_NUMBER).await {
            println!("Client error: {why:?}");
        }
        return Ok(client)
    }
}

#[cfg(test)]
mod test {
    use crate::discord::connect_bot::{connect_bot, TEST_PASSED};

    #[tokio::test]
    async fn test_discord_bot_connection(){
        let _ = connect_bot().await;
        assert_eq!(TEST_PASSED.try_lock().unwrap().pop_front().unwrap(), true);
    }
}
