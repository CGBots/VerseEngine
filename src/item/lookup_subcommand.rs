use crate::database::items::get_item_by_id;
use crate::database::inventory::{Inventory, HolderType};
use crate::database::characters::get_character_by_user_id;
use crate::database::universe::get_universe_by_server_id;
use crate::database::stats::Stat;
use crate::discord::poise_structs::{Context, Error};
use crate::utility::reply::reply_with_args_and_ephemeral;
use crate::utility::carousel::{CarouselConfig, CarouselPage, create_carousel_embed, create_carousel_components};
use crate::translation::get_by_locale;
use mongodb::bson::oid::ObjectId;
use serenity::all::{CreateEmbed, CreateActionRow, ComponentInteraction, CreateInteractionResponse, CreateInteractionResponseMessage, CreateSelectMenu, CreateSelectMenuKind, CreateSelectMenuOption};
use std::str::FromStr;
use poise::ChoiceParameter;

use crate::tr;

/// Affiche les détails d'un item possédé.
#[poise::command(slash_command, guild_only, rename = "item_lookup")]
pub async fn lookup_subcommand(
    ctx: Context<'_>,
    #[description = "L'ID de la ligne d'inventaire"] id: Option<String>,
) -> Result<(), Error> {
    if let Some(id_str) = id {
        let result = _lookup(ctx, id_str).await;
        if let Err(e) = result {
            reply_with_args_and_ephemeral(ctx, Err(e), None, true).await?;
        }
        return Ok(());
    }

    let guild_id = ctx.guild_id().unwrap();
    let user_id = ctx.author().id;

    let universe = match get_universe_by_server_id(guild_id.get()).await {
        Ok(Some(u)) => u,
        _ => return Err("item__universe_not_found".into()),
    };

    let character = match get_character_by_user_id(universe.universe_id, user_id.get()).await {
        Ok(Some(c)) => c,
        _ => return Err("item__character_not_found".into()),
    };

    let (embed, components) = create_lookup_carousel_page(
        0,
        universe.universe_id,
        character._id,
        ctx.locale().unwrap_or("fr")
    ).await?;

    ctx.send(poise::CreateReply::default()
        .embed(embed)
        .components(components)
        .ephemeral(true)
    ).await?;

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

    if inventory_entry.holder.holder_id != character._id {
        return Err("item__not_your_item".into());
    }

    let item = get_item_by_id(inventory_entry.item_id).await?.ok_or("item__not_found")?;

    let embed = create_item_detail_embed(ctx.locale().unwrap_or("fr"), &item).await;

    ctx.send(poise::CreateReply::default()
        .embed(embed)
        .ephemeral(true)
    ).await?;

    Ok(())
}

pub async fn create_item_detail_embed(locale: &str, item: &crate::database::items::Item) -> CreateEmbed {
    let mut embed = CreateEmbed::new()
        .title(&item.item_name)
        .description(item.description.as_deref().unwrap_or(&get_by_locale(locale, "item_no_description", None, None)))
        .field(get_by_locale(locale, "item_lookup_usage", None, None), get_by_locale(locale, item.item_usage.name(), None, None), true);

    if let Some(secret) = &item.secret_informations {
         embed = embed.field(get_by_locale(locale, "item_lookup_secret", None, None), secret, false);
    }

    if !item.effects.is_empty() {
        let mut effects_text = String::new();
        for effect in &item.effects {
            let res = Stat::get_stat_by_id(effect.stat_id).await;
            let stat_name = match res {
                Ok(Some(stat)) => stat.name,
                _ => effect.stat_id.to_string(),
            };
            effects_text.push_str(&format!("- {}: `{}` | {}: `{:?}` | {}: `{:?}`\n", 
                get_by_locale(locale, "item_lookup_stat", None, None), stat_name, 
                get_by_locale(locale, "item_lookup_value", None, None), effect.value.as_f64(), 
                get_by_locale(locale, "item_lookup_type", None, None), effect.modifier_type));
        }
        embed = embed.field(get_by_locale(locale, "item_lookup_effects", None, None), effects_text, false);
    }

    if let Some(image_url) = &item.image {
        embed = embed.image(image_url);
    }

    embed
}

