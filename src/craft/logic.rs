use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use serenity::all::{Http, UserId, CreateEmbed, Color, CreateMessage};
use once_cell::sync::Lazy;
use tokio::sync::Mutex;
use fluent::FluentArgs;
use crate::database::craft::PlayerCraft;
use crate::database::recipe::Recipe;
use crate::database::inventory::{Inventory, HolderType};
use crate::database::universe::get_universe_by_id;
use crate::translation::get_by_locale;
use mongodb::bson::oid::ObjectId;
use crate::utility::timer::{self, Timer};

pub static CRAFTS: Lazy<Arc<Mutex<Vec<PlayerCraft>>>> = Lazy::new(|| Arc::new(Mutex::new(vec![])));
/// Minuteur unique de la file des crafts (voir [`crate::utility::timer`]).
pub static CRAFT_TIMER: Lazy<Timer> = Lazy::new(|| Timer::new("craft"));
pub static HTTP_CLIENT: Lazy<Arc<Mutex<Option<Arc<Http>>>>> = Lazy::new(|| Arc::new(Mutex::new(None)));

/// Instant courant en secondes UNIX (0 si l'horloge système est avant 1970).
pub fn now_secs() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs()
}

/// Timestamp de fin d'un craft démarré à `now` avec un délai de `delay` secondes.
///
/// Sature au lieu de déborder si le délai de la recette est démesuré.
pub fn craft_end_timestamp(now: u64, delay: u64) -> u64 {
    now.saturating_add(delay)
}

/// Construit le craft en cours d'un joueur pour une recette à délai, démarré à `now`.
pub fn new_player_craft(universe_id: ObjectId, user_id: u64, server_id: u64, recipe_id: ObjectId, delay: u64, now: u64) -> PlayerCraft {
    PlayerCraft {
        _id: ObjectId::new(),
        universe_id,
        user_id,
        server_id,
        recipe_id,
        start_timestamp: now,
        end_timestamp: craft_end_timestamp(now, delay),
        is_finished: false,
    }
}

/// Temps restant (en secondes) avant `end_timestamp`, 0 si déjà dépassé.
pub fn remaining_delay_secs(end_timestamp: u64, now: u64) -> u64 {
    end_timestamp.saturating_sub(now)
}

/// Sépare les crafts terminés (`end_timestamp <= now`) de ceux encore en cours.
pub fn split_finished(crafts: Vec<PlayerCraft>, now: u64) -> (Vec<PlayerCraft>, Vec<PlayerCraft>) {
    crafts.into_iter().partition(|c| c.end_timestamp <= now)
}

/// Délai avant le prochain réveil de l'ordonnanceur : celui du craft qui se termine le plus tôt.
/// `None` s'il n'y a aucun craft en cours.
pub fn next_wakeup_delay(crafts: &[PlayerCraft], now: u64) -> Option<u64> {
    crafts
        .iter()
        .map(|c| remaining_delay_secs(c.end_timestamp, now))
        .min()
}

/// Vérifie qu'un joueur peut lancer une recette.
///
/// - `tools_needed` : outils requis par la recette (non consommés) ;
/// - `available_tools` : outils disponibles (inventaire du joueur + outils posés dans le salon) ;
/// - `ingredients` : `(quantité, objet)` requis par la recette ;
/// - `stock` : `(objet, quantité)` détenus par le joueur, une entrée par ligne d'inventaire.
///
/// Si un même objet apparaît sur plusieurs lignes d'ingrédients, les quantités sont additionnées :
/// elles seront consommées l'une après l'autre sur la même ligne d'inventaire.
///
/// # Errors
/// `"recipe__missing_tool"` ou `"recipe__missing_ingredient"` (clés de traduction).
pub fn check_craft_requirements(
    tools_needed: &[ObjectId],
    available_tools: &[ObjectId],
    ingredients: &[(u64, ObjectId)],
    stock: &[(ObjectId, u64)],
) -> Result<(), &'static str> {
    if tools_needed.iter().any(|tool| !available_tools.contains(tool)) {
        return Err("recipe__missing_tool");
    }

    let mut required: Vec<(ObjectId, u64)> = Vec::new();
    for (qty, item_id) in ingredients {
        match required.iter_mut().find(|(id, _)| id == item_id) {
            Some((_, total)) => *total = total.saturating_add(*qty),
            None => required.push((*item_id, *qty)),
        }
    }

    let enough = required.iter().all(|(item_id, needed)| {
        stock.iter().any(|(id, available)| id == item_id && available >= needed)
    });
    if !enough {
        return Err("recipe__missing_ingredient");
    }

    Ok(())
}

