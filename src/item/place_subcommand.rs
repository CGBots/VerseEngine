use mongodb::bson::oid::ObjectId;
use crate::database::areas::{get_area_by_channel_id, Area};
use crate::database::characters::get_character_by_user_id;
use crate::database::inventory::{Inventory, HolderType};
use crate::database::items::{get_item_by_id};
use crate::database::places::get_place_by_category_id;
use crate::database::universe::get_universe_by_server_id;
use crate::database::tool::Tool;
use crate::discord::poise_structs::{Context, Error};
use crate::item::ItemUsage;
use crate::utility::carousel::{CarouselConfig, CarouselPage, create_carousel_embed, create_carousel_components};
use crate::translation::get_by_locale;
use serenity::all::{CreateInteractionResponse, CreateInteractionResponseMessage, CreateSelectMenu, CreateSelectMenuKind, CreateSelectMenuOption, CreateActionRow, ComponentInteraction};
use std::str::FromStr;

use fluent::FluentArgs;

/// Place un objet dans le salon actuel.
#[poise::command(slash_command, guild_only, rename = "item_place_command")]
pub async fn item_place(
    ctx: Context<'_>,
    immutable: Option<bool>,
) -> Result<(), Error> {
    let guild_id = ctx.guild_id().unwrap();
    let user_id = ctx.author().id;

    let universe = match get_universe_by_server_id(guild_id.get()).await {
        Ok(Some(u)) => u,
        _ => return Err("loot_table__universe_not_found".into()),
    };

    let character = match get_character_by_user_id(universe.universe_id, user_id.get()).await {
        Ok(Some(c)) => c,
        _ => return Err("loot_table__character_not_found".into()),
    };

    let (embed, components) = create_place_carousel_page(
        0,
        universe.universe_id,
        character._id,
        immutable.unwrap_or(false),
        ctx.locale().unwrap_or("fr")
    ).await?;

    ctx.send(poise::CreateReply::default()
        .embed(embed)
        .components(components)
        .ephemeral(true)
    ).await?;

    Ok(())
}

pub async fn create_place_carousel_page(
    page_idx: usize,
    universe_id: ObjectId,
    character_id: ObjectId,
    immutable: bool,
    locale: &str,
) -> Result<(serenity::all::CreateEmbed, Vec<CreateActionRow>), Error> {
    let inventory = Inventory::get_by_holder(universe_id, character_id, HolderType::Character).await?;
    
    let mut placeable_items = Vec::new();
    for inv in inventory {
        if let Ok(Some(item)) = get_item_by_id(inv.item_id).await {
            if matches!(item.item_usage, ItemUsage::Placeable) && inv.quantity > 0 {
                if let Some(inv_id) = inv._id {
                    placeable_items.push((item, inv.quantity, inv_id));
                }
            }
        }
    }

    if placeable_items.is_empty() {
        let embed = serenity::all::CreateEmbed::new()
            .title(get_by_locale(locale, "item_place__title", None, None))
            .description(get_by_locale(locale, "inventory__empty_description", None, None));
        return Ok((embed, vec![]));
    }

    let items_per_page = 5;
    let total_pages = (placeable_items.len() as f64 / items_per_page as f64).ceil() as usize;
    let start_idx = page_idx * items_per_page;
    let end_idx = (start_idx + items_per_page).min(placeable_items.len());
    let page_items = &placeable_items[start_idx..end_idx];

    let mut description = String::new();
    let mut options = Vec::new();

    for (item, qty, inv_id) in page_items {
        description.push_str(&format!("- **{}** (x{}) - ID: `{}`\n", item.item_name, qty, inv_id));
        options.push(CreateSelectMenuOption::new(
            format!("{} (x{})", item.item_name, qty),
            inv_id.to_hex()
        ));
    }

    let page = CarouselPage {
        title: get_by_locale(locale, "item_place__title", None, None),
        description,
        fields: vec![],
        footer: format!("Page {}/{}", page_idx + 1, total_pages),
        color: serenity::all::Colour::BLUE,
    };

    let embed = create_carousel_embed(page);
    let mut components = create_carousel_components(CarouselConfig {
        prefix: "item_place".to_string(),
        current_page: page_idx,
        total_pages,
        metadata: vec![character_id.to_hex(), universe_id.to_hex(), immutable.to_string()],
    }, locale);

    let metadata_str = format!("{}:{}:{}", character_id.to_hex(), universe_id.to_hex(), immutable);
    let select_id = format!("{}:select:{}:{}", "item_place", metadata_str, page_idx);
    let select_menu = CreateSelectMenu::new(select_id, CreateSelectMenuKind::String { options })
        .placeholder(get_by_locale(locale, "item_place__select_placeholder", None, None));
    
    components.push(CreateActionRow::SelectMenu(select_menu));

    Ok((embed, components))
}

