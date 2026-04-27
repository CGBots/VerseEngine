use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use serenity::all::{Http, UserId, Color, CreateMessage, CreateInteractionResponse, CreateInteractionResponseMessage};
use once_cell::sync::Lazy;
use tokio::sync::Mutex;
use tokio::task::JoinHandle;
use tokio::time::sleep;
use fluent::FluentArgs;
use crate::database::loot::PlayerLoot;
use crate::database::db_client::get_db_client;
use crate::database::inventory::{Inventory, HolderType};
use crate::database::items::get_item_by_name;
use crate::database::universe::get_universe_by_id;
use crate::translation::get_by_locale;
use crate::utility::carousel::{CarouselConfig, CarouselPage, create_carousel_embed, create_carousel_components, paginate_text};

pub static LOOTS: Lazy<Arc<Mutex<Vec<PlayerLoot>>>> = Lazy::new(|| Arc::new(Mutex::new(vec![])));
pub static LOOT_SLEEPER: Lazy<Arc<Mutex<Option<JoinHandle<()>>>>> = Lazy::new(|| Arc::new(Mutex::new(None)));
pub static HTTP_CLIENT: Lazy<Arc<Mutex<Option<Arc<Http>>>>> = Lazy::new(|| Arc::new(Mutex::new(None)));

fn loot_process(delay: u64) -> JoinHandle<()> {
    tokio::spawn(async move {
        sleep(Duration::from_secs(delay)).await;

        let mut next_delay: Option<u64> = None;
        let completed_loots: Vec<PlayerLoot>;

        {
            let mut loots = LOOTS.lock().await;
            if loots.is_empty() {
                let mut sleeper = LOOT_SLEEPER.lock().await;
                *sleeper = None;
                return;
            }

            let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
            
            let (finished, pending): (Vec<PlayerLoot>, Vec<PlayerLoot>) = loots.clone().into_iter().partition(|l| l.end_timestamp <= now);
            *loots = pending;
            completed_loots = finished;

            if !loots.is_empty() {
                let mut min_end = u64::MAX;
                for l in loots.iter() {
                    if l.end_timestamp < min_end {
                        min_end = l.end_timestamp;
                    }
                }
                next_delay = Some(if min_end > now { min_end - now } else { 0 });
            }
        }

        for loot in completed_loots {
            let _ = finalize_loot(loot, false).await;
        }

        if let Some(d) = next_delay {
            let mut sleeper = LOOT_SLEEPER.lock().await;
            *sleeper = Some(loot_process(d));
        } else {
            let mut sleeper = LOOT_SLEEPER.lock().await;
            *sleeper = None;
        }
    })
}

async fn finalize_loot(mut loot: PlayerLoot, is_late: bool) -> Result<(), crate::discord::poise_structs::Error> {
    let character = crate::database::characters::get_character_by_user_id(loot.universe_id, loot.user_id).await?
        .ok_or("loot_table__character_not_found")?;

    let item_counts: std::collections::HashMap<String, u64> = {
        let mut counts = std::collections::HashMap::new();
        for item_name in &loot.items {
            *counts.entry(item_name.clone()).or_insert(0) += 1;
        }
        counts
    };

    let mut inventory_ids = Vec::new();

    let client = get_db_client().await;
    let mut session = client.start_session().await?;
    session.start_transaction().await?;

    let result: Result<(), crate::discord::poise_structs::Error> = async {
        // Production des items
        for (item_name, &quantity) in &item_counts {
            if let Ok(Some(item)) = get_item_by_name(loot.universe_id, item_name).await {
                let inv_id = Inventory::add_item_to_inventory_with_session(
                    &mut session,
                    loot.universe_id,
                    character._id,
                    HolderType::Character,
                    item._id,
                    quantity
                ).await?;
                inventory_ids.push(format!("{}:{}:{}", inv_id.to_hex(), item_name, quantity));
            }
        }

        // Marquer comme fini et supprimer de la DB
        loot.is_finished = true;
        loot.remove_with_session(&mut session).await?;
        Ok(())
    }.await;

    match result {
        Ok(_) => {
            session.commit_transaction().await?;
        }
        Err(e) => {
            session.abort_transaction().await?;
            return Err(e);
        }
    }

    // Notifier l'utilisateur
    if let Some(http) = HTTP_CLIENT.lock().await.as_ref() {
        let universe_res = get_universe_by_id(loot.universe_id).await;
        let universe = match universe_res {
            Ok(Some(u)) => u,
            _ => return Ok(()), // Should not happen but safety first
        };

        let locale = "fr"; // Default to fr for DMs for now, or we could try to get it from somewhere
        let (embed, components) = create_loot_finished_page(
            0,
            &inventory_ids,
            &universe.name,
            &character.name,
            is_late,
            locale,
            Some(loot.universe_id)
        ).await?;

        let _ = UserId::new(loot.user_id).direct_message(http, CreateMessage::new().embed(embed).components(components)).await;
    }

    Ok(())
}

