use fluent::FluentArgs;
use crate::craft::service::{self, RecipeRef, ToolSpot};
use crate::discord::poise_structs::{Context, Error};
use crate::utility::reply::reply_with_args_and_ephemeral;

// Toute la logique (outils, ingrédients, exclusion avec le voyage et le farm) est dans
// `crate::craft::service`, partagée avec l'activité.
/// Fabrique un objet à partir d'une recette.
#[poise::command(slash_command, guild_only, rename = "recipe_craft")]
pub async fn craft(
    ctx: Context<'_>,
    recipe_name: String,
    #[min = 1]
    #[max = 999]
    quantity: Option<u64>,
) -> Result<(), Error> {
    let guild_id = ctx.guild_id().ok_or("guild_only")?;
    let quantity = quantity.unwrap_or(1);
    let http = ctx.serenity_context().http.clone();
    let started = service::start(&http, guild_id.get(), ctx.author().id.get(), RecipeRef::Name(&recipe_name), quantity, ToolSpot::Channel(ctx.channel_id().get())).await?;

    let mut args = FluentArgs::new();
    args.set("recipe_name", started.recipe_name);
    args.set("quantity", started.quantity);
    if started.delay > 0 {
        args.set("delay", started.delay);
        reply_with_args_and_ephemeral(ctx, Ok("recipe__craft_started"), Some(args), true).await?;
    } else {
        reply_with_args_and_ephemeral(ctx, Ok("recipe__craft_success"), Some(args), true).await?;
    }
    Ok(())
}
