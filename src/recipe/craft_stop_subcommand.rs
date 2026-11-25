use fluent::FluentArgs;
use crate::discord::poise_structs::{Context, Error};
use crate::utility::reply::reply_with_args_and_ephemeral;

// Les exemplaires terminés sont livrés, les ingrédients des autres sont rendus
// (`crate::craft::service::stop`).
/// Arrête le craft en cours.
#[poise::command(slash_command, guild_only, rename = "recipe_stop")]
pub async fn stop(ctx: Context<'_>) -> Result<(), Error> {
    let guild_id = ctx.guild_id().ok_or("guild_only")?;
    match crate::craft::service::stop(guild_id.get(), ctx.author().id.get()).await {
        Ok(stopped) => {
            let mut args = FluentArgs::new();
            args.set("delivered", stopped.delivered);
            args.set("refunded", stopped.refunded);
            reply_with_args_and_ephemeral(ctx, Ok("recipe__craft_stopped"), Some(args), true).await?;
        }
        Err(e) if e.to_string() == "recipe__no_craft_in_progress" => {
            reply_with_args_and_ephemeral(ctx, Ok("recipe__no_craft_in_progress"), None, true).await?;
        }
        Err(e) => return Err(e),
    }
    Ok(())
}
