//! Vues de l'inventaire, du livre de recettes, du craft et du farm pour l'activité.
//!
//! Fonctions pures : les routes (`http.rs`) lisent la base, ces fonctions assemblent la réponse.
//! Le nombre maximal d'exemplaires faisables est calculé ici, côté serveur, avec les mêmes règles
//! que le lancement du craft (`craft::logic::max_craftable`).

use std::collections::HashMap;

use mongodb::bson::oid::ObjectId;
use serde::Serialize;
use serde_json::Value;

use crate::activity::dto::stat_value_json;
use crate::craft::logic::{completed_units, max_craftable, MAX_CRAFT_QUANTITY};
use crate::database::craft::PlayerCraft;
use crate::database::inventory::Inventory;
use crate::database::items::Item;
use crate::database::loot::PlayerLoot;
use crate::database::modifiers::ModifierType;
use crate::database::recipe::Recipe;
use crate::database::stats::Stat;
use crate::item::ItemUsage;

#[derive(Debug, Serialize, PartialEq)]
pub struct EffectDto {
    pub stat: String,
    pub value: Value,
    /// `base`, `multiplier` ou `flat` (comme les statuts du personnage).
    pub modifier_type: &'static str,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct ItemCardDto {
    pub inventory_id: String,
    pub item_id: String,
    pub name: String,
    pub description: Option<String>,
    /// Informations réservées au détenteur (affichées aussi par `/item lookup`).
    pub secret: Option<String>,
    /// `consumable`, `usable`, `placeable`, `wearable` ou `none`.
    pub usage: &'static str,
    pub effects: Vec<EffectDto>,
    pub quantity: u64,
}

