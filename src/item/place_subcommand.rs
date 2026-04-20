use mongodb::bson::oid::ObjectId;
use crate::database::characters::get_character_by_user_id;
use crate::database::inventory::{Inventory, HolderType};
use crate::database::items::get_item_by_id;
use crate::database::places::get_place_by_category_id;
use crate::database::server::get_server_by_id;
use crate::database::tool::Tool;
use crate::discord::poise_structs::{Context, Error};
use crate::item::ItemUsage;

use fluent::FluentArgs;
use crate::utility::reply::{reply, reply_with_args_and_ephemeral};

/// Place un objet dans le salon actuel.
#[poise::command(slash_command, guild_only, rename = "item_place")]
pub async fn item_place(
    ctx: Context<'_>,
    inventory_id: String,
    immutable: Option<bool>,
) -> Result<(), Error> {
    match _item_place(ctx, inventory_id, immutable).await{
        Ok((res, args, rp_args)) => {
            reply_with_args_and_ephemeral(ctx, Ok(res), Some(args), true).await?;
            let rp_msg = crate::translation::get(ctx, "item_placed_rp", None, Some(&rp_args));
            ctx.channel_id().say(&ctx, rp_msg).await?;
        }
        Err(err) => {reply(ctx, Err(err)).await?;}
    }

    Ok(())
}

/// Place un objet dans le salon actuel.
pub async fn _item_place(
    ctx: Context<'_>,
    inventory_id: String,
    immutable: Option<bool>,
) -> Result<(&str, FluentArgs<'_>, FluentArgs<'_>), Error> {
    let oid = ObjectId::parse_str(&inventory_id).map_err(|_| "item__invalid_id")?;
    
    let server = get_server_by_id(ctx.guild_id().unwrap().get())
        .await?
        .ok_or("item__server_not_found")?;
        
    let character = get_character_by_user_id(server.universe_id, ctx.author().id.get())
        .await?
        .ok_or("loot_table__character_not_found")?;
        
    let inventory_entry = Inventory::get_by_id(oid).await?
        .ok_or("item__not_found_in_inventory")?;
        
    if inventory_entry.holder.holder_id != character._id {
        return Err("item__not_your_item".into());
    }
    
    let item = get_item_by_id(inventory_entry.item_id).await?
        .ok_or("item__not_found")?;
        
    if !matches!(item.item_usage, ItemUsage::Placeable) {
        return Err("item__not_placeable".into());
    }
    
    // Vérifier si on est dans un salon (Place)
    let channel_id = ctx.channel_id();
    let channel = channel_id.to_channel(&ctx).await?.guild().ok_or("item__not_in_guild_channel")?;
    let category_id = channel.parent_id.ok_or("item__not_in_category")?.get();
    
    let _place = get_place_by_category_id(server.universe_id, category_id).await?
        .ok_or("item__not_a_place")?;
    
    let channel_id_val = channel_id.get();
        
    // Gestion de l'owner pour les admins
    let is_admin = ctx.author_member().await.map_or(false, |m| m.permissions.map_or(false, |p| p.administrator()));
    
    let owner_id = if is_admin {
        if immutable.unwrap_or(false) {
            None
        } else {
            Some(character._id)
        }
    } else {
        Some(character._id)
    };
    
    // Créer l'inventaire pour le Tool si nécessaire
    let tool_id = ObjectId::new();
    let tool_inventory_id = if item.inventory_size > 0 {
        Some(Inventory::create_empty_inventory(server.universe_id, HolderType::Item, tool_id).await?)
    } else {
        None
    };
    
    // Créer le Tool
    let tool = Tool {
        _id: Some(tool_id),
        universe_id: server.universe_id,
        server_id: server.server_id,
        owner_id,
        category_id,
        channel_id: channel_id_val,
        original_item: item._id,
        name: item.item_name.clone(),
        chained: None,
        inventory_id: tool_inventory_id,
        inventory_size: item.inventory_size,
    };
    
    // Retirer l'item de l'inventaire du joueur
    if !Inventory::remove_item(oid, 1).await? {
        return Err("item__failed_to_remove".into());
    }
    
    // Sauvegarder le Tool
    tool.save().await?;
    
    let mut args = FluentArgs::new();
    args.set("item_name", item.item_name.clone());
    args.set("channel_name", channel.name.clone());

    let mut rp_args = FluentArgs::new();
    rp_args.set("item_name", item.item_name.clone());
    rp_args.set("character_name", character.name.clone());

    Ok(("item_placed_success", args, rp_args))
}
