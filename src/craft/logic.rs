use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use serenity::all::{Http, UserId, CreateEmbed, Color, CreateMessage};
use once_cell::sync::Lazy;
use tokio::sync::Mutex;
use tokio::task::JoinHandle;
use tokio::time::sleep;
use fluent::FluentArgs;
use crate::database::craft::PlayerCraft;
use crate::database::recipe::Recipe;
use crate::database::inventory::{Inventory, HolderType};
use crate::database::universe::get_universe_by_id;
use crate::translation::get_by_locale;

pub static CRAFTS: Lazy<Arc<Mutex<Vec<PlayerCraft>>>> = Lazy::new(|| Arc::new(Mutex::new(vec![])));
pub static CRAFT_SLEEPER: Lazy<Arc<Mutex<Option<JoinHandle<()>>>>> = Lazy::new(|| Arc::new(Mutex::new(None)));
pub static HTTP_CLIENT: Lazy<Arc<Mutex<Option<Arc<Http>>>>> = Lazy::new(|| Arc::new(Mutex::new(None)));

fn craft_process(delay: u64) -> JoinHandle<()> {
    tokio::spawn(async move {
        sleep(Duration::from_secs(delay)).await;

        let mut next_delay: Option<u64> = None;
        let _next_id: Option<String> = None;

        let completed_crafts: Vec<PlayerCraft>;

        {
            let mut crafts = CRAFTS.lock().await;
            if crafts.is_empty() {
                let mut sleeper = CRAFT_SLEEPER.lock().await;
                *sleeper = None;
                return;
            }

            let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
            
            // On sépare les crafts terminés de ceux qui restent
            let (finished, pending): (Vec<PlayerCraft>, Vec<PlayerCraft>) = crafts.clone().into_iter().partition(|c| c.end_timestamp <= now);
            *crafts = pending;
            completed_crafts = finished;

            // Calculer le prochain réveil
            if !crafts.is_empty() {
                let mut min_end = u64::MAX;
                for c in crafts.iter() {
                    if c.end_timestamp < min_end {
                        min_end = c.end_timestamp;
                    }
                }
                next_delay = Some(if min_end > now { min_end - now } else { 0 });
            }
        }

        // Traiter les crafts terminés
        for craft in completed_crafts {
            let _ = finalize_craft(craft, false).await;
        }

        if let Some(d) = next_delay {
            let mut sleeper = CRAFT_SLEEPER.lock().await;
            *sleeper = Some(craft_process(d));
        } else {
            let mut sleeper = CRAFT_SLEEPER.lock().await;
            *sleeper = None;
        }
    })
}

async fn finalize_craft(mut craft: PlayerCraft, is_late: bool) -> Result<(), crate::discord::poise_structs::Error> {
    let recipe = Recipe::get_by_universe(craft.universe_id).await?
        .into_iter().find(|r| r._id == Some(craft.recipe_id))
        .ok_or("recipe__not_found")?;

    let character = crate::database::characters::get_character_by_user_id(craft.universe_id, craft.user_id).await?
        .ok_or("recipe__character_not_found")?;

    // Production des items
    for (qty, item_id) in &recipe.result {
        Inventory::add_item_to_inventory(
            craft.universe_id,
            character._id,
            HolderType::Character,
            *item_id,
            *qty
        ).await?;
    }

    // Marquer comme fini et supprimer de la DB
    craft.is_finished = true;
    let _ = craft.remove().await;

    // Notifier l'utilisateur
    if let Some(http) = HTTP_CLIENT.lock().await.as_ref() {
        let mut universe_name = String::new();
        if let Ok(Some(u)) = get_universe_by_id(craft.universe_id).await {
            universe_name = u.name;
        }

        let mut args = FluentArgs::new();
        args.set("recipe_name", recipe.recipe_name.as_str());
        args.set("universe", universe_name.as_str());

        let title = get_by_locale("fr", "recipe__craft_finished_title", None, None);
        let description_key = if is_late { "recipe__craft_finished_late_message" } else { "recipe__craft_finished_message" };
        let description = get_by_locale("fr", description_key, None, Some(&args));

        let embed = CreateEmbed::new()
            .title(title)
            .description(description)
            .color(Color::from_rgb(0, 255, 0));

        let _ = UserId::new(craft.user_id).direct_message(http, CreateMessage::new().embed(embed)).await;
    }

    Ok(())
}