#[derive(Debug, Serialize)]
pub struct InventoryDto {
    pub items: Vec<ItemCardDto>,
    pub server_time: u64,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct IngredientDto {
    pub item_id: String,
    pub name: String,
    /// Quantité pour un exemplaire.
    pub quantity: u64,
    /// Quantité détenue.
    pub owned: u64,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct ProductDto {
    pub name: String,
    pub quantity: u64,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct ToolDto {
    pub name: String,
    /// Dans l'inventaire ou posé à la position du joueur.
    pub available: bool,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct RecipeDto {
    pub id: String,
    pub name: String,
    /// Durée d'un exemplaire (s).
    pub delay: u64,
    pub ingredients: Vec<IngredientDto>,
    pub results: Vec<ProductDto>,
    pub tools: Vec<ToolDto>,
    /// Exemplaires faisables maintenant (0 : recette non réalisable).
    pub max_craftable: u64,
}

#[derive(Debug, Serialize)]
pub struct RecipeBookDto {
    /// Réalisables d'abord, puis les autres ; par nom dans chaque groupe.
    pub recipes: Vec<RecipeDto>,
    pub max_quantity: u64,
    pub server_time: u64,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct CraftView {
    pub recipe_id: String,
    pub recipe_name: Option<String>,
    pub quantity: u64,
    /// Exemplaires déjà terminés.
    pub completed: u64,
    pub start_timestamp: u64,
    pub end_timestamp: u64,
}

#[derive(Debug, Serialize)]
pub struct CraftStateDto {
    pub craft: Option<CraftView>,
    /// Clé Fluent du résultat de l'action (réponses des POST uniquement).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<&'static str>,
    /// Arrêt : exemplaires livrés et exemplaires dont les ingrédients ont été rendus.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delivered: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub refunded: Option<u64>,
    pub server_time: u64,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct FarmView {
    pub start_timestamp: u64,
    pub end_timestamp: u64,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct LootedDto {
    pub name: String,
    pub quantity: u64,
    /// Nom tiré mais objet inexistant dans l'univers : rien n'a été donné.
    pub unknown: bool,
}

#[derive(Debug, Serialize)]
pub struct FinishedFarmDto {
    /// Fin de la fouille (heure serveur, s) : identifie le résultat côté client.
    pub at: u64,
    pub items: Vec<LootedDto>,
}

#[derive(Debug, Serialize)]
pub struct FarmStateDto {
    /// Fouille en cours. Le butin déjà tiré n'est pas envoyé : il reste une surprise.
    pub farm: Option<FarmView>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<&'static str>,
    /// Fouille instantanée : ce qui vient d'être obtenu.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub looted: Vec<LootedDto>,
    /// Dernière fouille terminée (il y a peu) : ce qui a été obtenu, pour la notification.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub finished: Option<FinishedFarmDto>,
    /// Temps de recharge de la zone (s), si la fouille a été refusée pour cette raison.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cooldown_secs: Option<u64>,
    pub server_time: u64,
}

pub fn usage_name(usage: &ItemUsage) -> &'static str {
    match usage {
        ItemUsage::Consumable => "consumable",
        ItemUsage::Usable => "usable",
        ItemUsage::Placeable => "placeable",
        ItemUsage::Wearable => "wearable",
        ItemUsage::None => "none",
    }
}

fn modifier_type_name(t: &ModifierType) -> &'static str {
    match t {
        ModifierType::Base => "base",
        ModifierType::Multiplier => "multiplier",
        ModifierType::Flats => "flat",
    }
}

/// Cartes de l'inventaire, triées par nom. Une ligne dont l'objet n'existe plus est ignorée.
pub fn inventory_cards(inventory: &[Inventory], items: &[Item], stats: &[Stat]) -> Vec<ItemCardDto> {
    let items: HashMap<ObjectId, &Item> = items.iter().map(|i| (i._id, i)).collect();
    let stat_names: HashMap<ObjectId, &str> = stats.iter().map(|s| (s._id, s.name.as_str())).collect();
    let mut cards: Vec<ItemCardDto> = inventory
        .iter()
        .filter(|line| line.quantity > 0)
        .filter_map(|line| {
            let item = items.get(&line.item_id)?;
            Some(ItemCardDto {
                inventory_id: line._id.map(|i| i.to_hex()).unwrap_or_default(),
                item_id: item._id.to_hex(),
                name: item.item_name.clone(),
                description: item.description.clone(),
                secret: item.secret_informations.clone(),
                usage: usage_name(&item.item_usage),
                effects: item
                    .effects
                    .iter()
                    .map(|e| EffectDto {
                        stat: stat_names.get(&e.stat_id).map(|s| s.to_string()).unwrap_or_else(|| "?".into()),
                        value: stat_value_json(&e.value),
                        modifier_type: modifier_type_name(&e.modifier_type),
                    })
                    .collect(),
                quantity: line.quantity,
            })
        })
        .collect();
    cards.sort_by_key(|c| c.name.to_lowercase());
    cards
}

/// Livre de recettes du joueur : pour chaque recette, ce qu'il a, ce qui manque et combien
/// d'exemplaires il peut lancer. `tools` : outils de l'inventaire et posés à sa position.
pub fn recipe_book(recipes: &[Recipe], item_names: &HashMap<ObjectId, String>, stock: &[(ObjectId, u64)], tools: &[ObjectId]) -> Vec<RecipeDto> {
    let name = |id: &ObjectId| item_names.get(id).cloned().unwrap_or_else(|| "?".into());
    let owned = |id: &ObjectId| stock.iter().filter(|(i, _)| i == id).map(|(_, q)| *q).max().unwrap_or(0);
    let mut book: Vec<RecipeDto> = recipes
        .iter()
        .filter_map(|r| {
            Some(RecipeDto {
                id: r._id?.to_hex(),
                name: r.recipe_name.clone(),
                delay: r.delay,
                ingredients: r
                    .ingredients
                    .iter()
                    .map(|(qty, id)| IngredientDto { item_id: id.to_hex(), name: name(id), quantity: *qty, owned: owned(id) })
                    .collect(),
                results: r.result.iter().map(|(qty, id)| ProductDto { name: name(id), quantity: *qty }).collect(),
                tools: r.tools_needed.iter().map(|id| ToolDto { name: name(id), available: tools.contains(id) }).collect(),
                max_craftable: max_craftable(&r.tools_needed, tools, &r.ingredients, stock, MAX_CRAFT_QUANTITY),
            })
        })
        .collect();
    book.sort_by_key(|r| (r.max_craftable == 0, r.name.to_lowercase()));
    book
}

pub fn craft_view(craft: &PlayerCraft, recipe_name: Option<String>, now: u64) -> CraftView {
    CraftView {
        recipe_id: craft.recipe_id.to_hex(),
        recipe_name,
        quantity: craft.quantity.max(1),
        completed: completed_units(craft, now),
        start_timestamp: craft.start_timestamp,
        end_timestamp: craft.end_timestamp,
    }
}

pub fn farm_view(loot: &PlayerLoot) -> FarmView {
    FarmView { start_timestamp: loot.start_timestamp, end_timestamp: loot.end_timestamp }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::inventory::{Holder, HolderType};
    use crate::database::modifiers::{Modifier, ModifierLevel};
    use crate::database::stats::StatValue;

    fn item(name: &str, usage: ItemUsage) -> Item {
        Item {
            _id: ObjectId::new(),
            universe_id: ObjectId::new(),
            item_name: name.into(),
            item_usage: usage,
            effects: vec![],
            description: Some(format!("about {name}")),
            secret_informations: None,
            image: None,
            wiki_post_id: None,
            inventory_id: None,
            inventory_size: 0,
        }
    }

    fn line(item_id: ObjectId, quantity: u64) -> Inventory {
        Inventory {
            _id: Some(ObjectId::new()),
            universe_id: ObjectId::new(),
            holder: Holder { holder_type: HolderType::Character, holder_id: ObjectId::new() },
            item_id,
            quantity,
        }
    }

    fn recipe(name: &str, ingredients: Vec<(u64, ObjectId)>, tools: Vec<ObjectId>) -> Recipe {
        Recipe {
            _id: Some(ObjectId::new()),
            universe_id: ObjectId::new(),
            tool_id: None,
            ingredients,
            result: vec![],
            tools_needed: tools,
            delay: 30,
            raw_text: String::new(),
            recipe_name: name.into(),
            wiki_posts_id: None,
        }
    }

    #[test]
    fn inventory_cards_show_usage_effects_and_quantity() {
        let speed = Stat {
            _id: ObjectId::new(),
            universe_id: ObjectId::new(),
            name: "vitesse".into(),
            base_value: StatValue::F64(1.0),
            formula: None,
            min: None,
            max: None,
            modifiers: vec![],
        };
        let mut potion = item("Potion", ItemUsage::Consumable);
        potion.effects.push(Modifier {
            stat_id: speed._id,
            value: StatValue::F64(1.5),
            modifier_type: ModifierType::Multiplier,
            end_timestamp: None,
            source: ObjectId::new(),
            level: ModifierLevel::Player,
        });
        let axe = item("hache", ItemUsage::Usable);
        let gone = ObjectId::new();
        let cards = inventory_cards(&[line(potion._id, 3), line(axe._id, 1), line(gone, 2), line(axe._id, 0)], &[potion.clone(), axe], &[speed]);
        assert_eq!(cards.iter().map(|c| (c.name.as_str(), c.usage, c.quantity)).collect::<Vec<_>>(), vec![("hache", "usable", 1), ("Potion", "consumable", 3)]);
        assert_eq!(cards[1].effects, vec![EffectDto { stat: "vitesse".into(), value: serde_json::json!(1.5), modifier_type: "multiplier" }]);
        assert_eq!(cards[1].description.as_deref(), Some("about Potion"));
    }

    #[test]
    fn the_book_lists_craftable_recipes_first_with_their_maximum() {
        let (wood, hammer) = (ObjectId::new(), ObjectId::new());
        let names = HashMap::from([(wood, "Bois".to_string()), (hammer, "Marteau".to_string())]);
        let recipes = [
            recipe("Zèbre", vec![(2, wood)], vec![]),
            recipe("Arc", vec![(50, wood)], vec![]),
            recipe("Chaise", vec![(3, wood)], vec![hammer]),
            recipe("Banc", vec![(1, wood)], vec![]),
        ];
        let book = recipe_book(&recipes, &names, &[(wood, 7)], &[wood]);
        let summary: Vec<_> = book.iter().map(|r| (r.name.as_str(), r.max_craftable)).collect();
        assert_eq!(summary, vec![("Banc", 7), ("Zèbre", 3), ("Arc", 0), ("Chaise", 0)]);
        let chaise = &book[3];
        assert_eq!(chaise.tools, vec![ToolDto { name: "Marteau".into(), available: false }]);
        assert_eq!(chaise.ingredients, vec![IngredientDto { item_id: wood.to_hex(), name: "Bois".into(), quantity: 3, owned: 7 }]);
    }

    #[test]
    fn the_craft_view_counts_finished_units() {
        let c = crate::craft::logic::new_player_craft(ObjectId::new(), 1, 1, ObjectId::new(), 10, 3, vec![], 100);
        let v = craft_view(&c, Some("Pain".into()), 125);
        assert_eq!((v.quantity, v.completed, v.end_timestamp), (3, 2, 130));
    }
}