pub async fn create_lookup_carousel_page(
    page_idx: usize,
    universe_id: ObjectId,
    character_id: ObjectId,
    locale: &str,
) -> Result<(serenity::all::CreateEmbed, Vec<CreateActionRow>), Error> {
    let inventory = Inventory::get_by_holder(universe_id, character_id, HolderType::Character).await?;
    
    if inventory.is_empty() {
        let embed = serenity::all::CreateEmbed::new()
            .title(get_by_locale(locale, "item_lookup__title", None, None))
            .description(get_by_locale(locale, "item_lookup__empty_inventory", None, None));
        return Ok((embed, vec![]));
    }

    let items_per_page = 5;
    let total_pages = (inventory.len() as f64 / items_per_page as f64).ceil() as usize;
    let start_idx = page_idx * items_per_page;
    let end_idx = (start_idx + items_per_page).min(inventory.len());
    let page_inventory = &inventory[start_idx..end_idx];

    let mut description = String::new();
    let mut options = Vec::new();

    for inv in page_inventory {
        if let Ok(Some(item)) = get_item_by_id(inv.item_id).await {
            let inv_id = inv._id.unwrap_or_else(ObjectId::new);
            description.push_str(&format!("**{}** (x{}) - ID: `{}`\n", item.item_name, inv.quantity, inv_id));
            options.push(CreateSelectMenuOption::new(
                format!("{} (x{})", item.item_name, inv.quantity),
                inv_id.to_hex()
            ));
        }
    }

    let page = CarouselPage {
        title: get_by_locale(locale, "item_lookup__title", None, None),
        description,
        fields: vec![],
        footer: format!("Page {}/{}", page_idx + 1, total_pages),
        color: serenity::all::Colour::BLUE,
    };

    let embed = create_carousel_embed(page);
    let mut components = create_carousel_components(CarouselConfig {
        prefix: "item_lookup".to_string(),
        current_page: page_idx,
        total_pages,
        metadata: vec![character_id.to_hex(), universe_id.to_hex()],
    }, locale);

    let metadata_str = format!("{}:{}", character_id.to_hex(), universe_id.to_hex());
    let select_id = format!("{}:select:{}:{}", "item_lookup", metadata_str, page_idx);
    let select_menu = CreateSelectMenu::new(select_id, CreateSelectMenuKind::String { options })
        .placeholder(get_by_locale(locale, "item_lookup__select_placeholder", None, None));
    
    components.push(CreateActionRow::SelectMenu(select_menu));

    Ok((embed, components))
}

pub async fn handle_lookup_interaction(
    ctx: serenity::all::Context,
    interaction: ComponentInteraction,
    action: &str,
    char_id_hex: &str,
    univ_id_hex: &str,
    page: usize,
) -> Result<(), Error> {
    let char_id = ObjectId::from_str(char_id_hex)?;
    let univ_id = ObjectId::from_str(univ_id_hex)?;
    let locale = interaction.locale.as_str();

    match action {
        "prev" | "next" | "refresh" => {
            let (embed, components) = create_lookup_carousel_page(page, univ_id, char_id, locale).await?;
            interaction.create_response(&ctx.http, CreateInteractionResponse::UpdateMessage(
                CreateInteractionResponseMessage::new()
                    .embed(embed)
                    .components(components)
            )).await?;
        }
        "select" => {
            if let serenity::all::ComponentInteractionDataKind::StringSelect { values } = &interaction.data.kind {
                if let Some(inv_id_hex) = values.first() {
                    let inv_id = ObjectId::from_str(inv_id_hex)?;
                    let inventory_entry = Inventory::get_by_id(inv_id).await?.ok_or("item__not_found_in_inventory")?;
                    let item = get_item_by_id(inventory_entry.item_id).await?.ok_or("item__not_found")?;

                    let embed = create_item_detail_embed(locale, &item).await;

                    interaction.create_response(&ctx.http, CreateInteractionResponse::UpdateMessage(
                        CreateInteractionResponseMessage::new()
                            .embed(embed)
                            .components(vec![])
                    )).await?;
                }
            }
        }
        _ => {}
    }

    Ok(())
}
