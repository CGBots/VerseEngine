use crate::database::characters::get_character_by_user_id;
use crate::database::inventory::{Inventory, HolderType};
use crate::database::tool::Tool;
use crate::database::universe::get_universe_by_server_id;
use crate::database::db_client::get_db_client;
use crate::database::items::{get_item_by_name, get_item_by_id};
use crate::discord::poise_structs::{Context, Error, Data};
use crate::utility::carousel::{CarouselConfig, CarouselPage, create_carousel_embed, create_carousel_components};
use crate::translation::{get, get_by_locale};
use fluent::FluentArgs;
use mongodb::bson::oid::ObjectId;
use serenity::all::{CreateInteractionResponse};
use serenity::json::json;
use std::str::FromStr;

#[poise::command(slash_command, guild_only, rename = "item_use")]
pub async fn item_use(
    ctx: Context<'_>,
    tool_id: Option<String>,
) -> Result<(), Error> {
    let guild_id = ctx.guild_id().unwrap();
    let user_id = ctx.author().id;

    let universe = match get_universe_by_server_id(guild_id.get()).await {
        Ok(Some(u)) => u,
        _ => return Err("use__universe_not_found".into()),
    };

    let character = match get_character_by_user_id(universe.universe_id, user_id.get()).await {
        Ok(Some(c)) => c,
        _ => return Err("use__character_not_found".into()),
    };

    let tool_id = match tool_id {
        Some(id) => match ObjectId::from_str(&id) {
            Ok(oid) => Some(oid),
            Err(_) => return Err("use__invalid_tool_id".into()),
        },
        None => None,
    };

    match tool_id {
        None => {
            // List tools in the current channel
            let channel_id = ctx.channel_id();
            let (embed, components) = create_tool_selection_page(
                0,
                universe.universe_id,
                channel_id.get(),
                ctx.locale().unwrap_or("fr")
            ).await?;

            ctx.send(poise::CreateReply::default()
                .embed(embed)
                .components(components)
                .ephemeral(true)
            ).await?;
        }
        Some(id) => {
            let tool = match Tool::get_by_id(id).await? {
                Some(t) => t,
                None => return Err("use__tool_not_found".into()),
            };

            if tool.inventory_size == 0 {
                return Err("use__no_inventory".into());
            }

            // Open Modal
            if let poise::Context::Application(app_ctx) = ctx {
                execute_use_modal(app_ctx, tool, character._id).await?;
            } else {
                return Err("use__only_slash_command".into());
            }
        }
    }

    Ok(())
}

