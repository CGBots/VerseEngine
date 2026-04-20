use poise::{CreateReply};
use serenity::all::{ButtonStyle, Color, ComponentInteractionCollector, CreateActionRow, CreateButton, CreateEmbed};
use crate::database::server::{get_server_by_id};
use crate::discord::poise_structs::{Context, Error};
use crate::tr;
use crate::universe::setup::full_setup::full_setup;
use crate::universe::setup::partial_setup::partial_setup;
use crate::utility::reply::reply;

///  * Enum representing the type of setup to be performed.
///  *
///  * This enum is derived with `Debug`, `poise::ChoiceParameter`, `Clone`, and `Copy` traits,
///  * enabling its use in various contexts such as debugging, dropdown choices in commands
///  * (when using the `poise` framework), and shallow copying.
///  *
///  * Variants:
///  * - `FullSetup`: Represents a complete setup process.
///  * - `PartialSetup`: Represents a partial or incomplete setup process.

#[derive(Debug, poise::ChoiceParameter, Clone, Copy)]
pub enum SetupType {
    FullSetup,
    PartialSetup
}

/// Sets up the bot or configuration based on the provided setup type.
///
/// # Parameters
/// - `ctx`: The command context, providing access to interaction details, bot state, and more.
/// - `setup_type`: The type of setup to perform, specified by the `SetupType` enum.
///
/// # Returns
/// - `Result<(), Error>`: Returns `Ok(())` if the setup process completes successfully, or an `Error` if it fails.
///
/// # Command Attributes
/// - `slash_command`: This function is executable as a slash command.
/// - `required_permissions = "ADMINISTRATOR"`: Only users with administrator permissions in the guild can use this command.
/// - `guild_only`: The command can only be invoked in a guild context, not in direct messages.
///
/// # Behavior
/// 1. Defers the response to provide more time for the execution.
/// 2. Delegates the main setup logic to a helper function `_setup`, passing in the context and the setup type.
/// 3. Replies to the user with the result of the setup process.
///
/// # Errors
/// This function may return an error if:
/// - Deferring the interaction fails.
/// - The setup process encounters an issue.
/// - Replying to the user fails.
///
/// # Usage
/// This command is intended to be run by guild administrators to perform initial setup steps or configurations required for the bot's operation.
#[poise::command(slash_command, required_permissions = "ADMINISTRATOR", guild_only, rename = "universe_setup")]
pub async fn setup(
    ctx: Context<'_>,
    setup_type: SetupType
) -> Result<(), Error> {
    let Ok(_) = ctx.defer().await else { return Err("reply__reply_failed".into()) };
    let result = _setup(&ctx, setup_type).await;
    let Ok(_) = reply(ctx, result).await else { return Err("reply__reply_failed".into()) };
    Ok(())
}

