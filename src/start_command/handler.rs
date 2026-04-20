use poise::CreateReply;
use serenity::all::{Color, CreateEmbed, CreateEmbedFooter};
use crate::discord::poise_structs::{Context, Error};

/// Starts an administrator-only guild slash command.
///
/// This command sends a message defined by the "start_message" localization key to the channel
/// where the command was invoked.
///
/// # Arguments
/// * `ctx` - The command context providing access to the interaction data, including the guild, channel, and invoking user.
///
/// # Returns
/// * `Result<(), Error>` - Returns `Ok(())` if the command executes successfully, or an `Error` otherwise.
///
/// # Attributes
/// * `#[poise::command]` - Marks this function as a Poise command.
///     - `slash_command` - Indicates this command is a slash command.
///     - `required_permissions = "ADMINISTRATOR"` - Restricts the command to users with administrator permissions.
///     - `guild_only` - Limits the command usage to guilds (servers) and prevents its usage in direct messages.
///
/// # Behavior
/// * Sends a localized response ("start_message") defined by the `tr!` macro.
/// * If the message cannot be sent (`await.unwrap()`), the program panics.
///
/// # Example
/// ```
/// // Example usage of the /start command inside a guild,
/// // assuming the user has administrator permissions:
/// /start
/// ```
#[poise::command(slash_command, rename = "start", required_permissions = "ADMINISTRATOR", guild_only)]
pub async fn start(ctx: Context<'_>) -> Result<(), Error> {
    let _ = ctx.send(
        CreateReply::default().embed(
            CreateEmbed::new()
                .title(crate::translation::get(ctx, "start_message", Some("title"), None))
                .description(crate::translation::get(ctx, "start_message", Some("description"), None))
                .footer(CreateEmbedFooter::new("start_message"))
                .color(Color::from_rgb(0x6f, 0x00, 0xff))
        ),
    ).await;
    Ok(())
}