pub async fn execute_use_modal(
    ctx: poise::ApplicationContext<'_, Data, Error>,
    tool: Tool,
    character_id: ObjectId,
) -> Result<(), Error> {
    let universe_id = tool.universe_id;
    
    // Get tool inventory content
    let tool_inventory = Inventory::get_by_holder(universe_id, tool._id.unwrap(), HolderType::Item).await?;
    let mut current_size: u64 = 0;
    let mut items_list = String::new();
    
    for inv in &tool_inventory {
        if let Ok(Some(item)) = get_item_by_id(inv.item_id).await {
            items_list.push_str(&format!("- {} {}\n", item.item_name, inv.quantity));
            current_size += inv.quantity;
        }
    }

    let tool_header = format!("### {} ({} / {})\n", tool.name, current_size, tool.inventory_size);
    let mut tool_content = tool_header.clone();
    
    if items_list.is_empty() {
        tool_content.push_str(&get(poise::Context::Application(ctx), "use__empty_inventory", None, None));
    } else {
        tool_content.push_str(&items_list);
    }

    // Get character inventory content
    let character_inventory_items = Inventory::get_by_holder(universe_id, character_id, HolderType::Character).await?;
    let char_inv_label = get(poise::Context::Application(ctx), "use__modal_character_inventory_label", None, None);
    let char_header = format!("### {}\n", char_inv_label);
    let mut character_inventory = char_header.clone();
    
    for inv in &character_inventory_items {
        if let Ok(Some(item)) = get_item_by_id(inv.item_id).await {
            character_inventory.push_str(&format!("- {} {}\n", item.item_name, inv.quantity));
        }
    }
    
    if character_inventory == char_header {
        character_inventory.push_str(&get(poise::Context::Application(ctx), "use__empty_inventory", None, None));
    }

    let title = tool.name.clone();
    let label = get(poise::Context::Application(ctx), "use__modal_label", None, None);
    let _chest_inventory_label = get(poise::Context::Application(ctx), "use__modal_chest_inventory_label", None, None);
    let _character_inventory_label = get(poise::Context::Application(ctx), "use__modal_character_inventory_label", None, None);
    let _instructions_label = get(poise::Context::Application(ctx), "use__modal_instructions_label", None, None);
    let instructions_value = get(poise::Context::Application(ctx), "use__modal_instructions_value", None, None);
    
    let custom_id = format!("{}-{}", ctx.interaction.id, "use_modal");

    let modal_json = json!({
        "type": 9,
        "data": {
            "custom_id": custom_id,
            "title": title,
            "components": [
                {
                    "type": 1,
                    "components": [
                        {
                            "type": 4,
                            "custom_id": "content",
                            "label": label,
                            "style": 2,
                            "placeholder": "> [item_name] [quantité]\n< [item_name] [quantité]",
                            "required": false
                        }
                    ]
                },
                {
                    "type": 10,
                    "content": tool_content,
                },
                {
                    "type": 10,
                    "content": character_inventory,
                },
                {
                    "type": 10,
                    "content": instructions_value,
                }
            ]
        }
    });

    ctx.serenity_context.http.create_interaction_response(ctx.interaction.id, &ctx.interaction.token, &modal_json, vec![]).await?;
    ctx.has_sent_initial_response.store(true, std::sync::atomic::Ordering::SeqCst);

    let response = serenity::collector::ModalInteractionCollector::new(ctx.serenity_context)
        .filter(move |m| m.data.custom_id == custom_id)
        .timeout(std::time::Duration::from_secs(600))
        .await;

    if let Some(m) = response {
        let content = m.data.components.iter()
            .flat_map(|row| row.components.iter())
            .find_map(|component| {
                if let serenity::all::ActionRowComponent::InputText(it) = component {
                    if it.custom_id == "content" {
                        return Some(it.value.clone().unwrap_or_default());
                    }
                }
                None
            })
            .unwrap_or_default();

        m.create_response(ctx.serenity_context, CreateInteractionResponse::Acknowledge).await?;

        // Process the content
        let result = process_use_with_transaction(ctx, universe_id, character_id, tool.clone(), content.clone()).await;
        
        if let Err(e) = result {
            ctx.interaction.create_followup(ctx.serenity_context, 
                serenity::all::CreateInteractionResponseFollowup::new()
                    .content(format!("Error: {}", e))
                    .ephemeral(true)
            ).await?;
        } else {
            ctx.interaction.create_followup(ctx.serenity_context, 
                serenity::all::CreateInteractionResponseFollowup::new()
                    .content(get(poise::Context::Application(ctx), "use__transfer_success", None, None))
                    .ephemeral(true)
            ).await?;
        }
    }

    Ok(())
}

async fn process_use_with_transaction(
    _ctx: poise::ApplicationContext<'_, Data, Error>,
    universe_id: ObjectId,
    character_id: ObjectId,
    tool: Tool,
    content: String,
) -> Result<(), Error> {
    let db_client = get_db_client().await;
    let mut session = db_client.start_session().await?;
    
    session.start_transaction().await?;
    
    let result = process_use_transfer_with_session(&mut session, universe_id, character_id, tool, content).await;
    
    match result {
        Ok(_) => {
            session.commit_transaction().await?;
            Ok(())
        }
        Err(e) => {
            session.abort_transaction().await?;
            Err(e.into())
        }
    }
}

