use crate::database::items::get_item_by_id;
use crate::database::inventory::Inventory;
use crate::database::characters::get_character_by_user_id;
use crate::database::stats::Stat;
use crate::discord::poise_structs::{Context, Error};
use crate::utility::reply::reply_with_args_and_ephemeral;
use mongodb::bson::oid::ObjectId;
use serenity::all::{CreateEmbed};

/// Affiche les détails d'un item possédé.
#[poise::command(slash_command, dm_only, rename = "lookup")]
pub async fn lookup_subcommand(
    ctx: Context<'_>,
    #[description = "ID de l'entrée d'inventaire"] id: String,
) -> Result<(), Error> {
    let result = _lookup(ctx, id).await;
    if let Err(e) = result {
        reply_with_args_and_ephemeral(ctx, Err(e), None, true).await?;
    }
    Ok(())
}

async fn _lookup(
    ctx: Context<'_>,
    id: String,
) -> Result<(), Error> {
    let oid = ObjectId::parse_str(&id).map_err(|_| "item__invalid_id")?;
    let inventory_entry = Inventory::get_by_id(oid).await?.ok_or("item__not_found_in_inventory")?;

    let character = get_character_by_user_id(inventory_entry.universe_id, ctx.author().id.get())
        .await?
        .ok_or("loot_table__character_not_found")?;

    if inventory_entry.character_id != character._id {
        return Err("item__not_your_item".into());
    }

    let item = get_item_by_id(inventory_entry.item_id).await?.ok_or("item__not_found")?;

    let mut embed = CreateEmbed::new()
        .title(&item.item_name)
        .description(item.description.as_deref().unwrap_or("_Pas de description_"))
        .field("Usage", format!("{:?}", item.item_usage), true);

    if let Some(secret) = &item.secret_informations {
         embed = embed.field("Informations Secrètes", secret, false);
    }

    if !item.effects.is_empty() {
        let mut effects_text = String::new();
        for effect in &item.effects {
            let res = Stat::get_stat_by_id(effect.stat_id).await;
            let stat_name = match res {
                Ok(Some(stat)) => stat.name,
                _ => effect.stat_id.to_string(),
            };
            effects_text.push_str(&format!("- Stat: `{}` | Valeur: `{:?}` | Type: `{:?}`\n", stat_name, effect.value.as_f64(), effect.modifier_type));
        }
        embed = embed.field("Effets", effects_text, false);
    }

    if let Some(image_url) = &item.image {
        embed = embed.image(image_url);
    }

    ctx.send(poise::CreateReply::default()
        .embed(embed)
        .ephemeral(true)
    ).await?;

    Ok(())
}
