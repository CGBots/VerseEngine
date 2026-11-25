//! # Service de farm (fouille) partagé
//!
//! Logique de `/loot search` et `/loot stop`, sans `poise::Context` : la commande slash et l'API
//! de l'activité appellent ces mêmes fonctions. Le tirage (`LootTable::roll`), les temps de
//! recharge, les stocks et l'exclusion avec le voyage et le craft sont donc identiques.
//!
//! Les erreurs sont des clés Fluent (`loot_table__…`).

use std::collections::BTreeMap;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use fluent::FluentArgs;
use futures::TryStreamExt;
use mongodb::bson::oid::ObjectId;
use serenity::all::{ChannelId, CreateMessage, Http};

use crate::database::characters::get_character_by_user_id;
use crate::database::db_client::get_db_client;
use crate::database::inventory::{HolderType, Inventory};
use crate::database::items::get_item_by_name;
use crate::database::loot::PlayerLoot;
use crate::database::loot_tables::{get_loot_table_by_channel_id, LootTable};
use crate::database::places::get_place_by_category_id;
use crate::database::server::get_server_by_id;
use crate::database::travel::{get_group_of_user, SpaceType};
use crate::database::universe::get_servers_from_universe;
use crate::discord::poise_structs::Error;
use crate::loot::logic::{add_loot, stop_loot};
use crate::occupation::{self, Occupation};
use crate::translation::get_by_locale;

/// Où fouiller : les tables de butin du salon et de sa catégorie.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FarmSpot {
    /// Salon de la commande slash et sa catégorie parente.
    Channel { channel_id: u64, category_id: Option<u64> },
    /// Position du joueur (activité) : dans un lieu, la table de sa catégorie et celle de son
    /// salon central s'il en a un ; sur une route, la table du salon de la route.
    Position,
}

/// Une ligne du butin obtenu immédiatement.
#[derive(Debug, Clone, PartialEq)]
pub enum LootLine {
    Given { item_name: String, quantity: u64, inventory_id: Option<ObjectId> },
    /// Nom présent dans la table mais aucun objet de ce nom dans l'univers.
    UnknownItem { item_name: String, quantity: u64 },
    DatabaseError { item_name: String, quantity: u64, error: String },
}

/// Résultat d'une fouille.
#[derive(Debug, Clone, PartialEq)]
pub enum FarmOutcome {
    /// Fouille à délai : le butin sera livré à `end_timestamp`.
    Started { delay: u64, end_timestamp: u64 },
    /// Fouille instantanée : butin déjà dans l'inventaire.
    Looted(Vec<LootLine>),
    /// Temps de recharge de la zone pas encore écoulé.
    RateLimited { remaining_secs: u64 },
}

/// Secondes de recharge restantes pour un personnage, `None` si la zone peut être fouillée.
pub fn cooldown_remaining(rate_limit: Option<u64>, last_loot: Option<DateTime<Utc>>, now: DateTime<Utc>) -> Option<u64> {
    let limit = rate_limit?;
    let elapsed = now.signed_duration_since(last_loot?).num_seconds().max(0) as u64;
    (elapsed < limit).then(|| limit - elapsed)
}

/// Regroupe les noms tirés (un par exemplaire) en `(nom, quantité)`, triés par nom.
pub fn count_items(names: Vec<String>) -> BTreeMap<String, u64> {
    let mut counts = BTreeMap::new();
    for name in names {
        *counts.entry(name).or_insert(0) += 1;
    }
    counts
}

async fn resolve_spot(universe_id: ObjectId, user_id: u64, spot: FarmSpot) -> Result<(Option<u64>, Option<u64>), Error> {
    match spot {
        FarmSpot::Channel { channel_id, category_id } => Ok((Some(channel_id), category_id)),
        FarmSpot::Position => {
            let group = get_group_of_user(universe_id, user_id).await?.ok_or("loot_table__character_not_found")?;
            match group.actual_space_type {
                SpaceType::Road => Ok((Some(group.actual_space_id), None)),
                SpaceType::Place => {
                    let central = get_place_by_category_id(universe_id, group.actual_space_id).await?.and_then(|p| p.central_channel_id);
                    Ok((central, Some(group.actual_space_id)))
                }
            }
        }
    }
}

