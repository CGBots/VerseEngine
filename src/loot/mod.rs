pub mod logic;
use crate::database::db_client::get_db_client;
use crate::database::items::get_item_by_name;
use crate::database::inventory::{Inventory, HolderType};
use rand::RngExt;
use crate::database::characters::get_character_by_user_id;
use crate::database::loot_tables::{get_loot_table_by_channel_id, LootTable, LootTableEntry};
use crate::database::server::get_server_by_id;
use crate::database::universe::{get_universe_by_server_id, get_servers_from_universe};
use crate::discord::poise_structs::{Context, Error};
use crate::utility::reply::reply_with_args_and_ephemeral;
use fluent::FluentArgs;
use serenity::all::ChannelId;
use futures::TryStreamExt;
use crate::database::loot::PlayerLoot;
use crate::loot::logic::{add_loot, stop_loot};
use std::time::{SystemTime, UNIX_EPOCH};

#[poise::command(slash_command, subcommands("search", "stop"), subcommand_required, guild_only, rename = "loot")]
pub async fn loot(_ctx: Context<'_>) -> Result<(), Error> { Ok(()) }

#[poise::command(slash_command, guild_only, rename = "loot_search")]
pub async fn search(ctx: Context<'_>) -> Result<(), Error> {
    match _loot(ctx).await? {
        Some(args) => {
            reply_with_args_and_ephemeral(ctx, Ok("loot_table__loot_success"), Some(args), true).await?;
        }
        None => {}
    }
    Ok(())
}

#[poise::command(slash_command, guild_only, rename = "loot_stop")]
pub async fn stop(ctx: Context<'_>) -> Result<(), Error> {
    let server = match get_server_by_id(ctx.guild_id().unwrap().get()).await {
        Ok(Some(s)) => s,
        _ => return Err("loot_table__server_not_found".into()),
    };

    match stop_loot(server.universe_id, ctx.author().id.get()).await {
        Ok(Some(_)) => {
            let _ = reply_with_args_and_ephemeral(ctx, Ok("loot_table__stopped"), None, true).await;
            Ok(())
        },
        _ => {
            let _ = reply_with_args_and_ephemeral(ctx, Ok("loot_table__not_in_loot"), None, true).await;
            Ok(())
        }
    }
}

pub async fn _loot(ctx: Context<'_>) -> Result<Option<FluentArgs<'_>>, Error> {
    // check for universe
    // Check for character,
    // check channel
    // check loot tables (place and/or category or road)
    // Roll through all loots.
    // Save all loots to player => create if not exists, else, edit quantity

    let Some(channel) = ctx.guild_channel().await else {return Err("loot_table__not_in_guild".into())}; //Channel n'est pas dans une guilde

    let guild_id = ctx.guild_id().unwrap();
    let user_id = ctx.author().id;

    let universe_res = get_universe_by_server_id(guild_id.get()).await;
    let universe = match universe_res {
        Ok(Some(u)) => u,
        Ok(None) => return Err("loot_table__universe_not_found".into()),
        Err(e) => {
            eprintln!("Error fetching universe for guild {}: {:?}", guild_id, e);
            return Err(format!("error:loot_table__error_fetching_universe:{}", e).into());
        }
    };

    let server = get_server_by_id(guild_id.get()).await?.ok_or("loot_table__server_not_found")?;

    // Check if player is already moving or crafting or looting
    if let Ok(Some(m)) = server.clone().get_player_move(user_id.get()).await {
        if m.is_in_move {
            return Err("loot_table__already_moving".into());
        }
    }

    if let Ok(Some(_)) = crate::database::craft::PlayerCraft::get_by_user_id(universe.universe_id, user_id.get()).await {
        return Err("loot_table__already_crafting".into());
    }

    if let Ok(Some(_)) = PlayerLoot::get_by_user_id(universe.universe_id, user_id.get()).await {
        return Err("loot_table__already_looting".into());
    }

    let character_res = get_character_by_user_id(universe.universe_id, user_id.get()).await;
    let character = match character_res {
        Ok(Some(c)) => c,
        Ok(None) => return Err("loot_table__character_not_found".into()),
        Err(e) => {
            eprintln!("Error fetching character for user {}: {:?}", user_id, e);
            return Err(format!("error:loot_table__error_fetching_character:{}", e).into());
        }
    };

    let channel_loot_table_res = get_loot_table_by_channel_id(universe.universe_id, channel.id.get()).await;
    let channel_loot_table = match channel_loot_table_res {
        Ok(t) => t,
        Err(e) => {
            eprintln!("Error fetching channel loot table for universe {} and channel {}: {:?}", universe.universe_id, channel.id, e);
            return Err(format!("error:loot_table__error_fetching_channel_table:{}", e).into());
        }
    };

    let category_loot_table = match channel.parent_id{
        None => {None}
        Some(category_id) => {
            let Some(server) = get_server_by_id(guild_id.get()).await? else {return Err("loot_table__server_not_found".into())};
            if server.contains_id(channel.id.get()){
                return Err("loot_table__setup_channel".into()); //Channel corresponding to a setup channel.
            }
            let category_loot_table_res = get_loot_table_by_channel_id(universe.universe_id, category_id.get()).await;
            match category_loot_table_res {
                Ok(t) => t,
                Err(e) => {
                    eprintln!("Error fetching category loot table for universe {} and category {}: {:?}", universe.universe_id, category_id, e);
                    return Err(format!("error:loot_table__error_fetching_category_table:{}", e).into());
                }
            }
        }
    };
    let mut all_looted_items = Vec::new();
    let mut total_delay = 0;

    let mut channel_loot_table_final = None;
    if let Some(mut channel_table) = channel_loot_table {
        if let Some(limit) = channel_table.rate_limit {
            if let Some(last_loots) = &channel_table.last_loot {
                if let Some(last_loot_time) = last_loots.get(&character._id.to_string()) {
                    let now = chrono::Utc::now();
                    let elapsed = now.signed_duration_since(*last_loot_time).num_seconds() as u64;
                    if elapsed < limit {
                        return Err(format!("error:loot_table__rate_limited:{}", limit - elapsed).into());
                    }
                }
            }
        }
        
        if let Some(d) = channel_table.delay {
            total_delay = total_delay.max(d);
        }

        let (items, updated) = channel_table.roll();
        if channel_table.entries.is_empty() {
            // Notification dans les logs de l'univers
            let mut servers_cursor = get_servers_from_universe(&universe.universe_id).await?;
            let mut log_args = FluentArgs::new();
            log_args.set("channel_id", channel.id.get());
            
            while let Some(server) = servers_cursor.try_next().await? {
                if let Some(log_channel_id) = server.log_channel_id {
                    let _ = ChannelId::new(log_channel_id.id).send_message(&ctx, 
                        serenity::all::CreateMessage::new().content(
                            crate::translation::get(ctx, "loot_table__deleted_log", None, Some(&log_args))
                        )
                    ).await;
                }
            }
            channel_loot_table_final = Some(channel_table);
        } else if updated || channel_table.rate_limit.is_some() {
            if channel_table.rate_limit.is_some() {
                let mut last_loots = channel_table.last_loot.unwrap_or_default();
                last_loots.insert(character._id.to_string(), chrono::Utc::now());
                channel_table.last_loot = Some(last_loots);
            }
            channel_loot_table_final = Some(channel_table);
        }
        all_looted_items.extend(items);
    }

    let mut category_loot_table_final = None;
    if let Some(mut category_table) = category_loot_table {
        if let Some(limit) = category_table.rate_limit {
            if let Some(last_loots) = &category_table.last_loot {
                if let Some(last_loot_time) = last_loots.get(&character._id.to_string()) {
                    let now = chrono::Utc::now();
                    let elapsed = now.signed_duration_since(*last_loot_time).num_seconds() as u64;
                    if elapsed < limit {
                        if all_looted_items.is_empty() {
                            return Err(format!("error:loot_table__rate_limited:{}", limit - elapsed).into());
                        }
                    }
                }
            }
        }

        if let Some(d) = category_table.delay {
            total_delay = total_delay.max(d);
        }

        let (items, updated) = category_table.roll();
        if category_table.entries.is_empty() {
            // Notification dans les logs de l'univers
            let mut servers_cursor = get_servers_from_universe(&universe.universe_id).await?;
            let mut log_args = FluentArgs::new();
            let category_id = channel.parent_id.unwrap().get();
            log_args.set("channel_id", category_id);

            while let Some(server) = servers_cursor.try_next().await? {
                if let Some(log_channel_id) = server.log_channel_id {
                    let _ = ChannelId::new(log_channel_id.id).send_message(&ctx,
                        serenity::all::CreateMessage::new().content(
                            crate::translation::get(ctx, "loot_table__deleted_log", None, Some(&log_args))
                        )
                    ).await;
                }
            }
            category_loot_table_final = Some(category_table);
        } else if updated || category_table.rate_limit.is_some() {
            if category_table.rate_limit.is_some() {
                let mut last_loots = category_table.last_loot.unwrap_or_default();
                last_loots.insert(character._id.to_string(), chrono::Utc::now());
                category_table.last_loot = Some(last_loots);
            }
            category_loot_table_final = Some(category_table);
        }
        all_looted_items.extend(items);
    }

    if all_looted_items.is_empty() {
        return Err("loot_table__no_loot_found".into());
    }

    if total_delay > 0 {
        let client = get_db_client().await;
        let mut session = client.start_session().await?;
        session.start_transaction().await?;

        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
        let player_loot = PlayerLoot {
            _id: None,
            universe_id: universe.universe_id,
            user_id: user_id.get(),
            server_id: guild_id.get(),
            items: all_looted_items,
            start_timestamp: now,
            end_timestamp: now + total_delay,
            is_finished: false,
        };

        let result: Result<(), Error> = async {
            if let Some(channel_table) = &channel_loot_table_final {
                if channel_table.entries.is_empty() {
                    channel_table.delete_with_session(&mut session).await?;
                } else {
                    channel_table.save_or_update_with_session(&mut session).await?;
                }
            }
            if let Some(category_table) = &category_loot_table_final {
                if category_table.entries.is_empty() {
                    category_table.delete_with_session(&mut session).await?;
                } else {
                    category_table.save_or_update_with_session(&mut session).await?;
                }
            }
            add_loot(ctx.serenity_context().http.clone(), player_loot).await?;
            Ok(())
        }.await;

        match result {
            Ok(_) => {
                session.commit_transaction().await?;
                let mut args = FluentArgs::new();
                args.set("delay", total_delay);
                reply_with_args_and_ephemeral(ctx, Ok("loot_table__loot_started"), Some(args), true).await?;
                return Ok(None);
            }
            Err(e) => {
                session.abort_transaction().await?;
                return Err(e);
            }
        }
    }

    let mut item_counts = std::collections::BTreeMap::new();
    for item_name in all_looted_items {
        *item_counts.entry(item_name).or_insert(0) += 1;
    }

    let mut item_list = Vec::new();

    let client = get_db_client().await;
    let mut session = client.start_session().await?;
    session.start_transaction().await?;

    let result: Result<Option<FluentArgs<'_>>, Error> = async {
        // Mettre à jour les tables de loot si nécessaire
        if let Some(channel_table) = &channel_loot_table_final {
            if channel_table.entries.is_empty() {
                channel_table.delete_with_session(&mut session).await?;
            } else {
                channel_table.save_or_update_with_session(&mut session).await?;
            }
        }
        if let Some(category_table) = &category_loot_table_final {
            if category_table.entries.is_empty() {
                category_table.delete_with_session(&mut session).await?;
            } else {
                category_table.save_or_update_with_session(&mut session).await?;
            }
        }

        for (item_name, quantity) in item_counts {
            let item_data_res = get_item_by_name(universe.universe_id, &item_name).await;
            let item_data = match item_data_res {
                Ok(Some(i)) => Some(i),
                Ok(None) => None,
                Err(e) => {
                    eprintln!("Error fetching item '{}' for universe {}: {:?}", item_name, universe.universe_id, e);
                    let mut err_args = FluentArgs::new();
                    err_args.set("item_name", item_name);
                    err_args.set("quantity", quantity);
                    err_args.set("error", e.to_string());
                    item_list.push(crate::translation::get(ctx, "loot_table__item_db_error", None, Some(&err_args)));
                    continue;
                }
            };

            if let Some(item_data) = item_data {
                let inventory_id = Inventory::add_item_to_inventory_with_session(
                    &mut session,
                    universe.universe_id,
                    character._id,
                    HolderType::Character,
                    item_data._id,
                    quantity as u64
                ).await;

                let id_str = match inventory_id {
                    Ok(id) => id.to_hex(),
                    Err(e) => {
                        eprintln!("Error adding item to inventory for character {}: {:?}", character._id, e);
                        "N/A".to_string()
                    }
                };

                item_list.push(format!("- {}x {} (ID: `{}`)", quantity, item_name, id_str));
            } else {
                let mut line_args = FluentArgs::new();
                line_args.set("item_name", item_name);
                line_args.set("quantity", quantity);
                item_list.push(crate::translation::get(ctx, "loot_table__item_not_found", None, Some(&line_args)));
            }
        }

        let mut args = FluentArgs::new();
        args.set("items", format!("\n{}", item_list.join("\n")));
        Ok(Some(args))
    }.await;

    match result {
        Ok(args) => {
            session.commit_transaction().await?;
            Ok(args)
        }
        Err(e) => {
            session.abort_transaction().await?;
            Err(e)
        }
    }
}

impl LootTable {
    pub fn roll(&mut self) -> (Vec<String>, bool) {
        let mut rng = rand::rng();
        let mut rolled_items = Vec::new();
        let mut updated = false;

        for entry in self.entries.iter_mut() {
            if entry.is_out_of_stock() {
                continue;
            }

            match entry {
                LootTableEntry::Item(item) => {
                    if rng.random_range(0.0..100.0) <= item.probability {
                        let quantity = rng.random_range(item.min..=item.max);
                        for _ in 0..quantity {
                            rolled_items.push(item.name.clone());
                        }

                        if item.decrement_stock() {
                            updated = true;
                        }
                    }
                }
                LootTableEntry::Set(set) => {
                    if rng.random_range(0.0..100.0) <= set.probability {
                        let num_picks = rng.random_range(set.min..=set.max);

                        for _ in 0..num_picks {
                            let total_weight: f64 = set.items.iter()
                                .filter(|i| !i.is_out_of_stock())
                                .map(|i| i.probability)
                                .sum();

                            if total_weight > 0.0 {
                                let mut weight_roll = rng.random_range(0.0..total_weight);
                                if let Some(item) = set.items.iter_mut()
                                    .filter(|i| !i.is_out_of_stock())
                                    .find(|i| {
                                        if weight_roll < i.probability {
                                            true
                                        } else {
                                            weight_roll -= i.probability;
                                            false
                                        }
                                    })
                                {
                                    let quantity = rng.random_range(item.min..=item.max);
                                    for _ in 0..quantity {
                                        rolled_items.push(item.name.clone());
                                    }

                                    if item.decrement_stock() {
                                        updated = true;
                                    }
                                }
                            }
                        }

                        let old_items_len = set.items.len();
                        set.items.retain(|i| !i.is_out_of_stock());
                        if set.items.len() != old_items_len {
                            updated = true;
                        }

                        if set.decrement_stock() {
                            updated = true;
                        }
                    }
                }
            }
        }

        let old_len = self.entries.len();
        self.entries.retain(|e| {
            if e.is_out_of_stock() {
                return false;
            }
            if let LootTableEntry::Set(s) = e {
                if s.items.is_empty() {
                    return false;
                }
            }
            true
        });
        if self.entries.len() != old_len {
            updated = true;
        }

        (rolled_items, updated)
    }
}

#[cfg(test)]
mod tests {
    use mongodb::bson::oid::ObjectId;
    use crate::database::loot_tables::{LootTable, LootTableEntry, LootTableItem, LootTableSet};

    #[test]
    fn test_roll_min_max_inclusive() {
        // Test Item
        for _ in 0..100 {
            let mut found_item_1 = false;
            let mut found_item_2 = false;
            let mut table = LootTable {
                _id: None,
                universe_id: ObjectId::new(),
                channel_id: 123,
                entries: vec![LootTableEntry::Item(LootTableItem {
                    name: "I".to_string(),
                    probability: 100.0,
                    min: 1,
                    max: 2,
                    stock: None,
                    secret: false,
                })],
                raw_text: "".to_string(),
                rate_limit: None,
                delay: None,
                last_loot: None,
            };

            for _ in 0..1000 {
                let (items, _) = table.roll();
                if items.len() == 1 { found_item_1 = true; }
                if items.len() == 2 { found_item_2 = true; }
                if found_item_1 && found_item_2 { break; }
            }
            assert!(found_item_1, "Item min (1) non tiré");
            assert!(found_item_2, "Item max (2) non tiré");
        }

        // Test Set Picks
        for _ in 0..100 {
            let mut found_set_picks_1 = false;
            let mut found_set_picks_2 = false;
            let mut table = LootTable {
                _id: None,
                universe_id: ObjectId::new(),
                channel_id: 123,
                entries: vec![LootTableEntry::Set(LootTableSet {
                    name: "S".to_string(),
                    probability: 100.0,
                    min: 1,
                    max: 2,
                    stock: None,
                    items: vec![LootTableItem {
                        name: "SI".to_string(),
                        probability: 100.0,
                        min: 1,
                        max: 1,
                        stock: None,
                        secret: false,
                    }],
                    secret: false,
                })],
                raw_text: "".to_string(),
                rate_limit: None,
                delay: None,
                last_loot: None,
            };

            for _ in 0..1000 {
                let (items, _) = table.roll();
                if items.len() == 1 { found_set_picks_1 = true; }
                if items.len() == 2 { found_set_picks_2 = true; }
                if found_set_picks_1 && found_set_picks_2 { break; }
            }
            assert!(found_set_picks_1, "Set picks min (1) non tiré");
            assert!(found_set_picks_2, "Set picks max (2) non tiré");
        }

        // Test Set Item Qty
        for _ in 0..100 {
            let mut found_set_item_qty_1 = false;
            let mut found_set_item_qty_2 = false;
            let mut table = LootTable {
                _id: None,
                universe_id: ObjectId::new(),
                channel_id: 123,
                entries: vec![LootTableEntry::Set(LootTableSet {
                    name: "S".to_string(),
                    probability: 100.0,
                    min: 1,
                    max: 1,
                    stock: None,
                    items: vec![LootTableItem {
                        name: "SI".to_string(),
                        probability: 100.0,
                        min: 1,
                        max: 2,
                        stock: None,
                        secret: false,
                    }],
                    secret: false,
                })],
                raw_text: "".to_string(),
                rate_limit: None,
                delay: None,
                last_loot: None,
            };

            for _ in 0..1000 {
                let (items, _) = table.roll();
                if items.len() == 1 { found_set_item_qty_1 = true; }
                if items.len() == 2 { found_set_item_qty_2 = true; }
                if found_set_item_qty_1 && found_set_item_qty_2 { break; }
            }
            assert!(found_set_item_qty_1, "Set item qty min (1) non tiré");
            assert!(found_set_item_qty_2, "Set item qty max (2) non tiré");
        }
    }

    #[test]
    fn test_roll_item_no_stock() {
        let mut table = LootTable {
            _id: None,
            universe_id: ObjectId::new(),
            channel_id: 123,
            entries: vec![LootTableEntry::Item(LootTableItem {
                name: "Gold".to_string(),
                probability: 100.0,
                min: 10,
                max: 10,
                stock: None,
                secret: false,
            })],
            raw_text: "".to_string(),
            rate_limit: None,
            delay: None,
            last_loot: None,
        };

        let (items, updated) = table.roll();
        assert_eq!(items.len(), 10);
        assert_eq!(items[0], "Gold");
        assert!(!updated); // No stock, so no update
    }

    #[test]
    fn test_roll_item_with_stock() {
        let mut table = LootTable {
            _id: None,
            universe_id: ObjectId::new(),
            channel_id: 123,
            entries: vec![LootTableEntry::Item(LootTableItem {
                name: "Sword".to_string(),
                probability: 100.0,
                min: 1,
                max: 1,
                stock: Some(1),
                secret: false,
            })],
            raw_text: "".to_string(),
            rate_limit: None,
            delay: None,
            last_loot: None,
        };

        let (items, updated) = table.roll();
        assert_eq!(items.len(), 1);
        assert!(updated);
        assert!(table.entries.is_empty()); // Stock reached 0, entry removed
    }

    #[test]
    fn test_roll_set_weighted() {
        let mut table = LootTable {
            _id: None,
            universe_id: ObjectId::new(),
            channel_id: 123,
            entries: vec![LootTableEntry::Set(LootTableSet {
                name: "ArmorSet".to_string(),
                probability: 100.0,
                min: 2,
                max: 2,
                stock: None,
                items: vec![
                    LootTableItem {
                        name: "Helmet".to_string(),
                        probability: 1.0, // Weight 1
                        min: 1,
                        max: 1,
                        stock: None,
                        secret: false,
                    },
                    LootTableItem {
                        name: "Boots".to_string(),
                        probability: 99.0, // Weight 99
                        min: 1,
                        max: 1,
                        stock: None,
                        secret: false,
                    },
                ],
                secret: false,
            })],
            raw_text: "".to_string(),
            rate_limit: None,
            delay: None,
            last_loot: None,
        };

        let (items, _updated) = table.roll();
        assert_eq!(items.len(), 2);
        // Statistically, it's almost always Boots, but we can't be 100% sure in one run.
        // But we check that it piocher exactly 2 items from the set as requested by min/max.
    }

    #[test]
    fn test_roll_set_stock_depletion() {
        let mut table = LootTable {
            _id: None,
            universe_id: ObjectId::new(),
            channel_id: 123,
            entries: vec![LootTableEntry::Set(LootTableSet {
                name: "LimitedSet".to_string(),
                probability: 100.0,
                min: 1,
                max: 1,
                stock: Some(1),
                items: vec![
                    LootTableItem {
                        name: "RareItem".to_string(),
                        probability: 100.0,
                        min: 1,
                        max: 1,
                        stock: None,
                        secret: false,
                    },
                ],
                secret: false,
            })],
            raw_text: "".to_string(),
            rate_limit: None,
            delay: None,
            last_loot: None,
        };

        let (items, updated) = table.roll();
        assert_eq!(items.len(), 1);
        assert!(updated);
        assert!(table.entries.is_empty()); // Set stock depleted
    }

    #[test]
    fn test_roll_set_empty_items_depletion() {
        let mut table = LootTable {
            _id: None,
            universe_id: ObjectId::new(),
            channel_id: 123,
            entries: vec![LootTableEntry::Set(LootTableSet {
                name: "One-time Set".to_string(),
                probability: 100.0,
                min: 1,
                max: 1,
                stock: None,
                items: vec![LootTableItem {
                    name: "Unique Item".to_string(),
                    probability: 100.0,
                    min: 1,
                    max: 1,
                    stock: Some(1),
                    secret: false,
                }],
                secret: false,
            })],
            raw_text: "".to_string(),
            rate_limit: None,
            delay: None,
            last_loot: None,
        };

        let (items, updated) = table.roll();
        assert_eq!(items.len(), 1);
        assert!(updated);
        assert!(table.entries.is_empty()); // Item stock reached 0 -> Set items empty -> Set removed
    }
}