use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use serenity::all::{Http, UserId, CreateEmbed, Color, CreateMessage};
use once_cell::sync::Lazy;
use tokio::sync::Mutex;
use tokio::task::JoinHandle;
use tokio::time::sleep;
use fluent::FluentArgs;
use crate::database::loot::PlayerLoot;
use crate::database::inventory::{Inventory, HolderType};
use crate::database::items::get_item_by_name;
use crate::database::universe::get_universe_by_id;
use crate::translation::get_by_locale;

pub static LOOTS: Lazy<Arc<Mutex<Vec<PlayerLoot>>>> = Lazy::new(|| Arc::new(Mutex::new(vec![])));
pub static LOOT_SLEEPER: Lazy<Arc<Mutex<Option<JoinHandle<()>>>>> = Lazy::new(|| Arc::new(Mutex::new(None)));
pub static HTTP_CLIENT: Lazy<Arc<Mutex<Option<Arc<Http>>>>> = Lazy::new(|| Arc::new(Mutex::new(None)));

fn loot_process(delay: u64) -> JoinHandle<()> {
    tokio::spawn(async move {
        sleep(Duration::from_secs(delay)).await;

        let mut next_delay: Option<u64> = None;
        let mut completed_loots: Vec<PlayerLoot> = Vec::new();

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

    let mut looted_items_str = Vec::new();

    // Production des items
    for item_name in &loot.items {
        if let Ok(Some(item)) = get_item_by_name(loot.universe_id, item_name).await {
            Inventory::add_item_to_inventory(
                loot.universe_id,
                character._id,
                HolderType::Character,
                item._id,
                1
            ).await?;
            looted_items_str.push(item_name.clone());
        }
    }

    // Marquer comme fini et supprimer de la DB
    loot.is_finished = true;
    let _ = loot.remove().await;

    // Notifier l'utilisateur
    if let Some(http) = HTTP_CLIENT.lock().await.as_ref() {
        let mut universe_name = String::new();
        if let Ok(Some(u)) = get_universe_by_id(loot.universe_id).await {
            universe_name = u.name;
        }

        let mut args = FluentArgs::new();
        args.set("items", looted_items_str.join(", "));
        args.set("universe", universe_name.as_str());
        args.set("character", character.name.as_str());

        let title = get_by_locale("fr", "loot_table__loot_finished_title", None, None);
        let description_key = if is_late { "loot_table__loot_finished_late_message" } else { "loot_table__loot_finished_message" };
        let description = get_by_locale("fr", description_key, None, Some(&args));

        let embed = CreateEmbed::new()
            .title(title)
            .description(description)
            .color(Color::from_rgb(0, 255, 0));

        let _ = UserId::new(loot.user_id).direct_message(http, CreateMessage::new().embed(embed)).await;
    }

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

pub async fn setup() {
    let universes = crate::database::universe::Universe::get_all_universes().await.unwrap_or_default();
    
    let mut all_active_loots = Vec::new();
    for universe in universes {
        if let Ok(active_loots) = PlayerLoot::get_active_loots(universe.universe_id).await {
            all_active_loots.extend(active_loots);
        }
    }

    if all_active_loots.is_empty() {
        return;
    }

    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
    let (finished, pending): (Vec<PlayerLoot>, Vec<PlayerLoot>) = all_active_loots.into_iter().partition(|l| l.end_timestamp <= now);

    for loot in finished {
        let _ = finalize_loot(loot, true).await;
    }

    if pending.is_empty() {
        return;
    }

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
}