fn craft_job() -> timer::Job {
    timer::job(process_due_crafts)
}

/// Finalise les crafts échus et retourne le délai avant le prochain.
async fn process_due_crafts() -> Option<u64> {
    let completed = {
        let mut crafts = CRAFTS.lock().await;
        let (finished, pending) = split_finished(std::mem::take(&mut *crafts), now_secs());
        *crafts = pending;
        finished
    };

    for craft in completed {
        if let Err(e) = finalize_craft(craft, false).await {
            eprintln!("[craft] cannot finalize a craft: {e:?}");
        }
    }

    let crafts = CRAFTS.lock().await;
    next_wakeup_delay(&crafts, now_secs())
}

async fn finalize_craft(mut craft: PlayerCraft, is_late: bool) -> Result<(), crate::discord::poise_structs::Error> {
    let recipe = Recipe::get_by_universe(craft.universe_id).await?
        .into_iter().find(|r| r._id == Some(craft.recipe_id))
        .ok_or("recipe__not_found")?;

    let character = crate::database::characters::get_character_by_user_id(craft.universe_id, craft.user_id).await?
        .ok_or("recipe__character_not_found")?;

    let db_client = crate::database::db_client::get_db_client().await;
    let mut session = db_client.start_session().await?;
    session.start_transaction().await?;

    let result: Result<(), crate::discord::poise_structs::Error> = async {
        // Production des items
        for (qty, item_id) in &recipe.result {
            Inventory::add_item_to_inventory_with_session(
                &mut session,
                craft.universe_id,
                character._id,
                HolderType::Character,
                *item_id,
                *qty
            ).await?;
        }

        // Marquer comme fini et supprimer de la DB
        craft.is_finished = true;
        let removed = craft.remove_with_session(&mut session).await?;
        timer::ensure_claimed(removed.deleted_count)?;
        Ok(())
    }.await;

    if let Err(e) = result {
        session.abort_transaction().await?;
        return Err(e);
    }

    session.commit_transaction().await?;

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

    // Ne repousse jamais un réveil plus proche déjà prévu pour un autre craft.
    CRAFT_TIMER.schedule(remaining_delay_secs(player_craft.end_timestamp, now_secs()), craft_job());

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

/// Recharge la file des crafts depuis la base. Appelé à chaque `ready` Discord, donc aussi après
/// une reconnexion : l'ancien minuteur est arrêté, le traitement éventuellement en cours se termine
/// d'abord, et chaque craft n'est finalisé qu'une fois.
pub async fn setup() {
    // Plus aucun nouveau traitement ne démarre ; on attend celui qui serait en cours.
    CRAFT_TIMER.stop();
    let _exclusive = CRAFT_TIMER.exclusive().await;
    // Gardé jusqu'au remplacement de la file : un craft lancé pendant la relecture n'est pas perdu.
    let mut crafts = CRAFTS.lock().await;

    let universes = match crate::database::universe::Universe::get_all_universes().await {
        Ok(u) => u,
        Err(e) => {
            eprintln!("Impossible d'initialiser la file de craft : erreur lors de la récupération des univers : {:?}", e);
            // On reprend la file en mémoire telle quelle plutôt que de tout arrêter.
            CRAFT_TIMER.restart(next_wakeup_delay(&crafts, now_secs()), craft_job());
            return;
        }
    };

    let mut all_active_crafts = Vec::new();
    for universe in universes {
        match PlayerCraft::get_active_crafts(universe.universe_id).await {
            Ok(active_crafts) => all_active_crafts.extend(active_crafts),
            Err(e) => {
                eprintln!("Impossible d'initialiser la file de craft : erreur lors de la récupération des crafts pour l'univers {:?} : {:?}", universe.universe_id, e);
                CRAFT_TIMER.restart(next_wakeup_delay(&crafts, now_secs()), craft_job());
                return;
            }
        }
    }

    let now = now_secs();
    let (finished, pending) = split_finished(all_active_crafts, now);

    // Crafts terminés pendant que le bot était éteint (ou déconnecté).
    for craft in finished {
        if let Err(e) = finalize_craft(craft, true).await {
            eprintln!("[craft] cannot finalize a recovered craft: {e:?}");
        }
    }

    *crafts = pending;
    let next_delay = next_wakeup_delay(&crafts, now_secs());
    CRAFT_TIMER.restart(next_delay, craft_job());
    println!("File de craft initialisée ({} craft(s) en attente).", crafts.len());
}

#[cfg(test)]
mod tests {
    use super::*;

    fn oid() -> ObjectId {
        ObjectId::new()
    }

    fn craft_ending_at(end: u64) -> PlayerCraft {
        PlayerCraft { end_timestamp: end, ..Default::default() }
    }

    // ---- délais ----

    #[test]
    fn end_timestamp_is_start_plus_delay() {
        assert_eq!(craft_end_timestamp(1_000, 60), 1_060);
        assert_eq!(craft_end_timestamp(1_000, 0), 1_000);
    }

    #[test]
    fn end_timestamp_saturates_instead_of_overflowing() {
        assert_eq!(craft_end_timestamp(u64::MAX - 1, 100), u64::MAX);
    }

    #[test]
    fn new_player_craft_records_the_schedule() {
        let (universe, recipe) = (oid(), oid());
        let c = new_player_craft(universe, 42, 7, recipe, 300, 5_000);
        assert_eq!(c.universe_id, universe);
        assert_eq!(c.recipe_id, recipe);
        assert_eq!((c.user_id, c.server_id), (42, 7));
        assert_eq!((c.start_timestamp, c.end_timestamp), (5_000, 5_300));
        assert!(!c.is_finished);
    }

    #[test]
    fn remaining_delay_is_never_negative() {
        assert_eq!(remaining_delay_secs(1_500, 1_000), 500);
        assert_eq!(remaining_delay_secs(1_000, 1_000), 0);
        assert_eq!(remaining_delay_secs(900, 1_000), 0);
    }

    #[test]
    fn split_finished_separates_done_and_pending_crafts() {
        let crafts = vec![craft_ending_at(900), craft_ending_at(1_000), craft_ending_at(1_001), craft_ending_at(5_000)];
        let (finished, pending) = split_finished(crafts, 1_000);
        let ends = |v: &[PlayerCraft]| v.iter().map(|c| c.end_timestamp).collect::<Vec<_>>();
        // un craft qui finit pile à `now` est terminé
        assert_eq!(ends(&finished), vec![900, 1_000]);
        assert_eq!(ends(&pending), vec![1_001, 5_000]);
    }

    #[test]
    fn split_finished_on_an_empty_queue() {
        let (finished, pending) = split_finished(vec![], 1_000);
        assert!(finished.is_empty() && pending.is_empty());
    }

    #[test]
    fn next_wakeup_targets_the_earliest_craft() {
        let crafts = vec![craft_ending_at(5_000), craft_ending_at(1_200), craft_ending_at(3_000)];
        assert_eq!(next_wakeup_delay(&crafts, 1_000), Some(200));
    }

    #[test]
    fn next_wakeup_is_immediate_when_a_craft_is_overdue() {
        let crafts = vec![craft_ending_at(900), craft_ending_at(5_000)];
        assert_eq!(next_wakeup_delay(&crafts, 1_000), Some(0));
    }

    #[test]
    fn next_wakeup_is_none_without_crafts() {
        assert_eq!(next_wakeup_delay(&[], 1_000), None);
    }

    // ---- prérequis ----

    #[test]
    fn requirements_are_met_with_enough_stock_and_tools() {
        let (wood, hammer) = (oid(), oid());
        assert_eq!(
            check_craft_requirements(&[hammer], &[hammer, oid()], &[(3, wood)], &[(wood, 3)]),
            Ok(())
        );
    }

    #[test]
    fn a_recipe_without_requirements_is_always_possible() {
        assert_eq!(check_craft_requirements(&[], &[], &[], &[]), Ok(()));
    }

    #[test]
    fn a_missing_tool_is_reported_first() {
        let (wood, hammer) = (oid(), oid());
        // ingrédient manquant ET outil manquant : l'outil est signalé en premier
        assert_eq!(
            check_craft_requirements(&[hammer], &[], &[(1, wood)], &[]),
            Err("recipe__missing_tool")
        );
    }

    #[test]
    fn missing_ingredient_is_reported() {
        let wood = oid();
        assert_eq!(
            check_craft_requirements(&[], &[], &[(1, wood)], &[]),
            Err("recipe__missing_ingredient")
        );
    }

    #[test]
    fn insufficient_quantity_is_reported() {
        let wood = oid();
        assert_eq!(
            check_craft_requirements(&[], &[], &[(5, wood)], &[(wood, 4)]),
            Err("recipe__missing_ingredient")
        );
        assert_eq!(
            check_craft_requirements(&[], &[], &[(5, wood)], &[(wood, 5)]),
            Ok(())
        );
    }

    #[test]
    fn the_same_ingredient_listed_twice_is_summed() {
        let wood = oid();
        let ingredients = [(2, wood), (3, wood)];
        // 4 < 2 + 3 : chaque ligne passerait seule, mais pas la recette
        assert_eq!(
            check_craft_requirements(&[], &[], &ingredients, &[(wood, 4)]),
            Err("recipe__missing_ingredient")
        );
        assert_eq!(
            check_craft_requirements(&[], &[], &ingredients, &[(wood, 5)]),
            Ok(())
        );
    }

    #[test]
    fn each_ingredient_is_checked_against_its_own_stock() {
        let (wood, stone) = (oid(), oid());
        let ingredients = [(2, wood), (2, stone)];
        // beaucoup de bois ne compense pas l'absence de pierre
        assert_eq!(
            check_craft_requirements(&[], &[], &ingredients, &[(wood, 100)]),
            Err("recipe__missing_ingredient")
        );
        assert_eq!(
            check_craft_requirements(&[], &[], &ingredients, &[(wood, 2), (stone, 2)]),
            Ok(())
        );
    }
}

/// Test d'intégration MongoDB : reconnexions répétées pendant qu'un craft est terminé.
#[cfg(test)]
mod reconnect_integration {
    use super::*;
    use crate::database::characters::Character;
    use crate::database::db_client::get_db_client;
    use crate::database::db_namespace::*;
    use crate::database::recipe::RECIPE_COLLECTION_NAME;
    use crate::database::universe::Universe;
    use crate::test_support::{block_on, random_id};
    use mongodb::bson::doc;

    #[test]
    #[ignore = "integration test: needs a running MongoDB replica set (MONGODB_* env vars); run with --ignored"]
    fn concurrent_setups_finalize_a_finished_craft_once() {
        block_on(async {
            let db = get_db_client().await.database(VERSEENGINE_DB_NAME);
            let universe_id = ObjectId::new();
            let user_id = random_id();
            let item_id = ObjectId::new();
            let recipe_id = ObjectId::new();
            let character_id = ObjectId::new();

            let universe = Universe {
                universe_id,
                name: "reconnect test".into(),
                creator_id: random_id(),
                global_time_modifier: 100,
                time_origin_timestamp: 0,
                creation_timestamp: 0,
            };
            universe.insert_universe().await.unwrap();
            Recipe {
                _id: Some(recipe_id),
                universe_id,
                tool_id: None,
                ingredients: vec![],
                result: vec![(3, item_id)],
                tools_needed: vec![],
                delay: 10,
                raw_text: String::new(),
                recipe_name: "test".into(),
                wiki_posts_id: None,
            }
            .save()
            .await
            .unwrap();
            db.collection::<Character>(CHARACTERS_COLLECTION_NAME)
                .insert_one(Character { _id: character_id, user_id, universe_id, name: "test".into(), stats: vec![], description: None, image: None })
                .await
                .unwrap();
            // Craft terminé pendant que le bot était déconnecté.
            new_player_craft(universe_id, user_id, 1, recipe_id, 10, now_secs() - 60).upsert().await.unwrap();

            // Copie en mémoire de l'ancien minuteur, faite avant la reconnexion.
            let stale_copy = PlayerCraft::get_active_crafts(universe_id).await.unwrap();
            assert_eq!(stale_copy.len(), 1);

            // Trois `ready` rapprochés (reconnexions) qui relancent chacun le setup.
            tokio::join!(setup(), setup(), setup());

            // L'ancien minuteur se réveille ensuite et finalise sa copie : rien ne doit être donné.
            let late = finalize_craft(stale_copy.into_iter().next().unwrap(), false).await;
            assert!(late.is_err(), "a craft already finalized is refused");

            let inventory = Inventory::get_by_character_id(universe_id, character_id).await.unwrap();
            let remaining = PlayerCraft::get_active_crafts(universe_id).await.unwrap();

            for (collection, filter) in [
                (UNIVERSES_COLLECTION_NAME, doc! {"_id": universe_id}),
                (RECIPE_COLLECTION_NAME, doc! {"universe_id": universe_id}),
                (CHARACTERS_COLLECTION_NAME, doc! {"universe_id": universe_id}),
                (INVENTORY_COLLECTION_NAME, doc! {"universe_id": universe_id}),
                (CRAFTS_COLLECTION_NAME, doc! {"universe_id": universe_id}),
            ] {
                db.collection::<mongodb::bson::Document>(collection).delete_many(filter).await.unwrap();
            }

            let given: u64 = inventory.iter().filter(|i| i.item_id == item_id).map(|i| i.quantity).sum();
            assert_eq!(given, 3, "the recipe result is given exactly once");
            assert!(remaining.is_empty(), "the craft is removed");
        });
    }
}
