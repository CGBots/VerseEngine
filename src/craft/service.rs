//! # Service de craft partagé
//!
//! Logique de `/recipe craft` et `/recipe stop`, sans `poise::Context` : la commande slash et
//! l'API de l'activité appellent ces mêmes fonctions, donc les règles (outils, ingrédients,
//! exclusion avec le voyage et le farm) sont identiques quelle que soit l'origine.
//!
//! Les erreurs sont des clés Fluent (`recipe__…`), affichées telles quelles par le bot et
//! renvoyées par l'API.

use std::sync::Arc;

use mongodb::bson::oid::ObjectId;
use serenity::all::Http;

use crate::craft::logic::{add_craft, check_craft_requirements, new_player_craft, now_secs, scaled, stop_craft, CraftStopped, MAX_CRAFT_QUANTITY};
use crate::database::characters::get_character_by_user_id;
use crate::database::inventory::{HolderType, Inventory};
use crate::database::recipe::Recipe;
use crate::database::server::get_server_by_id;
use crate::database::tool::Tool;
use crate::database::travel::{get_group_of_user, SpaceType};
use crate::discord::poise_structs::Error;
use crate::occupation::{self, Occupation};

/// Recette choisie : par nom (commande slash) ou par identifiant (activité).
#[derive(Debug, Clone, Copy)]
pub enum RecipeRef<'a> {
    Name(&'a str),
    Id(ObjectId),
}

/// Où chercher les outils posés, en plus de ceux de l'inventaire.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolSpot {
    /// Salon de la commande slash.
    Channel(u64),
    /// Position du joueur : tout le lieu (sa catégorie) ou le salon de la route. Utilisé par
    /// l'activité, qui n'est dans aucun salon textuel.
    Position,
}

/// Craft lancé.
#[derive(Debug, Clone)]
pub struct CraftStarted {
    pub recipe_name: String,
    pub quantity: u64,
    /// Durée totale (s) ; 0 pour une recette instantanée, déjà livrée.
    pub delay: u64,
}

/// Stock d'un inventaire, au format de [`check_craft_requirements`].
pub fn stock_of(inventory: &[Inventory]) -> Vec<(ObjectId, u64)> {
    inventory.iter().map(|i| (i.item_id, i.quantity)).collect()
}

/// Outils utilisables : objets de l'inventaire et outils posés à `spot`.
pub async fn available_tools(universe_id: ObjectId, user_id: u64, inventory: &[Inventory], spot: ToolSpot) -> Result<Vec<ObjectId>, Error> {
    let mut tools: Vec<ObjectId> = inventory.iter().map(|i| i.item_id).collect();
    let placed = match spot {
        ToolSpot::Channel(channel_id) => Tool::get_by_channel_id(universe_id, channel_id).await?,
        ToolSpot::Position => match get_group_of_user(universe_id, user_id).await? {
            Some(g) if g.actual_space_type == SpaceType::Place => Tool::get_by_category_id(universe_id, g.actual_space_id).await?,
            Some(g) => Tool::get_by_channel_id(universe_id, g.actual_space_id).await?,
            None => vec![],
        },
    };
    tools.extend(placed.into_iter().map(|t| t.original_item));
    Ok(tools)
}

/// Lance `quantity` exemplaires d'une recette. Équivaut à `/recipe craft`.
///
/// Les ingrédients des `quantity` exemplaires sont consommés tout de suite ; les exemplaires sont
/// ensuite fabriqués l'un après l'autre (durée totale = délai × quantité). Une recette sans délai
/// est livrée immédiatement.
///
/// # Errors
/// `recipe__invalid_quantity`, `recipe__not_found`, `recipe__missing_tool`,
/// `recipe__missing_ingredient`, et les refus d'occupation ([`occupation::Busy::check`]).
pub async fn start(http: &Arc<Http>, guild_id: u64, user_id: u64, recipe: RecipeRef<'_>, quantity: u64, spot: ToolSpot) -> Result<CraftStarted, Error> {
    if !(1..=MAX_CRAFT_QUANTITY).contains(&quantity) {
        return Err("recipe__invalid_quantity".into());
    }
    let server = get_server_by_id(guild_id).await?.ok_or("recipe__server_not_found")?;
    let universe_id = server.universe_id;
    let character = get_character_by_user_id(universe_id, user_id).await?.ok_or("recipe__character_not_found")?;

    // Vérification puis écriture sans qu'un autre départ du même joueur s'intercale.
    let _lock = occupation::lock(universe_id, user_id).await;
    occupation::ensure_free(universe_id, user_id, Occupation::Craft).await?;

    let recipe = match recipe {
        RecipeRef::Name(name) => Recipe::get_by_name(universe_id, name).await?,
        RecipeRef::Id(id) => Recipe::get_by_id(universe_id, id).await?,
    }
    .ok_or("recipe__not_found")?;
    let recipe_id = recipe._id.ok_or("recipe__not_found")?;

    let inventory = Inventory::get_by_character_id(universe_id, character._id).await?;
    let tools = available_tools(universe_id, user_id, &inventory, spot).await?;
    let needed = scaled(&recipe.ingredients, quantity);
    check_craft_requirements(&recipe.tools_needed, &tools, &needed, &stock_of(&inventory))?;

    // Construit une seule fois : la base et la file en mémoire partagent le même horaire.
    let player_craft = new_player_craft(universe_id, user_id, server.server_id, recipe_id, recipe.delay, quantity, recipe.ingredients.clone(), now_secs());

    let db_client = crate::database::db_client::get_db_client().await;
    let mut session = db_client.start_session().await?;
    session.start_transaction().await?;
    let result: Result<(), Error> = async {
        for (qty, item_id) in &needed {
            let removed = Inventory::remove_item_from_holder_with_session(&mut session, universe_id, character._id, HolderType::Character, *item_id, *qty).await?;
            if !removed {
                return Err("recipe__error_during_consumption".into());
            }
        }
        if recipe.delay > 0 {
            player_craft.upsert_with_session(&mut session).await?;
        } else {
            for (qty, item_id) in scaled(&recipe.result, quantity) {
                Inventory::add_item_to_inventory_with_session(&mut session, universe_id, character._id, HolderType::Character, item_id, qty).await?;
            }
        }
        Ok(())
    }
    .await;
    if let Err(e) = result {
        let _ = session.abort_transaction().await;
        return Err(e);
    }
    session.commit_transaction().await?;

    let delay = if recipe.delay > 0 {
        let delay = player_craft.end_timestamp.saturating_sub(player_craft.start_timestamp);
        add_craft(http.clone(), player_craft).await?;
        delay
    } else {
        0
    };
    Ok(CraftStarted { recipe_name: recipe.recipe_name, quantity, delay })
}

/// Arrête le craft en cours : exemplaires finis livrés, ingrédients des autres rendus.
/// Équivaut à `/recipe stop`.
///
/// # Errors
/// `recipe__no_craft_in_progress`.
pub async fn stop(guild_id: u64, user_id: u64) -> Result<CraftStopped, Error> {
    let server = get_server_by_id(guild_id).await?.ok_or("recipe__server_not_found")?;
    stop_craft(server.universe_id, user_id).await?.ok_or_else(|| Error::from("recipe__no_craft_in_progress"))
}
