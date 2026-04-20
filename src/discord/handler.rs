use poise::{async_trait};
#[allow(unused_imports)]
use poise::serenity_prelude::all::{ChannelType, CreateChannel, Context, Guild, Ready};
use poise::serenity_prelude::{EventHandler};
#[cfg(test)] use crate::discord::connect_bot::TEST_PASSED;

#[allow(unused_imports)]
#[cfg(not(test))] use std::ops::Add;

#[allow(unused_imports)]
#[cfg(not(test))] use serenity::all::ActivityData;
use serenity::all::{CreateInteractionResponse, CreateInteractionResponseMessage, Interaction, Member};
use crate::characters::create_character_sub_command::{accept_character, choose_character_place, delete_character, modify_character, refuse_character, submit_character};
use crate::characters::inventory_subcommand::handle_inventory_interaction;
use crate::item::use_subcommand::handle_tool_selection_interaction;
use crate::recipe::create_subcommand::{approve_recipe, reject_recipe, modify_recipe_interaction};
use crate::item::create_item_subcommand::{approve_item, reject_item};
#[allow(unused_imports)]
use crate::translation::{apply_translations, tr};
use crate::tr_locale;
use crate::travel::travel__sub_command::{travel_from_handler};
use crate::database::server::get_server_by_id;
use crate::database::travel::SpaceType;
use crate::travel::logic::manage_roles;

/// The `Handler` struct serves as a placeholder or marker in this context.
///
/// This struct may be used to define behavior, facilitate functionality, or act as
/// a component in a larger system. Currently, it doesn't hold any data or
/// implement any methods but can be extended to include specific functionality
/// as required.
///
/// # Examples
///
/// Basic usage:
///
/// ```
/// let handler = Handler;
/// // Additional logic or functionality can be added here
/// ```
///
/// This structure can be customized and expanded as necessary to meet the needs
/// of the application.
pub struct Handler;
#[async_trait]
impl EventHandler for Handler {
    ///  Handles the `ready` event in an asynchronous context for testing purposes.
    ///
    ///  This function is executed when the bot successfully connects to Discord
    ///  during tests. It logs a message indicating the bot's connection status
    ///  and modifies a shared `TEST_PASSED` Mutex to reflect that the event
    ///  handler has been executed.
    ///
    ///  # Arguments
    ///
    ///  * `self` - The instance of the struct this function is a part of.
    ///  * `_ctx` - The context of the event, which contains data and utilities
    ///             required for the event handling. It is not used in this function.
    ///  * `ready` - The `Ready` struct, which contains information about the
    ///              bot's connection, such as the bot user's details.
    ///
    ///  # Behavior
    ///
    ///  - Prints a message to the console confirming the bot's connection
    ///    and the associated bot user's name.
    ///  - Attempts to acquire a lock on the `TEST_PASSED` Mutex:
    ///       - If successful, it pushes `true` to the front of the linked list
    ///         inside the Mutex.
    ///       - If an error occurs while acquiring the lock, prints the error.
    ///
    ///  # Notes
    ///
    ///  - This function is conditionally compiled and will only be available
    ///    when the `test` configuration is enabled (e.g., during unit/integration tests).
    ///  - Ensure that the `TEST_PASSED` Mutex is properly initialized before use to
    ///    avoid runtime issues.
    ///  - Any errors encountered when locking the Mutex will only be logged to the
    ///    console; they are not propagated further.
    #[cfg(test)]
    async fn ready(&self, _ctx: Context, ready: Ready) {
        println!("{} is connected!", ready.user.name);
        match TEST_PASSED.lock(){
            Ok(mut mutex) => {mutex.push_front(true)}
            Err(e) => {println!("{:?}", e)}
        }
    }

    #[cfg(not(test))]
    async fn ready(&self, _ctx: Context, ready: Ready) {
        println!("{} is connected!", ready.user.name);
        let _ = crate::travel::logic::setup().await;
        let _ = crate::craft::logic::setup().await;
        let _ = crate::loot::logic::setup().await;
        let _ = crate::universe::time::setup_universal_time().await;
    }

