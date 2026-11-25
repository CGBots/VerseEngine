pub mod logic;
pub mod service;

use rand::RngExt;
use crate::database::loot_tables::{LootTable, LootTableEntry};
use crate::database::server::get_server_by_id;
use crate::discord::poise_structs::{Context, Error};
use crate::loot::service::{FarmOutcome, FarmSpot, LootLine};
use crate::utility::reply::reply_with_args_and_ephemeral;
use fluent::FluentArgs;

#[poise::command(slash_command, subcommands("search", "stop"), subcommand_required, guild_only, rename = "loot")]
pub async fn loot(_ctx: Context<'_>) -> Result<(), Error> { Ok(()) }

// Fouille la zone du salon. La logique (tirage, recharge, exclusion avec le voyage et le craft)
// est dans `service::start`, partagée avec l'activité.
#[poise::command(slash_command, guild_only, rename = "loot_search")]
pub async fn search(ctx: Context<'_>) -> Result<(), Error> {
    let Some(channel) = ctx.guild_channel().await else { return Err("loot_table__not_in_guild".into()) };
    let guild_id = ctx.guild_id().ok_or("loot_table__not_in_guild")?.get();
    let server = get_server_by_id(guild_id).await?.ok_or("loot_table__server_not_found")?;
    // Salon de configuration du serveur (logs, commandes…) : pas une zone de jeu.
    if channel.parent_id.is_some() && server.contains_id(channel.id.get()) {
        return Err("loot_table__setup_channel".into());
    }

    let spot = FarmSpot::Channel { channel_id: channel.id.get(), category_id: channel.parent_id.map(|p| p.get()) };
    let http = ctx.serenity_context().http.clone();
    let locale = ctx.locale().unwrap_or("fr").to_string();
    match service::start(&http, guild_id, ctx.author().id.get(), &locale, spot).await? {
        FarmOutcome::Started { delay, .. } => {
            let mut args = FluentArgs::new();
            args.set("delay", delay);
            reply_with_args_and_ephemeral(ctx, Ok("loot_table__loot_started"), Some(args), true).await?;
        }
        FarmOutcome::RateLimited { remaining_secs } => {
            let mut args = FluentArgs::new();
            args.set("time", remaining_secs);
            crate::utility::reply::reply_with_args(ctx, Err("loot_table__rate_limited".into()), Some(args)).await?;
        }
        FarmOutcome::Looted(lines) => {
            let text: Vec<String> = lines.into_iter().map(|line| loot_line_text(ctx, line)).collect();
            let mut args = FluentArgs::new();
            args.set("items", format!("\n{}", text.join("\n")));
            reply_with_args_and_ephemeral(ctx, Ok("loot_table__loot_success"), Some(args), true).await?;
        }
    }
    Ok(())
}

/// Ligne du message de butin, au format historique de la commande.
fn loot_line_text(ctx: Context<'_>, line: LootLine) -> String {
    match line {
        LootLine::Given { item_name, quantity, inventory_id } => {
            let id = inventory_id.map(|i| i.to_hex()).unwrap_or_else(|| "N/A".to_string());
            format!("- {quantity}x {item_name} (ID: `{id}`)")
        }
        LootLine::UnknownItem { item_name, quantity } => {
            let mut args = FluentArgs::new();
            args.set("item_name", item_name);
            args.set("quantity", quantity);
            crate::translation::get(ctx, "loot_table__item_not_found", None, Some(&args))
        }
        LootLine::DatabaseError { item_name, quantity, error } => {
            let mut args = FluentArgs::new();
            args.set("item_name", item_name);
            args.set("quantity", quantity);
            args.set("error", error);
            crate::translation::get(ctx, "loot_table__item_db_error", None, Some(&args))
        }
    }
}

#[poise::command(slash_command, guild_only, rename = "loot_stop")]
pub async fn stop(ctx: Context<'_>) -> Result<(), Error> {
    let guild_id = ctx.guild_id().ok_or("loot_table__not_in_guild")?.get();
    let key = match service::stop(guild_id, ctx.author().id.get()).await {
        Ok(_) => "loot_table__stopped",
        Err(e) if e.to_string() == "loot_table__server_not_found" => return Err(e),
        Err(_) => "loot_table__not_in_loot",
    };
    let _ = reply_with_args_and_ephemeral(ctx, Ok(key), None, true).await;
    Ok(())
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