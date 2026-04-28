use crate::database::characters::{get_character_by_user_id};
use crate::database::inventory::{Inventory, HolderType};
use crate::database::universe::get_universe_by_server_id;
use crate::database::items::{get_item_by_id};
use crate::database::modifiers::{ModifierLevel};
use crate::database::places::get_place_by_category_id;
use crate::database::road::get_road_by_channel_id;
use crate::database::craft::PlayerCraft;
use crate::database::loot::PlayerLoot;
use crate::discord::poise_structs::{Context, Error};
use crate::utility::carousel::{CarouselConfig, CarouselPage, create_carousel_embed, create_carousel_components};
use crate::translation::get_by_locale;
use mongodb::bson::oid::ObjectId;
use serenity::all::{CreateInteractionResponse, CreateInteractionResponseMessage, CreateSelectMenu, CreateSelectMenuKind, CreateSelectMenuOption, CreateActionRow, ComponentInteraction};
use crate::item::ItemUsage;
use std::str::FromStr;

#[poise::command(slash_command, guild_only, rename = "consume")]
pub async fn consume(ctx: Context<'_>) -> Result<(), Error> {
    let guild_id = ctx.guild_id().unwrap();
    let user_id = ctx.author().id;

    let universe = match get_universe_by_server_id(guild_id.get()).await {
        Ok(Some(u)) => u,
        _ => return Err("consume__universe_not_found".into()),
    };

    let character = match get_character_by_user_id(universe.universe_id, user_id.get()).await {
        Ok(Some(c)) => c,
        _ => return Err("consume__character_not_found".into()),
    };

    // Vérification des processus longs en cours
    if let Ok(Some(player_move)) = character.clone().get_player_move().await {
        if player_move.is_in_move {
            return Err("consume__busy".into());
        }
    }

    if let Ok(Some(_)) = PlayerCraft::get_by_user_id(universe.universe_id, user_id.get()).await {
        return Err("consume__busy".into());
    }

    if let Ok(Some(_)) = PlayerLoot::get_by_user_id(universe.universe_id, user_id.get()).await {
        return Err("consume__busy".into());
    }

    let (embed, components) = create_consume_carousel_page(
        0,
        universe.universe_id,
        character._id,
        ctx.locale().unwrap_or("en-US")
    ).await?;

    ctx.send(poise::CreateReply::default()
        .embed(embed)
        .components(components)
        .ephemeral(true)
    ).await?;

    Ok(())
}

pub async fn create_consume_carousel_page(
    page_idx: usize,
    universe_id: ObjectId,
    character_id: ObjectId,
    locale: &str,
) -> Result<(serenity::all::CreateEmbed, Vec<CreateActionRow>), Error> {
    let inventory = Inventory::get_by_holder(universe_id, character_id, HolderType::Character).await?;
    
    let mut consumable_items = Vec::new();
    for inv in inventory {
        if let Ok(Some(item)) = get_item_by_id(inv.item_id).await {
            if matches!(item.item_usage, ItemUsage::Consumable) && inv.quantity > 0 {
                consumable_items.push((item, inv.quantity));
            }
        }
    }

    if consumable_items.is_empty() {
        let embed = serenity::all::CreateEmbed::new()
            .title(get_by_locale(locale, "consume__title", None, None))
            .description(get_by_locale(locale, "consume__empty_inventory", None, None));
        return Ok((embed, vec![]));
    }

    let items_per_page = 5;
    let total_pages = (consumable_items.len() as f64 / items_per_page as f64).ceil() as usize;
    let start_idx = page_idx * items_per_page;
    let end_idx = (start_idx + items_per_page).min(consumable_items.len());
    let page_items = &consumable_items[start_idx..end_idx];

    let mut description = String::new();
    let mut options = Vec::new();

    for (item, qty) in page_items {
        description.push_str(&format!("**{}** (x{}) - ID: `{}`\n", item.item_name, qty, item._id));
        options.push(CreateSelectMenuOption::new(
            format!("{} (x{})", item.item_name, qty),
            item._id.to_hex()
        ));
    }

    let page = CarouselPage {
        title: get_by_locale(locale, "consume__title", None, None),
        description,
        fields: vec![],
        footer: format!("Page {}/{}", page_idx + 1, total_pages),
        color: serenity::all::Colour::BLUE,
    };

    let embed = create_carousel_embed(page);
    let mut components = create_carousel_components(CarouselConfig {
        prefix: "item_consume".to_string(),
        current_page: page_idx,
        total_pages,
        metadata: vec![character_id.to_hex(), universe_id.to_hex()],
    }, locale);

    let metadata_str = format!("{}:{}", character_id.to_hex(), universe_id.to_hex());
    let select_id = format!("{}:select:{}:{}", "item_consume", metadata_str, page_idx);
    let select_menu = CreateSelectMenu::new(select_id, CreateSelectMenuKind::String { options })
        .placeholder(get_by_locale(locale, "consume__select_placeholder", None, None));
    
    components.push(CreateActionRow::SelectMenu(select_menu));

    Ok((embed, components))
}

