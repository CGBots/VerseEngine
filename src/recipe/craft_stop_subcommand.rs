use crate::discord::poise_structs::{Context, Error};
use crate::database::server::get_server_by_id;
use crate::craft::logic::stop_craft;
use crate::utility::reply::reply_with_args;

/// Arrête le craft en cours.
#[poise::command(slash_command, guild_only, rename = "recipe_stop")]
pub async fn stop(ctx: Context<'_>) -> Result<(), Error> {
    let guild_id = ctx.guild_id().unwrap();
    let user_id = ctx.author().id.get();

    let server = get_server_by_id(guild_id.get()).await?.ok_or("recipe__server_not_found")?;

    match stop_craft(server.universe_id, user_id).await {
        Ok(Some(_)) => {
            reply_with_args(ctx, Ok("recipe__craft_stopped"), None).await?;
        }
        _ => {
            reply_with_args(ctx, Ok("recipe__no_craft_in_progress"), None).await?;
        }
    }

    Ok(())
}
