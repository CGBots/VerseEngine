use serenity::all::{ChannelId, RoleId, Channel};
use crate::database::loot_tables::{get_loot_table_by_channel_id, LootTable};
use crate::database::places::get_place_by_category_id;
use crate::database::road::get_road_by_channel_id;
use crate::database::server::get_server_by_id;
use crate::discord::poise_structs::{Context, Error};
use crate::loot_table::execute_loot_table_modal;
use crate::utility::loot_table_parser::LootTableParser;
use crate::utility::reply::reply;
use futures::TryStreamExt;

#[poise::command(slash_command, guild_only, rename = "loot_table_edit")]
pub async fn edit(
    ctx: Context<'_>,
    channel_id: ChannelId,
    #[description = "Temps de recharge en secondes entre deux loots"] rate_limit: Option<u64>,
    #[description = "Délai en secondes pour obtenir le loot"] delay: Option<u64>,
) -> Result<(), Error> {
    let guild_id = ctx.guild_id().unwrap();
    let server = get_server_by_id(guild_id.get()).await?.ok_or("loot_table__server_not_found")?;

    let member = ctx.author_member().await.ok_or("loot_table__no_permission")?;
    let is_admin = member.permissions.map_or(false, |p| p.administrator());
    let is_mod = server.moderator_role_id.map_or(false, |role_id| member.roles.contains(&RoleId::new(role_id.id)));

    if !is_admin && !is_mod {
        return Err("loot_table__no_permission".into());
    }

    let channel_id_u64 = channel_id.get();

    // Déterminer le type de cible: lieu (catégorie), route (canal) ou salon d'un lieu
    let mut is_valid_target = false;

    // 1. Est-ce une catégorie de lieu ?
    if get_place_by_category_id(server.universe_id, channel_id_u64).await?.is_some() {
        is_valid_target = true;
    }

    // 2. Est-ce un canal de route ?
    else if get_road_by_channel_id(server.universe_id, channel_id_u64).await?.is_some() {
        is_valid_target = true;
    }

    // 3. Est-ce un salon dans une catégorie de lieu ?
    else {
        if let Ok(Channel::Guild(guild_channel)) = ctx.serenity_context().http.get_channel(channel_id).await {
            if let Some(parent_id) = guild_channel.parent_id {
                if get_place_by_category_id(server.universe_id, parent_id.get()).await?.is_some() {
                    is_valid_target = true;
                }
            }
        }
    }

    if !is_valid_target {
        return Err("loot_table__target_not_found".into());
    }

    let existing_lt = get_loot_table_by_channel_id(server.universe_id, channel_id_u64).await?;
    let default_content = existing_lt.as_ref().map(|lt| lt.raw_text.clone()).unwrap_or_default();
    let last_loot = existing_lt.as_ref().and_then(|lt| lt.last_loot.clone());
    let existing_delay = existing_lt.as_ref().and_then(|lt| lt.delay);
    let existing_rate_limit = existing_lt.as_ref().and_then(|lt| lt.rate_limit);

    let delay = delay.or(existing_delay);
    let rate_limit = rate_limit.or(existing_rate_limit);

    let modal_result = match ctx {
        poise::Context::Application(app_ctx) => {
            execute_loot_table_modal(app_ctx, default_content).await?
        }
        _ => return Err("loot_table__slash_only".into()),
    };

    if let Some(modal_data) = modal_result {
        let entries = LootTableParser::parse(&modal_data.content, server.universe_id).await;
        match entries {
            Ok(entries) => {
                // Save loot_table table
                let loot_table = LootTable {
                    _id: None,
                    universe_id: server.universe_id,
                    channel_id: channel_id_u64,
                    entries: entries.clone(),
                    raw_text: modal_data.content,
                    rate_limit,
                    delay,
                    last_loot,
                };
                loot_table.save_or_update().await?;

                let _ = reply(ctx, Ok("loot_table__success")).await;
            }
            Err(e) => {
                if e.starts_with("loot_table__invalid_min_max:") {
                    let parts: Vec<&str> = e.strip_prefix("loot_table__invalid_min_max:").unwrap().split('|').collect();
                    if parts.len() == 2 {
                        let mut args = fluent::FluentArgs::new();
                        args.set("min", parts[0].to_string());
                        args.set("max", parts[1].to_string());
                        let _ = crate::utility::reply::reply_with_args(ctx, Err("loot_table__invalid_min_max".into()), Some(args)).await;
                        return Ok(());
                    }
                } else if e.starts_with("loot_table__invalid_item_name:") {
                    if let Some(name) = e.strip_prefix("loot_table__invalid_item_name:") {
                        let mut args = fluent::FluentArgs::new();
                        args.set("name", name.to_string());
                        let _ = crate::utility::reply::reply_with_args(ctx, Err("loot_table__invalid_item_name".into()), Some(args)).await;
                        return Ok(());
                    }
                }
                let _ = reply(ctx, Err(e.into())).await;
            }
        }
    }

    Ok(())
}
