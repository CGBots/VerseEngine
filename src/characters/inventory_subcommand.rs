use crate::database::characters::get_character_by_user_id;
use crate::database::inventory::Inventory;
use crate::database::items::get_item_by_id;
use crate::database::universe::get_universe_by_server_id;
use crate::discord::poise_structs::{Context, Error};
use crate::utility::reply::reply_with_args_and_ephemeral;

/// Renvoie l'inventaire du personnage sous forme de liste.
#[poise::command(slash_command, guild_only, rename = "inventory")]
pub async fn inventory_subcommand(ctx: Context<'_>) -> Result<(), Error> {
    let guild_id = ctx.guild_id().ok_or("Not in a guild")?.get();
    let Some(universe) = get_universe_by_server_id(guild_id).await? else {return Err("loot_table__universe_not_found".into())};

    let Some(character) = get_character_by_user_id(universe.universe_id, ctx.author().id.get()).await? else {return Err("loot_table__character_not_found".into())};

    let inventory_entries = Inventory::get_by_character_id(universe.universe_id, character._id).await?;

    if inventory_entries.is_empty() {
        ctx.author().direct_message(ctx, serenity::all::CreateMessage::new().content(crate::translation::get(ctx, "inventory__empty", Some("message"), None))).await?;
        return Ok(());
    }

    let mut items_text = Vec::new();
    for entry in inventory_entries {
        if let Some(item) = get_item_by_id(entry.item_id).await? {
            let id_str = entry._id.map(|oid| oid.to_hex()).unwrap_or_else(|| "N/A".to_string());
            items_text.push(format!("- {}x {} (ID: `{}`)", entry.quantity, item.item_name, id_str));
        }
    }

    if items_text.is_empty() {
        ctx.author().direct_message(ctx, serenity::all::CreateMessage::new().content(crate::translation::get(ctx, "inventory__empty", Some("message"), None))).await?;
        return Ok(());
    }

    // Ajout de l'indication pour /lookup
    let footer_msg = format!("\n{}", crate::translation::get(ctx, "inventory__lookup_hint", None, None));
    
    // Gestion de la pagination (max 2000 caractères par message Discord)
    let mut current_message = String::new();

    for item_line in items_text {
        if current_message.len() + item_line.len() + 2 > 1900 {
            ctx.author().direct_message(ctx, serenity::all::CreateMessage::new().content(&current_message)).await?;
            current_message.clear();
        }
        current_message.push_str(&item_line);
        current_message.push('\n');
    }

    if !current_message.is_empty() {
        current_message.push_str(&footer_msg);
        ctx.author().direct_message(ctx, serenity::all::CreateMessage::new().content(&current_message)).await?;
    }

    reply_with_args_and_ephemeral(ctx, Ok("inventory__sent_dm"), None, true).await?;

    Ok(())
}