/// Asynchronously initializes or reconfigures the server setup process based on the provided setup type.
///
/// # Arguments
/// * `ctx` - The context of the command, which includes the guild and channel information where the command was triggered.
/// * `setup_type` - An enum representing the type of setup to perform. Can be either `FullSetup` or `PartialSetup`.
///
/// # Returns
/// `Result<&'static str, Error>` - Returns a success message if the setup process completes successfully, or an error message if the operation fails.
///
/// # Workflow
/// 1. Retrieves the `guild_id` from the context.
/// 2. Fetches the server from the database by its `guild_id`. If the server is not found, an error is returned.
/// 3. Checks if the server has any existing setup configuration (roles, categories, or channels):
///    - If a configuration exists, prompts the user for confirmation (via interactive buttons) to either cancel or continue the setup:
///      - If the user cancels, the setup process terminates with a cancellation response.
///      - If the user chooses to continue, the process resets the existing setup.
///    - If no configuration exists, proceeds directly to the setup process.
/// 4. Executes either a full or partial setup based on the `setup_type` provided:
///    - `FullSetup`: Performs a comprehensive setup with all components of the server.
///    - `PartialSetup`: Configures only a subset of the server based on specific criteria.
/// 5. Updates the server configuration in the database.
/// 6. Returns a success message if the setup completes successfully, or an error message if an error occurs.
///
/// # Button Interaction Workflow
/// - Users are presented with interactive buttons (`Cancel` and `Continue`) if a configuration is already present:
///   - `Cancel`: Deletes the interactive message and exits the setup process.
///   - `Continue`: Proceeds with the setup while removing the interactive buttons.
///
/// # Timeout Handling
/// - If the user does not interact with the confirmation buttons within 60 seconds, the interactive message is deleted
///   and the process is aborted, returning a timeout error.
///
/// # Errors
/// - `"setup__server_not_found"`: The server was not found in the database.
/// - `"setup__server_already_setup_timeout"`: The user did not respond to the interactive buttons within the timeout period.
/// - `"setup_server__cancelled"`: The user chose to cancel the setup process.
/// - `"setup_server__failed"`: A generic error indicating that the setup process encountered an issue.
///
/// # Example Usage
/// ```rust
/// let result = _setup(ctx, SetupType::FullSetup).await;
/// match result {
///     Ok(message) => println!("{}", message), // Prints "setup_server__success" on success.
///     Err(error) => eprintln!("{}", error),    // Prints error messages like "setup__server_not_found".
/// }
/// ```
pub async fn _setup(ctx: &Context<'_>, setup_type: SetupType) -> Result<&'static str, Error> {
    let guild_id = ctx.guild_id().unwrap();

    let Ok(server_opt) = get_server_by_id(guild_id.get()).await else { return Err("setup__server_not_found".into()) };
    let Some(mut server) = server_opt else { return Err("setup__server_not_found".into()) };
    let server_snapshot = server.clone().snaphot(ctx).await;

    if server.admin_role_id.is_some()
        || server.moderator_role_id.is_some()
        || server.spectator_role_id.is_some()
        || server.player_role_id.is_some()
        || server.road_category_id.is_some()
        || server.rp_wiki_channel_id.is_some()
        || server.admin_category_id.is_some()
        || server.nrp_category_id.is_some()
        || server.rp_category_id.is_some()
        || server.rp_character_channel_id.is_some() {

        let reply = {
            let components = vec![CreateActionRow::Buttons(vec![
                CreateButton::new("cancel")
                    .style(ButtonStyle::Primary)
                    .label(tr!(*ctx, "cancel_setup")),
                CreateButton::new("continue")
                    .style(ButtonStyle::Danger)
                    .label(tr!(*ctx, "continue_setup")),
            ])];

            CreateReply::default()
                .embed(
                    CreateEmbed::new()
                        .color(Color::from_rgb(0xff, 0x98, 0))
                        .title(crate::translation::get(*ctx, &"setup__continue_setup_message", Some("title"), None))
                        .description(crate::translation::get(*ctx, &"setup__continue_setup_message", Some("message"), None))
                )
                .components(components)
        };

        let message = ctx.send(reply.clone()).await.unwrap();

        let serenity_context = ctx.serenity_context();

        let interaction = ComponentInteractionCollector::new(&serenity_context)
            .author_id(ctx.author().id)
            .channel_id(ctx.channel_id())
            .timeout(std::time::Duration::from_secs(60))
            .await;
        match interaction {
            None => {
                let Ok(_) = message.delete(*ctx).await else { return Err("setup_server__failed".into()) };
                return Err("setup__server_already_setup_timeout".into());
            }
            Some(mci) => {
                let Ok(_) = mci.defer(ctx).await else { return Err("setup_server__failed".into()) };
                let Ok(_) = message.edit(*ctx, reply.components(vec![])).await else { return Err("setup_server__failed".into()) };
                match mci.data.custom_id.as_str() {
                    "cancel" => {
                        let Ok(_) = message.delete(*ctx).await else { return Err("setup_server__failed".into()) };
                        return Ok("setup_server__cancelled");
                    }
                    _ => {}
                };
            }
        };
    }

    let result = match setup_type {
        SetupType::FullSetup => { full_setup(ctx, &mut server, server_snapshot).await }
        SetupType::PartialSetup => { partial_setup(ctx, &mut server, server_snapshot).await }
    };

    let Ok(_) = server.update().await else { return Err("setup__server_update_failed".into()) };

    match result {
        Ok(_) => { Ok("setup_server__success") }
        Err(_) => { Err("setup_server__failed".into()) }
    }
}