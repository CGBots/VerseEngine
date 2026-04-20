use std::time::{SystemTime, UNIX_EPOCH};
use fluent::FluentArgs;
use mongodb::bson::oid::ObjectId;
use crate::database::characters::get_character_by_user_id;
use crate::database::inventory::{HolderType, Inventory};
use crate::database::recipe::Recipe;
use crate::database::server::get_server_by_id;
use crate::database::tool::Tool;
use crate::database::craft::PlayerCraft;
use crate::discord::poise_structs::{Context, Error};
use crate::recipe::recipe;
use crate::utility::reply::{reply, reply_with_args_and_ephemeral};
use crate::craft::logic::add_craft;

/// Fabrique un objet à partir d'une recette.
#[poise::command(slash_command, guild_only, rename = "recipe_craft")]
pub async fn craft(
    ctx: Context<'_>,
    recipe_name: String,
) -> Result<(), Error> {
    let guild_id = ctx.guild_id().unwrap();
    let user_id = ctx.author().id.get();
    
    let server = get_server_by_id(guild_id.get()).await?.ok_or("recipe__server_not_found")?;
    let character = get_character_by_user_id(server.universe_id, user_id).await?.ok_or("recipe__character_not_found")?;
    
    // Vérifier si un mouvement est en cours
    if let Ok(Some(m)) = server.clone().get_player_move(user_id).await {
        if m.is_in_move {
            return Err("recipe__cannot_craft_while_moving".into());
        }
    }

    // Vérifier si un craft est déjà en cours
    if let Ok(Some(_)) = PlayerCraft::get_by_user_id(server.universe_id, user_id).await {
        return Err("recipe__craft_already_in_progress".into());
    }

    // Récupérer toutes les recettes de l'univers et filtrer par nom (ou par ID plus tard)
    let recipes = Recipe::get_by_universe(server.universe_id).await?;
    let recipe = recipes.into_iter().find(|r| r.recipe_name == recipe_name).ok_or("recipe__not_found")?;

    // 1. Vérifier les outils
    // On vérifie d'abord dans l'inventaire du joueur
    let player_inventory = Inventory::get_by_character_id(server.universe_id, character._id).await?;
    let mut available_tool_ids: Vec<ObjectId> = player_inventory.iter().map(|i| i.item_id).collect();

    // On vérifie aussi les outils posés dans le salon actuel
    let channel_id = ctx.channel_id().get();
    let placed_tools = Tool::get_by_channel_id(server.universe_id, channel_id).await?;
    for tool in placed_tools {
        available_tool_ids.push(tool.original_item);
    }

    for tool_needed in &recipe.tools_needed {
        if !available_tool_ids.contains(tool_needed) {
            return Err("recipe__missing_tool".into());
        }
    }

    // 2. Vérifier les ingrédients
    for (qty, item_id) in &recipe.ingredients {
        let has_ingredient = player_inventory.iter().any(|i| i.item_id == *item_id && i.quantity >= *qty);
        if !has_ingredient {
            return Err("recipe__missing_ingredient".into());
        }
    }

    // 3. Appliquer les changements (Consommation)
    for (qty, item_id) in &recipe.ingredients {
        let removed = Inventory::remove_item_from_holder(
            server.universe_id,
            character._id,
            HolderType::Character,
            *item_id,
            *qty
        ).await?;
        
        if !removed {
            // C'est un cas critique qui ne devrait pas arriver vu la vérification précédente
            return Err("recipe__error_during_consumption".into());
        }
    }

    if recipe.delay > 0 {
        // Craft avec délai
        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
        let player_craft = PlayerCraft {
            _id: ObjectId::new(),
            universe_id: server.universe_id,
            user_id: user_id,
            server_id: server.server_id,
            recipe_id: recipe._id.unwrap(),
            start_timestamp: now,
            end_timestamp: now + recipe.delay,
            is_finished: false,
        };

        add_craft(ctx.serenity_context().http.clone(), player_craft).await?;

        let mut args = FluentArgs::new();
        args.set("recipe_name", recipe.recipe_name);
        args.set("delay", recipe.delay);
        reply_with_args_and_ephemeral(ctx, Ok("recipe__craft_started"), Some(args), true).await?;
    } else {
        // Craft instantané
        for (qty, item_id) in &recipe.result {
            Inventory::add_item_to_inventory(
                server.universe_id,
                character._id,
                HolderType::Character,
                *item_id,
                *qty
            ).await?;
        }

        let mut args = FluentArgs::new();
        args.set("recipe_name", recipe.recipe_name);
        reply_with_args_and_ephemeral(ctx, Ok("recipe__craft_success"), Some(args), true).await?;
    }

    Ok(())
}