async fn validate_transfer_with_session(
    session: &mut mongodb::ClientSession,
    universe_id: ObjectId,
    tool: &Tool,
    character_id: ObjectId,
    content: &str,
) -> Result<(std::collections::HashMap<ObjectId, u64>, std::collections::HashMap<ObjectId, u64>), String> {
    let lines: Vec<&str> = content.lines().collect();
    
    // 1. Get current inventories with session
    let initial_tool_inv = Inventory::get_by_holder_with_session(session, universe_id, tool._id.unwrap(), HolderType::Item).await
        .map_err(|e| e.to_string())?;
    let initial_char_inv = Inventory::get_by_holder_with_session(session, universe_id, character_id, HolderType::Character).await
        .map_err(|e| e.to_string())?;

    let mut tool_items = std::collections::HashMap::new();
    for inv in initial_tool_inv {
        tool_items.insert(inv.item_id, inv.quantity);
    }

    let mut char_items = std::collections::HashMap::new();
    for inv in initial_char_inv {
        char_items.insert(inv.item_id, inv.quantity);
    }

    // 2. Parse lines
    for line in lines {
        let line = line.trim();
        if line.is_empty() { continue; }

        let mode = &line[..1];
        let rest = line[1..].trim();
        
        let parts: Vec<&str> = rest.rsplitn(2, ' ').collect();
        let (item_name, quantity) = if parts.len() == 2 {
            let q = parts[0].parse::<u64>().unwrap_or(1);
            (parts[1], q)
        } else {
            (rest, 1)
        };

        let item = get_item_by_name(universe_id, item_name).await
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("Item not found: {}", item_name))?;

        if mode == ">" {
            // Take from tool, give to character
            let current_q = tool_items.get(&item._id).cloned().unwrap_or(0);
            if current_q < quantity {
                return Err(format!("Not enough {} in tool", item_name));
            }
            *tool_items.entry(item._id).or_insert(0) -= quantity;
            *char_items.entry(item._id).or_insert(0) += quantity;
        } else if mode == "<" {
            // Give to tool, take from character
            let current_q = char_items.get(&item._id).cloned().unwrap_or(0);
            if current_q < quantity {
                return Err(format!("Not enough {} in character inventory", item_name));
            }
            *char_items.entry(item._id).or_insert(0) -= quantity;
            *tool_items.entry(item._id).or_insert(0) += quantity;
        }
    }

    // 3. Verify tool inventory size
    let new_total_size: u64 = tool_items.values().sum();
    if new_total_size > tool.inventory_size {
        return Err(format!("Inventory size exceeded: {}/{}", new_total_size, tool.inventory_size));
    }

    Ok((tool_items, char_items))
}

async fn process_use_transfer_with_session(
    session: &mut mongodb::ClientSession,
    universe_id: ObjectId,
    character_id: ObjectId,
    tool: Tool,
    content: String,
) -> Result<(), String> {
    let (tool_items, char_items) = validate_transfer_with_session(session, universe_id, &tool, character_id, &content).await?;

    // 4. Apply changes using session
    let initial_tool_inv_map: std::collections::HashMap<_, _> = Inventory::get_by_holder_with_session(session, universe_id, tool._id.unwrap(), HolderType::Item).await
        .map_err(|e| e.to_string())?
        .into_iter().map(|i| (i.item_id, i.quantity)).collect();

    for (item_id, &new_q) in &tool_items {
        let old_q = initial_tool_inv_map.get(item_id).cloned().unwrap_or(0);
        if new_q > old_q {
            let _ = Inventory::add_item_to_inventory_with_session(session, universe_id, tool._id.unwrap(), HolderType::Item, *item_id, new_q - old_q).await.map_err(|e| e.to_string());
        } else if new_q < old_q {
            Inventory::remove_item_from_holder_with_session(session, universe_id, tool._id.unwrap(), HolderType::Item, *item_id, old_q - new_q).await.map_err(|e| e.to_string())?;
        }
    }

    let initial_char_inv_map: std::collections::HashMap<_, _> = Inventory::get_by_holder_with_session(session, universe_id, character_id, HolderType::Character).await
        .map_err(|e| e.to_string())?
        .into_iter().map(|i| (i.item_id, i.quantity)).collect();

    for (item_id, &new_q) in &char_items {
        let old_q = initial_char_inv_map.get(item_id).cloned().unwrap_or(0);
        if new_q > old_q {
            let _ = Inventory::add_item_to_inventory_with_session(session, universe_id, character_id, HolderType::Character, *item_id, new_q - old_q).await.map_err(|e| e.to_string());
        } else if new_q < old_q {
            Inventory::remove_item_from_holder_with_session(session, universe_id, character_id, HolderType::Character, *item_id, old_q - new_q).await.map_err(|e| e.to_string())?;
        }
    }

    Ok(())
}