pub async fn handle_consume_interaction(
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
            let (embed, components) = create_consume_carousel_page(page, univ_id, char_id, locale).await?;
            interaction.create_response(&ctx.http, CreateInteractionResponse::UpdateMessage(
                CreateInteractionResponseMessage::new()
                    .embed(embed)
                    .components(components)
            )).await?;
        }
        "select" => {
            if let serenity::all::ComponentInteractionDataKind::StringSelect { values } = &interaction.data.kind {
                if let Some(item_id_hex) = values.first() {
                    let item_id = ObjectId::from_str(item_id_hex)?;
                    
                    // Consume logic
                    let result = apply_consumption(&ctx, univ_id, char_id, item_id, interaction.channel_id.get()).await;
                    
                    let embed = match result {
                        Ok(item_name) => {
                            let mut args = fluent::FluentArgs::new();
                            args.set("item_name", item_name);
                            serenity::all::CreateEmbed::new()
                                .title(get_by_locale(locale, "consume__title", None, None))
                                .description(get_by_locale(locale, "consume__success", None, Some(&args)))
                                .color(serenity::all::Color::from_rgb(0, 255, 0))
                        },
                        Err(e) => {
                            let mut args = fluent::FluentArgs::new();
                            args.set("error", e.to_string());
                            serenity::all::CreateEmbed::new()
                                .title(get_by_locale(locale, "consume__title", None, None))
                                .description(get_by_locale(locale, "consume__error", None, Some(&args)))
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

use crate::database::db_client::get_db_client;

async fn apply_consumption(
    ctx: &serenity::all::Context,
    univ_id: ObjectId,
    char_id: ObjectId,
    item_id: ObjectId,
    channel_id: u64,
) -> Result<String, Error> {
    let item = get_item_by_id(item_id).await?.ok_or("Item not found")?;
    
    // Vérification des processus longs en cours
    let character_info = crate::database::characters::get_character_by_id(char_id).await?
        .ok_or("Character not found")?;

    if let Ok(Some(player_move)) = character_info.clone().get_player_move().await {
        if player_move.is_in_move {
            return Err("consume__busy".into());
        }
    }

    if let Ok(Some(_)) = PlayerCraft::get_by_user_id(univ_id, character_info.user_id).await {
        return Err("consume__busy".into());
    }

    if let Ok(Some(_)) = PlayerLoot::get_by_user_id(univ_id, character_info.user_id).await {
        return Err("consume__busy".into());
    }

    if !matches!(item.item_usage, ItemUsage::Consumable) {
        return Err("Item is not consumable".into());
    }

    // 1. Récupération de tous les objets potentiellement affectés
    let mut character = crate::database::characters::get_character_by_id(char_id).await?
        .ok_or("Character not found")?;
    
    let mut _universe = crate::database::universe::get_universe_by_id(univ_id).await?
        .ok_or("Universe not found")?;

    let mut road = get_road_by_channel_id(univ_id, channel_id).await?;
    let mut area = crate::database::areas::get_area_by_channel_id(univ_id, channel_id).await?;
    let mut place = None;
    if road.is_none() {
        if let Ok(serenity::all::Channel::Guild(channel)) = ctx.http.get_channel(serenity::all::ChannelId::new(channel_id)).await {
            if let Some(category_id) = channel.parent_id {
                place = get_place_by_category_id(univ_id, category_id.get()).await?;
            }
        }
    }

    // 2. Préparation de la transaction
    let client = get_db_client().await;
    let mut session = client.start_session().await?;
    
    session.start_transaction().await?;

    // 3. Retrait de l'item de l'inventaire
    match Inventory::remove_item_from_holder_with_session(&mut session, univ_id, char_id, HolderType::Character, item_id, 1).await {
        Ok(true) => (),
        Ok(false) => {
            session.abort_transaction().await?;
            return Err("Item not found in inventory".into());
        },
        Err(e) => {
            session.abort_transaction().await?;
            return Err(e.into());
        }
    }

    // 4. Application des effets en mémoire
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("Time went backwards")
        .as_secs();

    for modifier in &item.effects {
        let mut mod_to_apply = modifier.clone();
        mod_to_apply.source = item_id;

        if let Some(duration_secs) = mod_to_apply.end_timestamp {
            mod_to_apply.end_timestamp = Some(now + duration_secs);
        }

        match mod_to_apply.level {
            ModifierLevel::Player => {
                if let Some(stat) = character.stats.iter_mut().find(|s| s._id == mod_to_apply.stat_id) {
                    stat.modifiers.push(mod_to_apply);
                }
            },
            ModifierLevel::Place => {
                if let Some(r) = road.as_mut() {
                    r.modifiers.push(mod_to_apply);
                } else if let Some(p) = place.as_mut() {
                    p.modifiers.push(mod_to_apply);
                }
            },
            ModifierLevel::Area => {
                if let Some(a) = area.as_mut() {
                    a.modifiers.push(mod_to_apply);
                } else {
                    let mut new_area = crate::database::areas::Area::new(univ_id, channel_id).await;
                    new_area.modifiers.push(mod_to_apply);
                    area = Some(new_area);
                }
            },
            ModifierLevel::Universe => {
                // Universe n'a pas de champ modifiers actuellement.
            }
        }
    }

    // 5. Sauvegarde des objets modifiés dans la session
    let result = async {
        character.update_with_session(&mut session).await?;

        if let Some(r) = road {
            r.update_with_optional_session(Some(&mut session)).await?;
        }
        
        if let Some(p) = place {
            p.update_with_optional_session(Some(&mut session)).await?;
        }

        if let Some(a) = area {
            a.update_with_session(&mut session).await?;
        }
        Ok::<(), Error>(())
    }.await;

    match result {
        Ok(_) => {
            session.commit_transaction().await?;
            Ok(item.item_name)
        },
        Err(e) => {
            session.abort_transaction().await?;
            Err(e)
        }
    }
}
