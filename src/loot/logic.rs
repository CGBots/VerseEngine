use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use serenity::all::{Http, UserId, Color, CreateMessage, CreateInteractionResponse, CreateInteractionResponseMessage};
use once_cell::sync::Lazy;
use tokio::sync::Mutex;
use fluent::FluentArgs;
use crate::database::loot::PlayerLoot;
use crate::database::db_client::get_db_client;
use crate::database::inventory::{Inventory, HolderType};
use crate::database::items::get_item_by_name;
use crate::database::universe::get_universe_by_id;
use crate::translation::get_by_locale;
use crate::utility::timer::{self, Timer};
use crate::utility::carousel::{CarouselConfig, CarouselPage, create_carousel_embed, create_carousel_components, paginate_text};

pub static LOOTS: Lazy<Arc<Mutex<Vec<PlayerLoot>>>> = Lazy::new(|| Arc::new(Mutex::new(vec![])));
/// Minuteur unique de la file des loots (voir [`crate::utility::timer`]).
pub static LOOT_TIMER: Lazy<Timer> = Lazy::new(|| Timer::new("loot"));
pub static HTTP_CLIENT: Lazy<Arc<Mutex<Option<Arc<Http>>>>> = Lazy::new(|| Arc::new(Mutex::new(None)));

fn now_secs() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs()
}

/// Sépare les loots terminés (`end_timestamp <= now`) de ceux encore en cours.
fn split_finished_loots(loots: Vec<PlayerLoot>, now: u64) -> (Vec<PlayerLoot>, Vec<PlayerLoot>) {
    loots.into_iter().partition(|l| l.end_timestamp <= now)
}

/// Délai avant la fin du loot le plus proche, `None` si la file est vide.
fn next_loot_delay(loots: &[PlayerLoot], now: u64) -> Option<u64> {
    loots.iter().map(|l| l.end_timestamp.saturating_sub(now)).min()
}

fn loot_job() -> timer::Job {
    timer::job(process_due_loots)
}

/// Finalise les loots échus et retourne le délai avant le prochain.
async fn process_due_loots() -> Option<u64> {
    let completed = {
        let mut loots = LOOTS.lock().await;
        let (finished, pending) = split_finished_loots(std::mem::take(&mut *loots), now_secs());
        *loots = pending;
        finished
    };

    for loot in completed {
        if let Err(e) = finalize_loot(loot, false).await {
            eprintln!("[loot] cannot finalize a loot: {e:?}");
        }
    }

    let loots = LOOTS.lock().await;
    next_loot_delay(&loots, now_secs())
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
    let mut given: Vec<(String, u64)> = Vec::new();

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
                given.push((item_name.clone(), quantity));
            }
        }

        // Marquer comme fini et supprimer de la DB
        loot.is_finished = true;
        let removed = loot.remove_with_session(&mut session).await?;
        timer::ensure_claimed(removed.deleted_count)?;
        Ok(())
    }.await;

    match result {
        Ok(_) => {
            session.commit_transaction().await?;
            // Avant l'évènement : l'activité relit l'état du farm et affiche ce butin.
            crate::activity::farm_results::record(loot.universe_id, loot.user_id, given, chrono::Utc::now().timestamp() as u64);
            crate::activity::events::notify_player(crate::activity::events::EventKind::FarmUpdated, loot.universe_id, loot.user_id);
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

    // Ne repousse jamais un réveil plus proche déjà prévu pour un autre loot.
    LOOT_TIMER.schedule(player_loot.end_timestamp.saturating_sub(now_secs()), loot_job());
    crate::activity::events::notify_player(crate::activity::events::EventKind::FarmUpdated, player_loot.universe_id, player_loot.user_id);

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
    if removed_loot.is_some() {
        crate::activity::events::notify_player(crate::activity::events::EventKind::FarmUpdated, universe_id, user_id);
    }

    Ok(removed_loot)
}

/// Recharge la file des loots depuis la base. Appelé à chaque `ready` Discord, donc aussi après
/// une reconnexion : l'ancien minuteur est arrêté, le traitement éventuellement en cours se termine
/// d'abord, et chaque loot n'est finalisé qu'une fois.
pub async fn setup() {
    LOOT_TIMER.stop();
    let _exclusive = LOOT_TIMER.exclusive().await;
    // Gardé jusqu'au remplacement de la file : un loot lancé pendant la relecture n'est pas perdu.
    let mut loots = LOOTS.lock().await;

    let universes = match crate::database::universe::Universe::get_all_universes().await {
        Ok(u) => u,
        Err(e) => {
            // Avant : panic. On garde la file en mémoire et on continue.
            eprintln!("Impossible d'initialiser la file de loot : erreur lors de la récupération des univers : {:?}", e);
            LOOT_TIMER.restart(next_loot_delay(&loots, now_secs()), loot_job());
            return;
        }
    };

    let mut all_active_loots = Vec::new();
    for universe in universes {
        match PlayerLoot::get_active_loots(universe.universe_id).await {
            Ok(active_loots) => all_active_loots.extend(active_loots),
            Err(e) => {
                eprintln!("Impossible d'initialiser la file de loot : erreur lors de la récupération des loots pour l'univers {:?} : {:?}", universe.universe_id, e);
                LOOT_TIMER.restart(next_loot_delay(&loots, now_secs()), loot_job());
                return;
            }
        }
    }

    let (finished, pending) = split_finished_loots(all_active_loots, now_secs());
    for loot in finished {
        if let Err(e) = finalize_loot(loot, true).await {
            eprintln!("[loot] cannot finalize a recovered loot: {e:?}");
        }
    }

    *loots = pending;
    LOOT_TIMER.restart(next_loot_delay(&loots, now_secs()), loot_job());
    println!("File de loot initialisée ({} loot(s) en attente).", loots.len());
}

#[cfg(test)]
mod tests {
    use super::*;
    use mongodb::bson::oid::ObjectId;

    fn loot_ending_at(end: u64) -> PlayerLoot {
        PlayerLoot {
            _id: None,
            universe_id: ObjectId::new(),
            user_id: end,
            server_id: 1,
            items: vec![],
            start_timestamp: 0,
            end_timestamp: end,
            is_finished: false,
        }
    }

    #[test]
    fn finished_loots_are_separated_from_pending_ones() {
        let (finished, pending) = split_finished_loots(vec![loot_ending_at(10), loot_ending_at(20), loot_ending_at(30)], 20);
        assert_eq!(finished.iter().map(|l| l.end_timestamp).collect::<Vec<_>>(), vec![10, 20]);
        assert_eq!(pending.iter().map(|l| l.end_timestamp).collect::<Vec<_>>(), vec![30]);
    }

    #[test]
    fn the_next_wake_up_targets_the_earliest_loot() {
        // L'ordre d'ajout ne compte pas : un loot ajouté après mais plus court passe devant.
        let loots = [loot_ending_at(500), loot_ending_at(150), loot_ending_at(300)];
        assert_eq!(next_loot_delay(&loots, 100), Some(50));
    }

    #[test]
    fn an_overdue_loot_wakes_up_immediately() {
        assert_eq!(next_loot_delay(&[loot_ending_at(90)], 100), Some(0));
    }

    #[test]
    fn no_loot_means_no_wake_up() {
        assert_eq!(next_loot_delay(&[], 100), None);
    }
}
