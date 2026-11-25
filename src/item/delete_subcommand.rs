//! `/item delete` : la suppression elle-même est dans [`crate::item::service::delete_item`],
//! partagée avec l'éditeur d'objets de l'activité.

use crate::database::server::get_server_by_id;
use crate::discord::poise_structs::{Context, Error};
use crate::item::service::delete_item;
use crate::utility::reply::reply_with_args;
use fluent::FluentArgs;
use crate::tr;
#[poise::command(slash_command, guild_only, required_permissions = "ADMINISTRATOR", rename = "item_delete")]
pub async fn delete(
    ctx: Context<'_>,
    name: String,
) -> Result<(), Error> {
    ctx.defer().await?;
    let guild_id = ctx.guild_id().ok_or("item_delete__not_in_guild")?.get();
    let Some(server) = get_server_by_id(guild_id).await? else {
        return Err("item_delete__server_not_found".into());
    };

    let universe_id = server.universe_id;
    let locale = ctx.locale().unwrap_or("fr");
    let deleted = match delete_item(ctx.serenity_context(), universe_id, &name, locale).await {
        Ok(d) => d,
        Err(e) if e.to_string() == "item_delete__not_found" => {
            let mut args = FluentArgs::new();
            args.set("name", name);
            reply_with_args(ctx, Err("item_delete__not_found".into()), Some(args)).await?;
            return Ok(());
        }
        Err(e) => return Err(e),
    };

    // Succès
    let mut args = FluentArgs::new();
    args.set("name", name.clone());
    if !deleted.affected_recipes.is_empty() {
        args.set("affected_recipes_text", tr!(ctx, "item_delete__affected_recipes", affected_recipes: format!("\n- {}", deleted.affected_recipes.join("\n- "))));
    }
    else {args.set("affected_recipes_text", "")}
    reply_with_args(ctx, Ok("item_delete__success"), Some(args)).await?;

    Ok(())
}