/// Table vidée par le tirage : prévient les salons de log de l'univers.
async fn log_emptied_table(http: &Http, universe_id: ObjectId, table_channel_id: u64, locale: &str) -> Result<(), Error> {
    let mut servers = get_servers_from_universe(&universe_id).await?;
    let mut args = FluentArgs::new();
    args.set("channel_id", table_channel_id);
    let content = get_by_locale(locale, "loot_table__deleted_log", None, Some(&args));
    while let Some(server) = servers.try_next().await? {
        if let Some(log) = server.log_channel_id {
            let _ = ChannelId::new(log.id).send_message(http, CreateMessage::new().content(content.clone())).await;
        }
    }
    Ok(())
}

/// Tire une table et met à jour son état (stocks, dernière fouille). Retourne les objets tirés,
/// le délai de la table et la table à réécrire (`None` si rien n'a changé).
async fn roll_table(http: &Http, universe_id: ObjectId, character_key: &str, mut table: LootTable, locale: &str) -> Result<(Vec<String>, u64, Option<LootTable>), Error> {
    let delay = table.delay.unwrap_or(0);
    let (items, updated) = table.roll();
    if table.entries.is_empty() {
        log_emptied_table(http, universe_id, table.channel_id, locale).await?;
        return Ok((items, delay, Some(table)));
    }
    if updated || table.rate_limit.is_some() {
        if table.rate_limit.is_some() {
            let mut last = table.last_loot.take().unwrap_or_default();
            last.insert(character_key.to_string(), Utc::now());
            table.last_loot = Some(last);
        }
        return Ok((items, delay, Some(table)));
    }
    Ok((items, delay, None))
}

/// Réécrit (ou supprime, si vide) les tables modifiées par le tirage.
async fn save_tables(session: &mut mongodb::ClientSession, tables: &[LootTable]) -> Result<(), Error> {
    for table in tables {
        if table.entries.is_empty() {
            table.delete_with_session(session).await?;
        } else {
            table.save_or_update_with_session(session).await?;
        }
    }
    Ok(())
}