pub async fn create_loot_finished_page(
    page_idx: usize,
    items_data: &[String],
    universe_name: &str,
    character_name: &str,
    is_late: bool,
    locale: &str,
    universe_id: Option<mongodb::bson::oid::ObjectId>,
) -> Result<(serenity::all::CreateEmbed, Vec<serenity::all::CreateActionRow>), crate::discord::poise_structs::Error> {
    let mut items_text = Vec::new();
    
    // Sort items by name for consistent display
    let mut sorted_items = items_data.to_vec();
    sorted_items.sort_by(|a, b| {
        let name_a = a.split(':').nth(1).unwrap_or("");
        let name_b = b.split(':').nth(1).unwrap_or("");
        name_a.cmp(name_b)
    });

    for item_data in sorted_items {
        let parts: Vec<&str> = item_data.split(':').collect();
        if parts.len() == 3 {
            let id = parts[0];
            let item_name = parts[1];
            let quantity = parts[2].parse::<u64>().unwrap_or(1);

            items_text.push(format!("- {}x {} (ID: `{}`)", quantity, item_name, id));
        }
    }

    // Pagination logic
    let items_per_page = 15;
    let empty_msg = get_by_locale(locale, "loot_table__empty_loot", None, None);
    let pages = paginate_text(&items_text, items_per_page, &empty_msg);
    
    let total_pages = pages.len();
    let current_page = page_idx.min(total_pages - 1);

    // Build the page
    let mut args = FluentArgs::new();
    args.set("universe", universe_name);
    args.set("character", character_name);
    
    let mut footer_args = FluentArgs::new();
    footer_args.set("current", current_page + 1);
    footer_args.set("total", total_pages);

    let title = get_by_locale(locale, "loot_table__loot_finished_title", None, None);
    let description_key = if is_late { "loot_table__loot_finished_late_message" } else { "loot_table__loot_finished_message" };
    
    // We want the description to include the items for the current page
    let base_description = get_by_locale(locale, description_key, None, Some(&args));
    let description = format!("{}\n\n{}", base_description, pages[current_page]);

    let carousel_page = CarouselPage {
        title,
        description,
        fields: vec![],
        footer: get_by_locale(locale, "inventory__page_footer", None, Some(&footer_args)),
        color: Color::from_rgb(0, 255, 0),
    };

    let carousel_config = CarouselConfig {
        prefix: "loot_res".to_string(),
        current_page,
        total_pages,
        metadata: vec![
            universe_name.to_string(),
            character_name.to_string(),
            is_late.to_string(),
            items_data.join(","),
            universe_id.map(|id| id.to_hex()).unwrap_or_default()
        ],
    };

    let embed = create_carousel_embed(carousel_page);
    let components = create_carousel_components(carousel_config, locale);

    Ok((embed, components))
}

