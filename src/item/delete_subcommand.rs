use crate::database::items::{Item, get_item_by_name};
use crate::database::inventory::{Inventory, HolderType};
use crate::database::loot_tables::LootTable;
use crate::database::recipe::{Recipe, RECIPE_COLLECTION_NAME};
use crate::database::server::{get_server_by_id, Server};
use crate::database::characters::Character;
use crate::database::universe::get_universe_by_id;
use crate::discord::poise_structs::{Context, Error};
use crate::utility::reply::reply_with_args;
use fluent::FluentArgs;
use serenity::all::{CreateMessage, CreateEmbed, Colour};
use mongodb::bson::{oid::ObjectId, doc};
use futures::{TryStreamExt, StreamExt};
use crate::tr;

#[poise::command(slash_command, guild_only, required_permissions = "ADMINISTRATOR", rename = "item_delete")]
pub async fn delete(
    ctx: Context<'_>,
    name: String,
) -> Result<(), Error> {
    ctx.defer().await?;
    let guild_id = ctx.guild_id().ok_or("item_delete__not_in_guild")?.get();
    let Some(server) = get_server_by_id(guild_id).await? else {
        return Err("item_delete__server_not_found".into());
    };

    let universe_id = server.universe_id;
    let universe = get_universe_by_id(universe_id).await?.ok_or("item_delete__universe_not_found")?;
    let universe_name = universe.name.clone();

    // 1. Récupérer l'item
    let Some(item) = get_item_by_name(universe_id, &name).await? else {
        let mut args = FluentArgs::new();
        args.set("name", name);
        reply_with_args(ctx, Err("item_delete__not_found".into()), Some(args)).await?;
        return Ok(());
    };

    let item_id = item._id;

    // 1.1 Identifier les recettes affectées (mais ne pas les supprimer)
    let db_client = crate::database::db_client::get_db_client().await;
    let recipes_col = db_client
        .database(crate::database::db_namespace::VERSEENGINE_DB_NAME)
        .collection::<Recipe>(RECIPE_COLLECTION_NAME);

    let filter = doc! {
        "universe_id": universe_id,
        "$or": [
            {"ingredients": {"$elemMatch": {"1": item_id}}},
            {"result": {"$elemMatch": {"1": item_id}}},
            {"tools_needed": item_id}
        ]
    };
    
    let mut affected_recipes = Vec::new();
    let mut cursor = recipes_col.find(filter).await?;
    while let Some(recipe) = cursor.try_next().await? {
        affected_recipes.push(recipe.recipe_name);
    }

    // 2. Identifier les propriétaires pour les notifier
    let holders = Inventory::get_all_holders_by_item_id(universe_id, item_id).await?;
    
    // On notifie seulement les personnages (joueurs)
    let character_holders: Vec<ObjectId> = holders.into_iter()
        .filter(|h| matches!(h.holder.holder_type, HolderType::Character))
        .map(|h| h.holder.holder_id)
        .collect();

    let mut characters = Vec::new();
    if !character_holders.is_empty() {
        let db_client = crate::database::db_client::get_db_client().await;
        let characters_col = db_client
            .database(crate::database::db_namespace::VERSEENGINE_DB_NAME)
            .collection::<Character>(crate::database::db_namespace::CHARACTERS_COLLECTION_NAME);

        let filter = doc! {"_id": {"$in": character_holders}};
        let mut cursor = characters_col.find(filter).await?;
        while let Some(character) = cursor.try_next().await? {
            characters.push(character);
        }
    }

    // 3. Supprimer de l'inventaire et de la base de données (Processus critique sécurisé par transaction)
    let mut session = crate::database::db_client::get_db_client().await.start_session().await?;
    session.start_transaction().await?;

    let delete_item_res = Item::delete_with_session(universe_id, &name, &mut session).await;
    let remove_inv_res = Inventory::remove_all_by_item_id_with_session(&mut session, universe_id, item_id).await;
    let remove_loot_res = LootTable::remove_item_from_all_tables_with_session(universe_id, &name, &mut session).await;

    if delete_item_res.is_err() || remove_inv_res.is_err() || remove_loot_res.is_err() {
        session.abort_transaction().await?;
        // Notification dans les salons de log des serveurs de l'univers en cas d'échec critique
        let error_msg = format!("CRITICAL ERROR during item deletion: item='{}', universe='{}' (ID: {}). Database state may be inconsistent.", name, universe_name, universe_id);
        log::error!("{}", error_msg);
        
        let message = CreateMessage::new().content(format!("⚠️ **ERREUR CRITIQUE**\n{}", error_msg));
        
        // Envoyer sur le serveur actuel
        if let Some(log_channel_id) = server.log_channel_id {
            let _ = serenity::all::ChannelId::new(log_channel_id.id).send_message(ctx, message.clone()).await;
        }

        // Envoyer sur les autres serveurs de l'univers
        if let Ok(servers_cursor) = server.get_other_servers().await {
            let servers = servers_cursor.try_collect::<Vec<Server>>().await.unwrap_or_default();
            for other_server in servers {
                if let Some(log_channel_id) = other_server.log_channel_id {
                    let _ = serenity::all::ChannelId::new(log_channel_id.id).send_message(ctx, message.clone()).await;
                }
            }
        }

        return Err("item_delete__critical_error".into());
    }

    session.commit_transaction().await?;

    // 4. Si le processus a réussi, envoyer les messages et supprimer le wiki
    if !characters.is_empty() {
        // Parallélisation de l'envoi des notifications avec une limite de concurrence
        // pour respecter les rate-limits de Discord.
        let stream = futures::stream::iter(characters);
        let universe_name_clone = universe_name.clone();
        let item_name = item.item_name.clone();
        
        stream.for_each_concurrent(10, |character| {
            let item_name = item_name.clone();
            let universe_name = universe_name_clone.clone();
            async move {
                let user_id = serenity::all::UserId::new(character.user_id);
                let mut args = FluentArgs::new();
                args.set("item_name", item_name);
                args.set("character_name", character.name.clone());
                args.set("universe_name", universe_name);
                
                let notification_text = crate::translation::get(ctx, "item_delete__notification", None, Some(&args));
                
                let embed = CreateEmbed::new()
                    .title(crate::translation::get(ctx, "item_delete__notification_title", None, None))
                    .description(notification_text)
                    .colour(Colour::RED);

                if let Ok(dm_channel) = user_id.create_dm_channel(ctx).await {
                    let _ = dm_channel.send_message(ctx, CreateMessage::new().embed(embed)).await;
                }
            }
        }).await;
    }

    // 5. Supprimer le post wiki si il existe
    if let Some(wiki_channel_id) = server.rp_wiki_channel_id {
        if let Ok(wiki_channel) = ctx.http().get_channel(wiki_channel_id.id.into()).await {
            if let Some(channel) = wiki_channel.guild() {
                // On cherche le post dans le forum qui porte le nom de l'item
                if let Ok(threads) = ctx.guild_id().unwrap().get_active_threads(ctx.http()).await {
                    for thread in threads.threads {
                        if thread.name() == name {
                            let _ = thread.delete(ctx.http()).await;
                        }
                    }
                }
                if let Ok(threads) = channel.id.get_archived_public_threads(ctx.http(), None, None).await {
                    for thread in threads.threads {
                        if thread.name() == name {
                            let _ = thread.delete(ctx.http()).await;
                        }
                    }
                }
            }
        }
    }
    
    // Tentative sur les autres serveurs de l'univers
    if let Ok(servers_cursor) = server.get_other_servers().await {
        let servers = servers_cursor.try_collect::<Vec<Server>>().await.unwrap_or_default();
        for other_server in servers {
            if let Some(wiki_channel_id) = other_server.rp_wiki_channel_id {
                if let Ok(wiki_channel) = ctx.http().get_channel(wiki_channel_id.id.into()).await {
                    if let Some(channel) = wiki_channel.guild() {
                        if let Ok(threads) = serenity::all::GuildId::new(other_server.server_id).get_active_threads(ctx.http()).await {
                            for thread in threads.threads {
                                if thread.name() == name {
                                    let _ = thread.delete(ctx.http()).await;
                                }
                            }
                        }
                        if let Ok(threads) = channel.id.get_archived_public_threads(ctx.http(), None, None).await {
                            for thread in threads.threads {
                                if thread.name() == name {
                                    let _ = thread.delete(ctx.http()).await;
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    
    // 6. Succès
    let mut args = FluentArgs::new();
    args.set("name", name.clone());
    if !affected_recipes.is_empty() {
        args.set("affected_recipes_text", tr!(ctx, "item_delete__affected_recipes", affected_recipes: format!("\n- {}", affected_recipes.join("\n- "))));
    }
    else {args.set("affected_recipes_text", "")}
    reply_with_args(ctx, Ok("item_delete__success"), Some(args)).await?;

    Ok(())
}