/// Fouille la zone. Équivaut à `/loot search`.
///
/// # Errors
/// `loot_table__no_loot_found`, et les refus d'occupation ([`occupation::Busy::check`]).
pub async fn start(http: &Arc<Http>, guild_id: u64, user_id: u64, locale: &str, spot: FarmSpot) -> Result<FarmOutcome, Error> {
    let server = get_server_by_id(guild_id).await?.ok_or("loot_table__server_not_found")?;
    let universe_id = server.universe_id;

    // Vérification puis écriture sans qu'un autre départ du même joueur s'intercale.
    let _lock = occupation::lock(universe_id, user_id).await;
    occupation::ensure_free(universe_id, user_id, Occupation::Farm).await?;

    let character = get_character_by_user_id(universe_id, user_id).await?.ok_or("loot_table__character_not_found")?;
    let character_key = character._id.to_string();
    let (channel_id, category_id) = resolve_spot(universe_id, user_id, spot).await?;

    let table_of = |id: Option<u64>, key: &'static str| async move {
        match id {
            Some(id) => get_loot_table_by_channel_id(universe_id, id).await.map_err(|e| {
                eprintln!("Error fetching loot table for universe {universe_id} and channel {id}: {e:?}");
                Error::from(key)
            }),
            None => Ok(None),
        }
    };
    let channel_table = table_of(channel_id, "loot_table__error_fetching_channel_table").await?;
    let category_table = table_of(category_id, "loot_table__error_fetching_category_table").await?;

    let now = Utc::now();
    let mut looted = Vec::new();
    let mut total_delay = 0;
    let mut to_save = Vec::new();

    if let Some(table) = channel_table {
        let last = table.last_loot.as_ref().and_then(|l| l.get(&character_key)).copied();
        if let Some(remaining_secs) = cooldown_remaining(table.rate_limit, last, now) {
            return Ok(FarmOutcome::RateLimited { remaining_secs });
        }
        let (items, delay, changed) = roll_table(http, universe_id, &character_key, table, locale).await?;
        total_delay = total_delay.max(delay);
        looted.extend(items);
        to_save.extend(changed);
    }
    if let Some(table) = category_table {
        let last = table.last_loot.as_ref().and_then(|l| l.get(&character_key)).copied();
        // Comme la commande d'origine : la recharge de la catégorie ne bloque que si le salon
        // n'a rien donné.
        if let Some(remaining_secs) = cooldown_remaining(table.rate_limit, last, now) {
            if looted.is_empty() {
                return Ok(FarmOutcome::RateLimited { remaining_secs });
            }
        }
        let (items, delay, changed) = roll_table(http, universe_id, &character_key, table, locale).await?;
        total_delay = total_delay.max(delay);
        looted.extend(items);
        to_save.extend(changed);
    }

    if looted.is_empty() {
        return Err("loot_table__no_loot_found".into());
    }

    let client = get_db_client().await;
    let mut session = client.start_session().await?;
    session.start_transaction().await?;

    if total_delay > 0 {
        let start = crate::craft::logic::now_secs();
        let player_loot = PlayerLoot {
            _id: None,
            universe_id,
            user_id,
            server_id: guild_id,
            items: looted,
            start_timestamp: start,
            end_timestamp: start.saturating_add(total_delay),
            is_finished: false,
        };
        let end_timestamp = player_loot.end_timestamp;
        let result: Result<(), Error> = async {
            save_tables(&mut session, &to_save).await?;
            add_loot(http.clone(), player_loot).await?;
            Ok(())
        }
        .await;
        return match result {
            Ok(()) => {
                session.commit_transaction().await?;
                Ok(FarmOutcome::Started { delay: total_delay, end_timestamp })
            }
            Err(e) => {
                let _ = session.abort_transaction().await;
                Err(e)
            }
        };
    }

    let result: Result<Vec<LootLine>, Error> = async {
        save_tables(&mut session, &to_save).await?;
        let mut lines = Vec::new();
        for (item_name, quantity) in count_items(looted) {
            match get_item_by_name(universe_id, &item_name).await {
                Ok(Some(item)) => {
                    let inventory_id = Inventory::add_item_to_inventory_with_session(&mut session, universe_id, character._id, HolderType::Character, item._id, quantity)
                        .await
                        .map_err(|e| eprintln!("Error adding item to inventory for character {}: {e:?}", character._id))
                        .ok();
                    lines.push(LootLine::Given { item_name, quantity, inventory_id });
                }
                Ok(None) => lines.push(LootLine::UnknownItem { item_name, quantity }),
                Err(e) => {
                    eprintln!("Error fetching item '{item_name}' for universe {universe_id}: {e:?}");
                    lines.push(LootLine::DatabaseError { item_name, quantity, error: e.to_string() });
                }
            }
        }
        Ok(lines)
    }
    .await;
    match result {
        Ok(lines) => {
            session.commit_transaction().await?;
            Ok(FarmOutcome::Looted(lines))
        }
        Err(e) => {
            let _ = session.abort_transaction().await;
            Err(e)
        }
    }
}

/// Arrête la fouille en cours (le butin tiré est perdu, comme `/loot stop`).
///
/// # Errors
/// `loot_table__not_in_loot`.
pub async fn stop(guild_id: u64, user_id: u64) -> Result<PlayerLoot, Error> {
    let server = get_server_by_id(guild_id).await?.ok_or("loot_table__server_not_found")?;
    stop_loot(server.universe_id, user_id).await?.ok_or_else(|| Error::from("loot_table__not_in_loot"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_cooldown_counts_down_from_the_last_search() {
        let now = Utc::now();
        let ten_ago = now - chrono::Duration::seconds(10);
        assert_eq!(cooldown_remaining(Some(60), Some(ten_ago), now), Some(50));
        assert_eq!(cooldown_remaining(Some(10), Some(ten_ago), now), None, "elapsed exactly");
        assert_eq!(cooldown_remaining(Some(5), Some(ten_ago), now), None);
    }

    #[test]
    fn no_cooldown_without_limit_or_previous_search() {
        let now = Utc::now();
        assert_eq!(cooldown_remaining(None, Some(now), now), None);
        assert_eq!(cooldown_remaining(Some(60), None, now), None);
    }

    #[test]
    fn a_last_search_in_the_future_keeps_the_full_cooldown() {
        // Horloge décalée : jamais de soustraction négative.
        let now = Utc::now();
        assert_eq!(cooldown_remaining(Some(60), Some(now + chrono::Duration::seconds(30)), now), Some(60));
    }

    #[test]
    fn rolled_names_are_counted_and_sorted() {
        let names = ["b", "a", "b", "b"].map(String::from).to_vec();
        assert_eq!(count_items(names).into_iter().collect::<Vec<_>>(), vec![("a".to_string(), 1), ("b".to_string(), 3)]);
    }
}