pub async fn create_tool_selection_page(
    page_idx: usize,
    universe_id: ObjectId,
    channel_id: u64,
    locale: &str,
) -> Result<(serenity::all::CreateEmbed, Vec<serenity::all::CreateActionRow>), Error> {
    let tools = Tool::get_by_channel_id(universe_id, channel_id).await?;

    if tools.is_empty() {
        let embed = serenity::all::CreateEmbed::new()
            .title(get_by_locale(locale, "use__list_tools", None, None))
            .description(get_by_locale(locale, "use__no_tools_found", Some("message"), None));
        return Ok((embed, vec![]));
    }

    let items_per_page = 5;
    let total_pages = (tools.len() as f64 / items_per_page as f64).ceil() as usize;
    let start_idx = page_idx * items_per_page;
    let end_idx = (start_idx + items_per_page).min(tools.len());
    let page_tools = &tools[start_idx..end_idx];

    let mut description = String::new();
    let mut options = Vec::new();

    for tool in page_tools {
        let tool_id = tool._id.unwrap();
        description.push_str(&format!("**{}** - ID: `{}`\n", tool.name, tool_id));
        options.push(serenity::all::CreateSelectMenuOption::new(
            format!("{} (ID: {})", tool.name, tool_id),
            tool_id.to_hex()
        ));
    }

    let mut footer_args = FluentArgs::new();
    footer_args.set("current", page_idx + 1);
    footer_args.set("total", total_pages);

    let carousel_page = CarouselPage {
        title: get_by_locale(locale, "use__list_tools", None, None),
        description,
        fields: vec![],
        footer: get_by_locale(locale, "use__list_tools", Some("footer"), Some(&footer_args)),
        color: serenity::all::Colour::ORANGE,
    };

    let carousel_config = CarouselConfig {
        prefix: "tool_sel".to_string(),
        current_page: page_idx,
        total_pages,
        metadata: vec![universe_id.to_hex(), channel_id.to_string()],
    };

    let embed = create_carousel_embed(carousel_page);
    let mut components = create_carousel_components(carousel_config, locale);

    let select_id = format!("{}:select:{}:{}:{}", "tool_sel", universe_id.to_hex(), channel_id, page_idx);
    let select_menu = serenity::all::CreateSelectMenu::new(select_id, serenity::all::CreateSelectMenuKind::String { options })
        .placeholder(get_by_locale(locale, "use__list_tools", Some("select_placeholder"), None));
    
    components.push(serenity::all::CreateActionRow::SelectMenu(select_menu));

    Ok((embed, components))
}

pub async fn handle_tool_selection_interaction(
    ctx: serenity::all::Context,
    component: serenity::all::ComponentInteraction,
    action: &str,
    universe_id_hex: &str,
    channel_id_str: &str,
    page: usize,
) -> Result<(), Error> {
    let universe_id = ObjectId::from_str(universe_id_hex).map_err(|_| "Invalid universe ID")?;
    let channel_id = channel_id_str.parse::<u64>().map_err(|_| "Invalid channel ID")?;
    let locale = component.locale.as_str();

    match action {
        "prev" | "next" | "refresh" => {
            let (embed, components) = create_tool_selection_page(page, universe_id, channel_id, locale).await?;
            component.create_response(&ctx.http, serenity::all::CreateInteractionResponse::UpdateMessage(
                serenity::all::CreateInteractionResponseMessage::new()
                    .embed(embed)
                    .components(components)
            )).await?;
        }
        "select" => {
            if let serenity::all::ComponentInteractionDataKind::StringSelect { values } = &component.data.kind {
                if let Some(tool_id_hex) = values.first() {
                    let tool_id = ObjectId::from_str(tool_id_hex)?;
                    
                    let tool = match Tool::get_by_id(tool_id).await? {
                        Some(t) => t,
                        None => return Err("error:use__tool_not_found".into()),
                    };

                    if tool.inventory_size == 0 {
                        return Err("error:use__no_inventory".into());
                    }

                    // On récupère l'univers et le personnage pour le modal
                    let guild_id = component.guild_id.ok_or("error:item__guild_only")?;
                    let user_id = component.user.id;
                    
                    let universe = get_universe_by_server_id(guild_id.get()).await?
                        .ok_or("error:use__universe_not_found")?;
                    
                    let character = crate::database::characters::get_character_by_user_id(universe.universe_id, user_id.get()).await?
                        .ok_or("error:use__character_not_found")?;

                    // On envoie le modal
                    // Attention: on doit utiliser l'interaction originale pour envoyer le modal
                    execute_use_modal_from_interaction(&ctx, component, tool, character._id).await?;
                }
            }
        }
        _ => {}
    }

    Ok(())
}