pub async fn handle_loot_carousel_interaction(
    ctx: serenity::all::Context,
    component: serenity::all::ComponentInteraction,
    universe_name: &str,
    character_name: &str,
    is_late: bool,
    items_data_raw: &str,
    page: usize,
    universe_id_str: Option<&str>,
) -> Result<(), crate::discord::poise_structs::Error> {
    let items_data: Vec<String> = items_data_raw.split(',').map(|s| s.to_string()).collect();
    
    let universe_id = universe_id_str.and_then(|id| mongodb::bson::oid::ObjectId::parse_str(id).ok());

    let (embed, components) = create_loot_finished_page(
        page,
        &items_data,
        universe_name,
        character_name,
        is_late,
        component.locale.as_str(),
        universe_id
    ).await?;

    component.create_response(&ctx, CreateInteractionResponse::UpdateMessage(
        CreateInteractionResponseMessage::new()
            .embed(embed)
            .components(components)
    )).await?;

    Ok(())
}

pub async fn add_loot(http: Arc<Http>, player_loot: PlayerLoot) -> Result<(), crate::discord::poise_structs::Error> {
    {
        let mut http_client = HTTP_CLIENT.lock().await;
        if http_client.is_none() {
            *http_client = Some(http);
        }
    }

    let _ = player_loot.clone().upsert().await;

    let mut loots = LOOTS.lock().await;
    loots.push(player_loot.clone());

    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
    let delay = if player_loot.end_timestamp > now { player_loot.end_timestamp - now } else { 0 };

    let mut sleeper = LOOT_SLEEPER.lock().await;
    if let Some(handle) = sleeper.as_ref() {
        handle.abort();
        *sleeper = Some(loot_process(delay));
    } else {
        *sleeper = Some(loot_process(delay));
    }

    Ok(())
}

pub async fn stop_loot(universe_id: mongodb::bson::oid::ObjectId, user_id: u64) -> Result<Option<PlayerLoot>, crate::discord::poise_structs::Error> {
    let mut removed_loot = None;
    {
        let mut loots = LOOTS.lock().await;
        if let Some(pos) = loots.iter().position(|l| l.user_id == user_id && l.universe_id == universe_id) {
            removed_loot = Some(loots.remove(pos));
        }
    }

    if let Some(loot) = &removed_loot {
        let _ = loot.remove().await;
    } else {
        if let Ok(Some(loot)) = PlayerLoot::get_by_user_id(universe_id, user_id).await {
            let _ = loot.remove().await;
            removed_loot = Some(loot);
        }
    }

    Ok(removed_loot)
}

#[allow(dead_code)]
pub async fn setup() {
    let universes = match crate::database::universe::Universe::get_all_universes().await {
        Ok(u) => u,
        Err(e) => panic!("Impossible d'initialiser la file de loot : erreur lors de la récupération des univers : {:?}", e),
    };
    
    let mut all_active_loots = Vec::new();
    for universe in universes {
        match PlayerLoot::get_active_loots(universe.universe_id).await {
            Ok(active_loots) => all_active_loots.extend(active_loots),
            Err(e) => panic!("Impossible d'initialiser la file de loot : erreur lors de la récupération des loots pour l'univers {:?} : {:?}", universe.universe_id, e),
        }
    }

    if all_active_loots.is_empty() {
        println!("File de loot initialisée (0 loot en cours).");
        return;
    }

    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
    let (finished, pending): (Vec<PlayerLoot>, Vec<PlayerLoot>) = all_active_loots.into_iter().partition(|l| l.end_timestamp <= now);

    for loot in finished {
        let _ = finalize_loot(loot, true).await;
    }

    if pending.is_empty() {
        println!("File de loot initialisée (0 loot en attente après traitement des loots terminés).");
        return;
    }

    let pending_count = pending.len();

    {
        let mut loots_lock = LOOTS.lock().await;
        *loots_lock = pending;
    }

    let mut min_delay = u64::MAX;
    
    {
        let loots = LOOTS.lock().await;
        for l in loots.iter() {
            let delay = if l.end_timestamp > now { l.end_timestamp - now } else { 0 };
            if delay < min_delay {
                min_delay = delay;
            }
        }
    }

    if min_delay != u64::MAX {
        let mut sleeper = LOOT_SLEEPER.lock().await;
        *sleeper = Some(loot_process(min_delay));
    }

    println!("File de loot initialisée ({} loot(s) en attente).", pending_count);
}