pub async fn add_craft(http: Arc<Http>, player_craft: PlayerCraft) -> Result<(), crate::discord::poise_structs::Error> {
    {
        let mut http_client = HTTP_CLIENT.lock().await;
        if http_client.is_none() {
            *http_client = Some(http);
        }
    }

    let _ = player_craft.clone().upsert().await;

    let mut crafts = CRAFTS.lock().await;
    crafts.push(player_craft.clone());

    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
    let delay = if player_craft.end_timestamp > now { player_craft.end_timestamp - now } else { 0 };

    let mut sleeper = CRAFT_SLEEPER.lock().await;
    if let Some(handle) = sleeper.as_ref() {
        // On ne redémarre le sleeper que si le nouveau craft finit plus tôt
        // Pour simplifier, on redémarre si le nouveau délai est plus court que le prochain réveil attendu
        // Mais ici on va juste redémarrer le sleeper pour qu'il prenne en compte le nouveau craft le plus proche
        handle.abort();
        *sleeper = Some(craft_process(delay));
    } else {
        *sleeper = Some(craft_process(delay));
    }

    Ok(())
}

pub async fn stop_craft(universe_id: mongodb::bson::oid::ObjectId, user_id: u64) -> Result<Option<PlayerCraft>, crate::discord::poise_structs::Error> {
    let mut removed_craft = None;
    {
        let mut crafts = CRAFTS.lock().await;
        if let Some(pos) = crafts.iter().position(|c| c.user_id == user_id && c.universe_id == universe_id) {
            removed_craft = Some(crafts.remove(pos));
        }
    }

    if let Some(craft) = &removed_craft {
        let _ = craft.remove().await;
        // On ne s'embête pas à redémarrer le sleeper, il se réveillera et verra qu'il n'y a plus rien à faire ou passera au suivant
    } else {
        // Vérifier en DB si jamais il n'est pas dans la liste MOVES (ne devrait pas arriver si tout est synchro)
        if let Ok(Some(craft)) = PlayerCraft::get_by_user_id(universe_id, user_id).await {
            let _ = craft.remove().await;
            removed_craft = Some(craft);
        }
    }

    Ok(removed_craft)
}

#[allow(dead_code)]
pub async fn setup() {
    let universes = match crate::database::universe::Universe::get_all_universes().await {
        Ok(u) => u,
        Err(e) => panic!("Impossible d'initialiser la file de craft : erreur lors de la récupération des univers : {:?}", e),
    };
    
    let mut all_active_crafts = Vec::new();
    for universe in universes {
        match PlayerCraft::get_active_crafts(universe.universe_id).await {
            Ok(active_crafts) => all_active_crafts.extend(active_crafts),
            Err(e) => panic!("Impossible d'initialiser la file de craft : erreur lors de la récupération des crafts pour l'univers {:?} : {:?}", universe.universe_id, e),
        }
    }

    if all_active_crafts.is_empty() {
        println!("File de craft initialisée (0 craft en cours).");
        return;
    }

    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
    let (finished, pending): (Vec<PlayerCraft>, Vec<PlayerCraft>) = all_active_crafts.into_iter().partition(|c| c.end_timestamp <= now);

    // Traiter les crafts qui se sont terminés pendant que le bot était éteint
    for craft in finished {
        let _ = finalize_craft(craft, true).await;
    }

    if pending.is_empty() {
        println!("File de craft initialisée (0 craft en attente après traitement des crafts terminés).");
        return;
    }

    let pending_count = pending.len();

    {
        let mut crafts_lock = CRAFTS.lock().await;
        *crafts_lock = pending;
    }

    // Démarrer le sleeper pour le craft le plus proche
    let mut min_delay = u64::MAX;
    
    {
        let crafts = CRAFTS.lock().await;
        for c in crafts.iter() {
            let delay = if c.end_timestamp > now { c.end_timestamp - now } else { 0 };
            if delay < min_delay {
                min_delay = delay;
            }
        }
    }

    if min_delay != u64::MAX {
        let mut sleeper = CRAFT_SLEEPER.lock().await;
        *sleeper = Some(craft_process(min_delay));
    }

    println!("File de craft initialisée ({} craft(s) en attente).", pending_count);
}