pub async fn execute_use_modal_from_interaction(
    ctx: &serenity::all::Context,
    interaction: serenity::all::ComponentInteraction,
    tool: Tool,
    character_id: ObjectId,
) -> Result<(), Error> {
    let universe_id = tool.universe_id;
    let locale = interaction.locale.as_str();
    
    // Get tool inventory content
    let tool_inventory = Inventory::get_by_holder(universe_id, tool._id.unwrap(), HolderType::Item).await?;
    let mut current_size: u64 = 0;
    let mut items_list = String::new();
    
    for inv in &tool_inventory {
        if let Ok(Some(item)) = get_item_by_id(inv.item_id).await {
            items_list.push_str(&format!("- {} {}\n", item.item_name, inv.quantity));
            current_size += inv.quantity;
        }
    }

    let tool_header = format!("### {} ({} / {})\n", tool.name, current_size, tool.inventory_size);
    let mut tool_content = tool_header.clone();
    
    if items_list.is_empty() {
        tool_content.push_str(&get_by_locale(locale, "use__empty_inventory", None, None));
    } else {
        tool_content.push_str(&items_list);
    }

    // Get character inventory content
    let character_inventory_items = Inventory::get_by_holder(universe_id, character_id, HolderType::Character).await?;
    let char_inv_label = get_by_locale(locale, "use__modal_character_inventory_label", None, None);
    let char_header = format!("### {}\n", char_inv_label);
    let mut character_inventory = char_header.clone();
    
    for inv in &character_inventory_items {
        if let Ok(Some(item)) = get_item_by_id(inv.item_id).await {
            character_inventory.push_str(&format!("- {} {}\n", item.item_name, inv.quantity));
        }
    }
    
    if character_inventory == char_header {
        character_inventory.push_str(&get_by_locale(locale, "use__empty_inventory", None, None));
    }

    let title = tool.name.clone();
    let label = get_by_locale(locale, "use__modal_label", None, None);
    let instructions_value = get_by_locale(locale, "use__modal_instructions_value", None, None);
    
    let custom_id = format!("{}-{}", interaction.id, "use_modal");

    let modal_json = json!({
        "type": 9,
        "data": {
            "custom_id": custom_id,
            "title": title,
            "components": [
                {
                    "type": 1,
                    "components": [
                        {
                            "type": 4,
                            "custom_id": "content",
                            "label": label,
                            "style": 2,
                            "placeholder": "> [item_name] [quantité]\n< [item_name] [quantité]",
                            "required": false
                        }
                    ]
                },
                {
                    "type": 10,
                    "content": tool_content,
                },
                {
                    "type": 10,
                    "content": character_inventory,
                },
                {
                    "type": 10,
                    "content": instructions_value,
                }
            ]
        }
    });

    ctx.http.create_interaction_response(interaction.id, &interaction.token, &modal_json, vec![]).await?;

    let response = serenity::collector::ModalInteractionCollector::new(ctx)
        .filter(move |m| m.data.custom_id == custom_id)
        .timeout(std::time::Duration::from_secs(600))
        .await;

    if let Some(m) = response {
        let content = m.data.components.iter()
            .flat_map(|row| row.components.iter())
            .find_map(|component| {
                if let serenity::all::ActionRowComponent::InputText(it) = component {
                    if it.custom_id == "content" {
                        return Some(it.value.clone().unwrap_or_default());
                    }
                }
                None
            })
            .unwrap_or_default();

        m.create_response(ctx, CreateInteractionResponse::Acknowledge).await?;

        // Process the content
        let db_client = get_db_client().await;
        let mut session = db_client.start_session().await?;
        session.start_transaction().await?;

        let result = process_use_transfer_with_session(&mut session, universe_id, character_id, tool.clone(), content.clone()).await;
        
        match result {
            Ok(_) => {
                session.commit_transaction().await?;
                interaction.create_followup(ctx, 
                    serenity::all::CreateInteractionResponseFollowup::new()
                        .content(get_by_locale(locale, "use__transfer_success", None, None))
                        .ephemeral(true)
                ).await?;
            }
            Err(err_msg) => {
                session.abort_transaction().await?;
                interaction.create_followup(ctx, 
                    serenity::all::CreateInteractionResponseFollowup::new()
                        .content(format!("Error: {}", err_msg))
                        .ephemeral(true)
                ).await?;
            }
        }
    }

    Ok(())
}