    async fn interaction_create(&self, ctx: Context, interaction: Interaction) {
        match interaction.message_component(){
            None => {}
            Some(modal) => {
                let modal_data = modal.data.custom_id.as_str();
                let result = match modal_data {
                    "create_character__delete_character" => delete_character(ctx.clone(), modal.clone()).await,
                    "create_character__submit_character" => submit_character(ctx.clone(), modal.clone()).await,
                    "create_character__refuse_character" => refuse_character(ctx.clone(), modal.clone()).await,
                    "create_character__accept_character" => accept_character(ctx.clone(), modal.clone()).await,
                    "create_character__modify_character" => modify_character(ctx.clone(), modal.clone()).await,
                    "create_character__choose_place" => choose_character_place(ctx.clone(), modal.clone()).await,
                    "recipe__approve" => approve_recipe(ctx.clone(), modal.clone()).await,
                    "recipe__reject" => reject_recipe(ctx.clone(), modal.clone()).await,
                    "recipe__modify" => modify_recipe_interaction(ctx.clone(), modal.clone()).await,
                    "select__menu__chose_destination" => travel_from_handler(ctx.clone(), modal.clone()).await,
                    _ => {
                        if modal_data.starts_with("item:") {
                            let parts: Vec<&str> = modal_data.split(':').collect();
                            if parts.len() == 3 {
                                // format: item:approve/reject:into_wiki
                                let action = parts[1];
                                let into_wiki = parts[2] == "true";
                                if action == "approve" {
                                    approve_item(ctx.clone(), modal.clone(), into_wiki).await
                                } else {
                                    reject_item(ctx.clone(), modal.clone(), into_wiki).await
                                }
                            } else {
                                return;
                            }
                        } else if modal_data.starts_with("inv:") {
                            let parts: Vec<&str> = modal_data.split(':').collect();
                            if parts.len() == 5 {
                                // format: inv:action:char_id:univ_id:page
                                let char_id = parts[2];
                                let univ_id = parts[3];
                                let page = parts[4].parse::<usize>().unwrap_or(0);
                                handle_inventory_interaction(ctx.clone(), modal.clone(), char_id, univ_id, page).await
                            } else {
                                return;
                            }
                        } else if modal_data.starts_with("tool_sel:") {
                            let parts: Vec<&str> = modal_data.split(':').collect();
                            if parts.len() == 5 {
                                // format: tool_sel:action:univ_id:chan_id:page
                                let univ_id = parts[2];
                                let chan_id = parts[3];
                                let page = parts[4].parse::<usize>().unwrap_or(0);
                                handle_tool_selection_interaction(ctx.clone(), modal.clone(), univ_id, chan_id, page).await
                            } else {
                                return;
                            }
                        } else {
                            return;
                        }
                    }
                };

                if let Err(e) = result {
                    let locale = modal.locale.as_str();
                    let content = tr_locale!(locale, &e.to_string());
                    let _ = modal.create_response(ctx, CreateInteractionResponse::Message(
                        CreateInteractionResponseMessage::new()
                            .content(content)
                            .ephemeral(true)
                    )).await;
                }
            }
        }
    }

    async fn guild_member_addition(&self, ctx: Context, new_member: Member) {
        let guild_id = new_member.guild_id.get();
        let user_id = new_member.user.id.get();

        // Récupérer le serveur dans la DB
        let server = match get_server_by_id(guild_id).await {
            Ok(Some(s)) => s,
            _ => return,
        };

        // Vérifier si un personnage existe pour cette personne
        if let Ok(Some(character)) = server.clone().has_character(user_id).await {
            // 1. Attribuer le rôle de joueur si configuré
            if let Some(player_role) = &server.player_role_id {
                manage_roles(ctx.http.clone(), guild_id, user_id, Some(player_role.id), None).await;
            }

            // 2. Renommer le joueur
            let nickname = if (character.name.to_string() + "│" + new_member.user.display_name()).chars().count() > 32 {
                character.name.to_string()
            } else {
                character.name.to_string() + "│" + new_member.user.display_name()
            };
            
            let _ = ctx.http.edit_member(
                new_member.guild_id,
                new_member.user.id,
                &serenity::all::EditMember::new().nickname(nickname),
                None
            ).await;

            // 3. Vérifier le PlayerMove pour attribuer le rôle du lieu ou de la route
            if let Ok(Some(player_move)) = server.get_player_move(user_id).await {
                match player_move.actual_space_type {
                    SpaceType::Place => {
                        // On vérifie si le lieu actuel est bien sur ce serveur
                        if let Ok(Some(place)) = crate::database::places::get_place_by_category_id(player_move.universe_id, player_move.actual_space_id).await {
                            if place.server_id == guild_id {
                                manage_roles(ctx.http.clone(), guild_id, user_id, Some(place.role), None).await;
                            }
                        }
                    }
                    SpaceType::Road => {
                        // On vérifie si la route actuelle est bien sur ce serveur ou si c'est le road_server_id
                        let road_server_id = player_move.road_server_id.unwrap_or(player_move.server_id);
                        if road_server_id == guild_id {
                            if let Some(road_role) = player_move.road_role_id {
                                manage_roles(ctx.http.clone(), guild_id, user_id, Some(road_role), None).await;
                            }
                        }
                    }
                }
            }
        }
    }
}