pub async fn handle_place_interaction(
    ctx: serenity::all::Context,
    interaction: ComponentInteraction,
    action: &str,
    char_id_hex: &str,
    univ_id_hex: &str,
    immutable_str: &str,
    page: usize,
) -> Result<(), Error> {
    let char_id = ObjectId::from_str(char_id_hex)?;
    let univ_id = ObjectId::from_str(univ_id_hex)?;
    let immutable = immutable_str == "true";
    let locale = interaction.locale.as_str();

    match action {
        "prev" | "next" | "refresh" => {
            let (embed, components) = create_place_carousel_page(page, univ_id, char_id, immutable, locale).await?;
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
                    
                    let result = _apply_place(&ctx, univ_id, char_id, inv_id, interaction.channel_id.get(), immutable).await;
                    
                    let embed = match result {
                        Ok((item_name, channel_name)) => {
                            let mut args = FluentArgs::new();
                            args.set("item_name", item_name.clone());
                            args.set("channel_name", channel_name);
                            
                            let mut rp_args = FluentArgs::new();
                            rp_args.set("item_name", item_name);
                            if let Ok(Some(character)) = crate::database::characters::get_character_by_id(char_id).await {
                                rp_args.set("character_name", character.name);
                            } else {
                                rp_args.set("character_name", interaction.user.name.clone());
                            }
                            
                            let rp_msg = crate::translation::get_by_locale(locale, "item_placed_rp", None, Some(&rp_args));
                            let _ = interaction.channel_id.say(&ctx.http, rp_msg).await;

                            serenity::all::CreateEmbed::new()
                                .title(get_by_locale(locale, "item_place__title", None, None))
                                .description(get_by_locale(locale, "item_placed_success", None, Some(&args)))
                                .color(serenity::all::Color::from_rgb(0, 255, 0))
                        },
                        Err(e) => {
                            serenity::all::CreateEmbed::new()
                                .title(get_by_locale(locale, "item_place__title", None, None))
                                .description(get_by_locale(locale, "item__error", None, None).replace("{}", &e.to_string()))
                                .color(serenity::all::Color::from_rgb(255, 0, 0))
                        }
                    };

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

async fn _apply_place(
    ctx: &serenity::all::Context,
    universe_id: ObjectId,
    character_id: ObjectId,
    inventory_id: ObjectId,
    channel_id_val: u64,
    immutable: bool,
) -> Result<(String, String), Error> {
    let db_client = crate::database::db_client::get_db_client().await;
    let mut session = db_client.start_session().await?;
    session.start_transaction().await?;

    let result = async {
        let inventory_entry = Inventory::get_by_id_with_session(&mut session, inventory_id).await?
            .ok_or("item__not_found_in_inventory")?;

        if inventory_entry.holder.holder_id != character_id {
            return Err("item__not_your_item".into());
        }

        let item = get_item_by_id(inventory_entry.item_id).await?
            .ok_or("item__not_found")?;

        if !matches!(item.item_usage, ItemUsage::Placeable) {
            return Err("item__not_placeable".into());
        }

        let channel_id = serenity::all::ChannelId::new(channel_id_val);
        let channel = channel_id.to_channel(&ctx).await?.guild().ok_or("item__not_in_guild_channel")?;
        let category_id = channel.parent_id.ok_or("item__not_in_category")?.get();

        let _place = get_place_by_category_id(universe_id, category_id).await?
            .ok_or("item__not_a_place")?;

        let area = match get_area_by_channel_id(universe_id, channel_id_val).await? {
            Some(a) => a,
            None => {
                let new_area = Area::new(universe_id, channel_id_val).await;
                new_area.insert_with_session(&mut session).await?;
                new_area
            }
        };

        let owner_id = if immutable {
            None
        } else {
            Some(character_id)
        };

        let tool_id = ObjectId::new();
        let tool_inventory_id = if item.inventory_size > 0 {
            Some(Inventory::create_empty_inventory_with_session(&mut session, universe_id, HolderType::Item, tool_id).await?)
        } else {
            None
        };

        let tool = Tool {
            _id: Some(tool_id),
            universe_id,
            server_id: channel.guild_id.get(),
            owner_id,
            category_id,
            channel_id: channel_id_val,
            area_id: Some(area._id),
            original_item: item._id,
            name: item.item_name.clone(),
            chained: None,
            inventory_id: tool_inventory_id,
            inventory_size: item.inventory_size,
        };

        if !Inventory::remove_item_with_session(&mut session, inventory_id, 1).await? {
            return Err("item__failed_to_remove".into());
        }

        tool.save_with_session(&mut session).await?;

        Ok((item.item_name, channel.name))
    }.await;

    match result {
        Ok(val) => {
            session.commit_transaction().await?;
            Ok(val)
        }
        Err(e) => {
            session.abort_transaction().await?;
            Err(e)
        }
    }
}
