use std::str::FromStr;
use futures::{TryStreamExt};
use std::time::Duration;
use std::sync::atomic::Ordering;
use mongodb::bson::doc;
use mongodb::bson::oid::ObjectId;
use serenity::all::{ButtonStyle, Color, ComponentInteraction, CreateActionRow, CreateButton, CreateEmbed, CreateEmbedAuthor, CreateEmbedFooter, CreateInputText, CreateInteractionResponse, CreateMessage, EditMember, EditMessage, EmbedField, InputTextStyle, ModalInteraction, Permissions};
use serenity::json::json;
use crate::discord::poise_structs::{Context, Error, Data};
use crate::utility::reply::reply;
use serenity::client::Context as SerenityContext;
use serenity::http::CacheHttp;
use serenity::utils::CreateQuickModal;
use crate::database::server::{get_server_by_id, Server};
use crate::{tr, tr_locale};
use crate::translation::get_by_locale;
use crate::database::characters::Character;
use crate::database::db_namespace::{CHARACTERS_COLLECTION_NAME, VERSEENGINE_DB_NAME};
use crate::database::places::{Place};
use crate::database::stats::{Stat, StatValue};
use crate::database::travel::{PlayerMove};
use crate::database::universe::get_universe_by_id;

pub static CHARACTER_MODAL_TITLE: &str = "character_modal_title";
pub static MODIFY_CHARACTER_BUTTON_CUSTOM_ID: &str = "create_character__modify_character";
pub static DELETE_CHARACTER_BUTTON_CUSTOM_ID: &str = "create_character__delete_character";
pub static SUBMIT_CHARACTER_BUTTON_CUSTOM_ID: &str = "create_character__submit_character";
pub static ACCEPT_CHARACTER_BUTTON_CUSTOM_ID: &str = "create_character__accept_character";
pub static REJECT_CHARACTER_BUTTON_CUSTOM_ID: &str = "create_character__refuse_character";
pub static CREATE_CHARACTER_SUBMIT_NOTIFICATION: &str = "create_character__submit_notification";

pub static CHARACTER_NAME: &str = "character_name";
pub static CHARACTER_DESCRIPTION: &str = "character_description";
pub static CHARACTER_STORY: &str = "character_story";
pub static CHARACTER_SPECIAL_REQUEST: &str = "character_special_request";
pub static CHARACTER_INSTRUCTION: &str = "character_instruction";
pub static CHARACTER_REJECT_REASON: &str = "character_reject_reason";
pub static ACCEPT_CHARACTER_CHOOSE_PLACE: &str = "create_character__choose_place";
pub static CHARACTER_ACCEPT__STAT_INPUT: &str = "character_stat_input";

pub struct CharacterModal {
    pub name: String,
    pub description: String,
    pub story: String,
    pub special_request: String,
    pub interaction: ModalInteraction,
}

pub async fn execute_character_modal(
    ctx: &SerenityContext,
    id: serenity::all::InteractionId,
    token: &str,
    locale: &str,
    default_values: Option<(&str, &str, &str, &str)>,
) -> Result<Option<CharacterModal>, Error> {
    let title = get_by_locale(locale, CHARACTER_MODAL_TITLE, None, None);
    let name_label = get_by_locale(locale, CHARACTER_NAME, None, None);
    let desc_label = get_by_locale(locale, CHARACTER_DESCRIPTION, None, None);
    let story_label = get_by_locale(locale, CHARACTER_STORY, None, None);
    let request_label = get_by_locale(locale, CHARACTER_SPECIAL_REQUEST, None, None);
    let instruction = get_by_locale(locale, CHARACTER_INSTRUCTION, None, None);

    let (def_name, def_desc, def_story, def_request) = default_values.unwrap_or(("", "", "", ""));

    let custom_id = format!("{}-{}", id, "character_modal");

    let modal_json = json!({
        "type": 9,
        "data": {
            "custom_id": custom_id,
            "title": title,
            "components": [
                {
                    "type": 10,
                    "content": instruction
                },
                {
                    "type": 1,
                    "components": [
                        {
                            "type": 4,
                            "custom_id": CHARACTER_NAME,
                            "label": name_label,
                            "style": 1,
                            "value": def_name,
                            "required": true,
                            "max_length": 32
                        }
                    ]
                },
                {
                    "type": 1,
                    "components": [
                        {
                            "type": 4,
                            "custom_id": CHARACTER_DESCRIPTION,
                            "label": desc_label,
                            "style": 2,
                            "value": def_desc,
                            "required": true,
                            "max_length": 1024
                        }
                    ]
                },
                {
                    "type": 1,
                    "components": [
                        {
                            "type": 4,
                            "custom_id": CHARACTER_STORY,
                            "label": story_label,
                            "style": 2,
                            "value": def_story,
                            "required": true,
                            "max_length": 1024
                        }
                    ]
                },
                {
                    "type": 1,
                    "components": [
                        {
                            "type": 4,
                            "custom_id": CHARACTER_SPECIAL_REQUEST,
                            "label": request_label,
                            "style": 2,
                            "value": def_request,
                            "required": false,
                            "max_length": 1024
                        }
                    ]
                }
            ]
        }
    });

    ctx.http.create_interaction_response(id, token, &modal_json, vec![]).await?;

    let response = serenity::collector::ModalInteractionCollector::new(ctx)
        .filter(move |m| m.data.custom_id == custom_id)
        .timeout(std::time::Duration::from_secs(1800))
        .await;

    if let Some(m) = response {
        let mut name = String::new();
        let mut description = String::new();
        let mut story = String::new();
        let mut special_request = String::new();

        for row in &m.data.components {
            for component in &row.components {
                if let serenity::all::ActionRowComponent::InputText(it) = component {
                    if it.custom_id == CHARACTER_NAME {
                        name = it.value.clone().unwrap_or_default();
                    } else if it.custom_id == CHARACTER_DESCRIPTION {
                        description = it.value.clone().unwrap_or_default();
                    } else if it.custom_id == CHARACTER_STORY {
                        story = it.value.clone().unwrap_or_default();
                    } else if it.custom_id == CHARACTER_SPECIAL_REQUEST {
                        special_request = it.value.clone().unwrap_or_default();
                    }
                }
            }
        }

        m.create_response(ctx, CreateInteractionResponse::Acknowledge).await?;

        Ok(Some(CharacterModal {
            name,
            description,
            story,
            special_request,
            interaction: m,
        }))
    } else {
        Ok(None)
    }
}

/// Verifies that the interaction user owns the character in the message
/// Verifies that the interaction user is the owner of the character described in the message.
///
/// This is determined by comparing the interaction user's ID with the ID stored in the
/// footer of the first embed of the message.
///
/// # Arguments
/// * `_ctx` - The serenity context (unused, kept for signature consistency).
/// * `component_interaction` - The interaction that triggered this check.
///
/// # Returns
/// * `Ok(())` if the user is the owner.
/// * `Err` if the footer is invalid or the user is not the owner.
async fn verify_character_ownership(
    _ctx: &SerenityContext,
    component_interaction: &ComponentInteraction,
) -> Result<(), Error> {
    let user_id = component_interaction.user.id.get();
    // The character owner's ID is stored as a string in the embed footer
    let Ok(character_user_id) = component_interaction.message.embeds[0]
        .footer.as_ref()
        .and_then(|footer| footer.text.parse::<u64>().ok())
        .ok_or_else(|| -> Error { "create_character__invalid_footer".into() }) else { return Err("create_character__invalid_footer".into()) };

    if user_id != character_user_id {
        return Err("create_character__not_owner".into());
    }

    Ok(())
}

/// Verifies that the user has moderator or administrator permissions in the current guild.
///
/// A user is considered to have permission if they:
/// 1. Have the `ADMINISTRATOR` permission.
/// 2. Have a role that matches the `moderator_role_id` in the server configuration.
/// 3. Have a role that matches the `admin_role_id` in the server configuration.
///
/// # Arguments
/// * `_ctx` - The serenity context (unused, kept for signature consistency).
/// * `component_interaction` - The interaction that triggered this check.
/// * `server` - The server configuration from the database.
///
/// # Returns
/// * `Ok(())` if the user has permission.
/// * `Err` if the user is not a member or lacks permissions.
async fn verify_moderator_permission(
    _ctx: &SerenityContext,
    component_interaction: &ComponentInteraction,
    server: &Server,
) -> Result<(), Error> {
    let Ok(member) = component_interaction.member.as_ref()
        .ok_or("create_character__no_member") else { return Err("create_character__no_member".into()) };

    let has_admin_permission = member.permissions
        .map_or(false, |p| p.contains(Permissions::ADMINISTRATOR));
    let has_moderator_role = server.moderator_role_id
        .map_or(false, |role| member.roles.contains(&role.id.into()));
    let has_admin_role = server.admin_role_id
        .map_or(false, |role| member.roles.contains(&role.id.into()));

    if !has_admin_permission && !has_moderator_role && !has_admin_role {
        return Err("create_character__no_permission".into());
    }

    Ok(())
}

/// Slash command to initiate the character creation process.
///
/// It delegates to `_create_character` and sends the result back to the user using the `reply` utility.
#[poise::command(slash_command, guild_only, rename = "character_create")]
pub async fn create_character(
    ctx: Context<'_>
) -> Result<(), Error> {
    let result = _create_character(ctx).await;
    if let Err(result) = result {
        if let Err(_) = reply(ctx, Err(result)).await {return Err("reply__reply_failed".into())};
    };
    Ok(())
}

/// Internal logic for creating a character.
///
/// This function:
/// 1. Validates that the command is used in the correct channel and the user doesn't already have a character.
/// 2. Opens a modal for the user to fill in character details.
/// 3. Sends a message with an embed containing the character info and buttons for further actions (Submit, Modify, Delete).
///
/// # Returns
/// * `Ok(&'static str)` - The translation key for the success message.
/// * `Err` - If any validation fails, the process times out, or a database error occurs.
pub async fn _create_character(ctx: Context<'_>) -> Result<&'static str, Error>{
    // 2 process qui échangent
    //  validation par les modos/admins
    //  modification par l'utilisateur DONE
    // la validation ouvre un modal pour demander les stats du joueur.
    // les infos sont enregistrés
    // le role joueur est attribué

    let guild_id = ctx.guild_id().unwrap();

    let server = get_server_by_id(guild_id.get()).await;
    let server = match server {
        Ok(result) => {
            match result{
                None => {return Err("create_character__no_universe_found".into())}
                Some(server) => {server}
            }
        }
        Err(_) => {
            return Err("create_character__database_error".into())}
    };

    if server.rp_character_channel_id.unwrap().id != ctx.channel_id().get(){
        return Err("create_character__wrong_channel".into())
    }
 
    let Ok(player_result) = server.has_character(ctx.author().id.get()).await else { return Err("create_character__database_error".into()) };
    if player_result.is_some() {
        return Err("create_character__character_already_existing".into());
    }

    let app_ctx = match ctx.clone() {
        Context::Application(app_ctx) => app_ctx,
        _ => return Err("create_character__guild_only".into()),
    };

    let locale = ctx.locale().unwrap_or("fr");
    let interaction_id = app_ctx.interaction.id;
    let interaction_token = &app_ctx.interaction.token;
    let modal_result = execute_character_modal(ctx.serenity_context(), interaction_id, interaction_token, locale, None).await?;
    
    app_ctx.has_sent_initial_response.store(true, Ordering::SeqCst);
    
    let character_modal = match modal_result {
        Some(m) => m,
        None => return Err("create_character__timed_out".into()),
    };

    let inputs = vec![
        character_modal.name,
        character_modal.description,
        character_modal.story,
        character_modal.special_request,
    ];

    let buttons = vec![
        CreateActionRow::Buttons(
            vec![
                CreateButton::new(SUBMIT_CHARACTER_BUTTON_CUSTOM_ID).label(tr!(ctx, SUBMIT_CHARACTER_BUTTON_CUSTOM_ID)).style(ButtonStyle::Success),
                CreateButton::new(MODIFY_CHARACTER_BUTTON_CUSTOM_ID).label(tr!(ctx, MODIFY_CHARACTER_BUTTON_CUSTOM_ID)).style(ButtonStyle::Primary),
                CreateButton::new(DELETE_CHARACTER_BUTTON_CUSTOM_ID).label(tr!(ctx, DELETE_CHARACTER_BUTTON_CUSTOM_ID)).style(ButtonStyle::Danger),
            ]
        )
    ];

    let result_message = app_ctx.channel_id().send_message(ctx, CreateMessage::new().embed(
        CreateEmbed::new()
            .footer(CreateEmbedFooter::new(ctx.author().id.get().to_string()))
            .title(inputs[0].clone())
            .field(tr!(ctx, CHARACTER_DESCRIPTION), inputs[1].clone(), true)
            .field(tr!(ctx, CHARACTER_STORY), inputs[2].clone(), true)
            .field(tr!(ctx, CHARACTER_SPECIAL_REQUEST), inputs[3].clone(), false)
            .author(CreateEmbedAuthor::new(ctx.author().name.as_str()))
            .color(Color::from_rgb(112, 190, 255))
    )
        .components(buttons)
    ).await;

    match result_message {
        Ok(_) => {
            Ok("create_character__submitted")
        }
        Err(_) => { Err("create_place__character_too_long".into()) }
    }
}

/// Deletes a character sheet draft from the channel.
///
/// Only the owner of the character can perform this action.
pub async fn delete_character(ctx: SerenityContext, component_interaction: ComponentInteraction) -> Result<&'static str, Error>{
    let Ok(_) = verify_character_ownership(&ctx, &component_interaction).await else { return Err("create_character__not_owner".into()) };
    let Ok(_) = component_interaction.message.delete(ctx).await else { return Err("create_character__database_error".into()) };
    Ok("delete_character")
}

/// Submits a character sheet draft for moderator approval.
///
/// This function:
/// 1. Verifies ownership.
/// 2. Replaces the action buttons with "Accept" and "Refuse" (visible to moderators).
/// 3. Changes the embed color to green to indicate submission.
/// 4. Sends a notification message to the server's log channel if configured.
pub async fn submit_character(ctx: SerenityContext, component_interaction: ComponentInteraction) -> Result<&'static str, Error> {
    let Ok(_) = verify_character_ownership(&ctx, &component_interaction).await else { return Err("create_character__not_owner".into()) };

    let buttons = vec![
        CreateActionRow::Buttons(
            vec![
                CreateButton::new(ACCEPT_CHARACTER_BUTTON_CUSTOM_ID).label(tr_locale!(component_interaction.locale.as_str(), ACCEPT_CHARACTER_BUTTON_CUSTOM_ID)).style(ButtonStyle::Success),
                CreateButton::new(REJECT_CHARACTER_BUTTON_CUSTOM_ID).label(tr_locale!(component_interaction.locale.as_str(), REJECT_CHARACTER_BUTTON_CUSTOM_ID )).style(ButtonStyle::Danger),
            ]
        )
    ];

    let message = component_interaction.message.clone();
    let embed: CreateEmbed = message.embeds[0].clone().into();

    let Ok(_) = component_interaction.channel_id.edit_message(ctx.clone(), message.id, EditMessage::new().embed(
        embed.color(Color::from_rgb(0, 255, 0))
    )
        .components(buttons)
    ).await else { return Err("create_character__database_error".into()) };

    let Ok(_) = component_interaction.create_response(ctx.clone(), CreateInteractionResponse::Acknowledge).await else { return Err("create_character__database_error".into()) };

    let message = tr_locale!(component_interaction.locale.as_str(), CREATE_CHARACTER_SUBMIT_NOTIFICATION) + " " + component_interaction.message.link().as_str();

    if let Ok(Some(server)) = get_server_by_id(component_interaction.guild_id.unwrap().get()).await {
        if let Some(log_channel) = server.log_channel_id {
            let _ = ctx.http().send_message(
                log_channel.id.into(),
                vec![],
                &CreateMessage::new().content(message),
            ).await;
        }
    }

    Ok("create_character__submitted")
}

/// Opens a modal to allow the user to modify their character sheet draft.
///
/// Only the owner can modify their character. The modal is pre-populated with
/// the current values extracted from the message embed.
pub async fn modify_character(ctx: SerenityContext, component_interaction: ComponentInteraction) -> Result<&'static str, Error> {
    let Ok(_) = verify_character_ownership(&ctx, &component_interaction).await else { return Err("create_character__not_owner".into()) };

    let embed_fields = component_interaction.message.embeds[0].clone().fields;
    let embed_title = component_interaction.message.embeds[0].title.clone().unwrap_or_default();
    
    let default_values = (
        embed_title.as_str(),
        embed_fields[0].value.as_str(),
        embed_fields[1].value.as_str(),
        embed_fields[2].value.as_str(),
    );

    let locale = component_interaction.locale.as_str();
    let modal_result = execute_character_modal(&ctx, component_interaction.id, &component_interaction.token, locale, Some(default_values)).await?;

    let character_modal = match modal_result {
        Some(m) => m,
        None => return Err("create_character__timed_out".into()),
    };

    let inputs = vec![
        character_modal.name,
        character_modal.description,
        character_modal.story,
        character_modal.special_request,
    ];

    let buttons = vec![
        CreateActionRow::Buttons(
            vec![
                CreateButton::new(SUBMIT_CHARACTER_BUTTON_CUSTOM_ID).label(tr_locale!(character_modal.interaction.locale.as_str(), SUBMIT_CHARACTER_BUTTON_CUSTOM_ID)).style(ButtonStyle::Success),
                CreateButton::new(MODIFY_CHARACTER_BUTTON_CUSTOM_ID).label(tr_locale!(character_modal.interaction.locale.as_str(), MODIFY_CHARACTER_BUTTON_CUSTOM_ID)).style(ButtonStyle::Primary),
                CreateButton::new(DELETE_CHARACTER_BUTTON_CUSTOM_ID).label(tr_locale!(character_modal.interaction.locale.as_str(), DELETE_CHARACTER_BUTTON_CUSTOM_ID)).style(ButtonStyle::Danger),
            ]
        )
    ];

    let interaction = character_modal.interaction.clone();

    let result_message = match character_modal.interaction.message.as_ref() {
        None => {
            character_modal.interaction.channel_id.send_message(ctx, CreateMessage::new().embed(
                CreateEmbed::new()
                    .footer(CreateEmbedFooter::new(character_modal.interaction.user.id.get().to_string()))
                    .title(inputs[0].clone())
                    .field(tr_locale!(interaction.locale.as_str(), CHARACTER_DESCRIPTION), inputs[1].clone(), true)
                    .field(tr_locale!(interaction.locale.as_str(), CHARACTER_STORY), inputs[2].clone(), true)
                    .field(tr_locale!(interaction.locale.as_str(), CHARACTER_SPECIAL_REQUEST), inputs[3].clone(), false)
                    .author(CreateEmbedAuthor::new(component_interaction.user.name.as_str()))
                    .color(Color::from_rgb(112, 190, 255))
            )
                .components(buttons)
            ).await
        }
        Some(message) => {
            let embed_fields = vec![
                (
                    tr_locale!(interaction.locale.as_str(), CHARACTER_DESCRIPTION),
                    inputs[1].clone(),
                    true
                ),
                (
                    tr_locale!(interaction.locale.as_str(), CHARACTER_STORY),
                    inputs[2].clone(),
                    true
                ),
                (
                    tr_locale!(interaction.locale.as_str(), CHARACTER_SPECIAL_REQUEST),
                    inputs[3].clone(),
                    false
                )
            ];
            character_modal.interaction.channel_id.edit_message(ctx, message.id, EditMessage::new().embed(
                CreateEmbed::new()
                    .footer(CreateEmbedFooter::new(message.embeds.get(0).unwrap().footer.clone().unwrap().text.as_str()))
                    .title(inputs[0].clone())
                    .fields(embed_fields)
                    .author(CreateEmbedAuthor::new(component_interaction.user.name.as_str()))
                    .color(Color::from_rgb(112, 190, 255))
            )
                .components(buttons)
            ).await
        }
    };

    match result_message {
        Ok(_) => Ok("create_character__submitted"),
        Err(_) => Err("create_place__character_too_long".into())
    }
}

/// Allows a moderator to refuse a character sheet.
///
/// This function:
/// 1. Verifies moderator permissions.
/// 2. Opens a modal to ask for a rejection reason.
/// 3. Appends the reason to the embed, changes its color to red, and removes all buttons.
pub async fn refuse_character(ctx: SerenityContext, component_interaction: ComponentInteraction) -> Result<&'static str, Error> {
    let guild_id = component_interaction.guild_id.unwrap();
    let Ok(server) = get_server_by_id(guild_id.get()).await else { return Err("create_character__database_error".into()) };
    let Ok(server) = server.ok_or("create_character__no_universe_found") else { return Err("create_character__no_universe_found".into()) };

    let Ok(_) = verify_moderator_permission(&ctx, &component_interaction, &server).await else { return Err("create_character__no_permission".into()) };

    let locale = component_interaction.locale.as_str();
    let title = get_by_locale(locale, CHARACTER_MODAL_TITLE, None, None);
    let reason_label = get_by_locale(locale, CHARACTER_REJECT_REASON, None, None);
    
    let custom_id = format!("{}-{}", component_interaction.id, "refuse_modal");
    
    let modal_json = json!({
        "type": 9,
        "data": {
            "custom_id": custom_id,
            "title": title,
            "components": [
                {
                    "type": 1,
                    "components": [
                        {
                            "type": 4,
                            "custom_id": CHARACTER_REJECT_REASON,
                            "label": reason_label,
                            "style": 2,
                            "required": false,
                            "max_length": 864
                        }
                    ]
                }
            ]
        }
    });

    ctx.http.create_interaction_response(component_interaction.id, &component_interaction.token, &modal_json, vec![]).await?;

    let response = serenity::collector::ModalInteractionCollector::new(&ctx)
        .filter(move |m| m.data.custom_id == custom_id)
        .timeout(std::time::Duration::from_secs(1800))
        .await;

    let m = match response {
        Some(m) => m,
        None => return Err("create_character__timed_out".into()),
    };

    let reason = m.data.components.iter()
        .flat_map(|row| row.components.iter())
        .find_map(|component| {
            if let serenity::all::ActionRowComponent::InputText(it) = component {
                if it.custom_id == CHARACTER_REJECT_REASON {
                    return Some(it.value.clone().unwrap_or_default());
                }
            }
            None
        })
        .unwrap_or_default();

    m.create_response(&ctx, CreateInteractionResponse::Acknowledge).await?;

    let Ok(message) = m.message.clone()
        .ok_or("create_character__message_not_found") else { return Err("create_character__message_not_found".into()) };

    let mut embed_fields = component_interaction.message.embeds[0].clone().fields;
    embed_fields.push(EmbedField::new(
        tr_locale!(component_interaction.locale.as_str(), CHARACTER_REJECT_REASON),
        reason,
        false
    ));

    let buttons = vec![
        CreateActionRow::Buttons(
            vec![
                CreateButton::new(SUBMIT_CHARACTER_BUTTON_CUSTOM_ID).label(tr_locale!(m.locale.as_str(), SUBMIT_CHARACTER_BUTTON_CUSTOM_ID)).style(ButtonStyle::Success),
                CreateButton::new(MODIFY_CHARACTER_BUTTON_CUSTOM_ID).label(tr_locale!(m.locale.as_str(), MODIFY_CHARACTER_BUTTON_CUSTOM_ID)).style(ButtonStyle::Primary),
                CreateButton::new(DELETE_CHARACTER_BUTTON_CUSTOM_ID).label(tr_locale!(m.locale.as_str(), DELETE_CHARACTER_BUTTON_CUSTOM_ID)).style(ButtonStyle::Danger),
            ]
        )
    ];

    let embed_fields: Vec<(String, String, bool)> = embed_fields
        .iter()
        .map(|field| (field.name.clone(), field.value.clone(), field.inline))
        .collect();

    let Ok(_) = m.channel_id.edit_message(ctx, message.id, EditMessage::new().embed(
        CreateEmbed::new()
            .footer(CreateEmbedFooter::new(message.embeds.get(0).unwrap().footer.clone().unwrap().text.as_str()))
            .title(message.embeds.get(0).unwrap().title.clone().unwrap_or_default())
            .fields(embed_fields)
            .author(CreateEmbedAuthor::new(component_interaction.user.name.as_str()))
            .color(Color::from_rgb(255, 0, 0))
    )
        .components(buttons)
    ).await else { return Err("create_character__database_error".into()) };

    Ok("create_character__refused")
}

/// Parses a stat value from a string based on the expected `StatValue` variant.
///
/// Supported types: `i64`, `f64`, `String`, `bool`.
fn parse_stat_value(value_str: &str, base_value: &StatValue) -> Option<StatValue> {
    match base_value {
        StatValue::I64(_) => value_str.parse::<i64>().ok().map(StatValue::I64),
        StatValue::F64(_) => value_str.parse::<f64>().ok().map(StatValue::F64),
        StatValue::String(_) => Some(StatValue::String(value_str.to_string())),
        StatValue::Bool(_) => value_str.parse::<bool>().ok().map(StatValue::Bool),
    }
}

/// Creates a new `Stat` instance with a new value while preserving other attributes.
fn create_stat_with_value(stat: &Stat, value: StatValue) -> Stat {
    Stat {
        _id: Default::default(),
        universe_id: Default::default(),
        name: stat.name.clone(),
        base_value: value,
        formula: stat.formula.clone(),
        min: stat.min.clone(),
        max: stat.max.clone(),
        modifiers: vec![],
    }
}



/// Allows a moderator to accept a character sheet and finalize its stats.
///
/// This is a complex multi-step process:
/// 1. Verifies moderator permissions.
/// 2. Fetches the defined stats for the universe.
/// 3. Opens a modal with a text area containing a template of all stats.
/// 4. Parses the moderator's input to extract stat values.
/// 5. Saves the character and its stats to the database.
/// 6. Assigns the `player_role_id` to the user if configured.
/// 7. Updates the message to indicate acceptance and removes all buttons.
pub async fn accept_character(ctx: SerenityContext, component_interaction: ComponentInteraction) -> Result<&'static str, Error> {
    let member = component_interaction.member.as_ref().unwrap();
    let guild_id = component_interaction.guild_id.unwrap();
    let server = get_server_by_id(guild_id.get()).await;
    let server = match server {
        Ok(result) => {
            match result {
                None => { return Err("create_character__no_universe_found".into()) }
                Some(server) => { server }
            }
        }
        Err(_) => { return Err("create_character__database_error".into()) }
    };

    // Permission check
    let has_admin_permission = member.permissions.map_or(false, |p| p.contains(Permissions::ADMINISTRATOR));
    let has_moderator_role = server.moderator_role_id.map_or(false, |role| member.roles.contains(&role.id.into()));
    let has_admin_role = server.admin_role_id.map_or(false, |role| member.roles.contains(&role.id.into()));
    if !has_admin_permission && !has_moderator_role && !has_admin_role {
        return Err("create_character__no_permission".into());
    }

    let Ok(universe) = get_universe_by_id(ObjectId::from_str(server.universe_id.to_string().as_str())?).await else { return Err("create_character__database_error".into()) };
    let Ok(universe) = universe.ok_or("create_character__no_universe_found") else { return Err("create_character__no_universe_found".into()) };
    let Ok(stats_cursor) = universe.clone().get_stats().await else { return Err("create_character__database_error".into()) };
    let Ok(stats) = stats_cursor.try_collect::<Vec<Stat>>().await else { return Err("create_character__database_error".into()) };

    // Prepare the stat template for the modal
    let locale = component_interaction.locale.as_str();
    let title = get_by_locale(locale, CHARACTER_MODAL_TITLE, None, None);
    let label = get_by_locale(locale, CHARACTER_ACCEPT__STAT_INPUT, None, None);

    let mut text = String::new();
    for stat in stats.clone() {
        text.push_str(&format!("{}: [{:?}]\n", stat.name, stat.base_value));
    };

    let custom_id = format!("{}-{}", component_interaction.id, "accept_modal");

    let modal_json = json!({
        "type": 9,
        "data": {
            "custom_id": custom_id,
            "title": title,
            "components": [
                {
                    "type": 1,
                    "components": [
                        {
                            "type": 4,
                            "custom_id": "stats_input",
                            "label": label,
                            "style": 2,
                            "value": text,
                            "required": false
                        }
                    ]
                }
            ]
        }
    });

    ctx.http.create_interaction_response(component_interaction.id, &component_interaction.token, &modal_json, vec![]).await?;

    let response = serenity::collector::ModalInteractionCollector::new(&ctx)
        .filter(move |m| m.data.custom_id == custom_id)
        .timeout(std::time::Duration::from_secs(1800))
        .await;

    let m = match response {
        Some(m) => m,
        None => return Err("create_character__timed_out".into()),
    };

    let input = m.data.components.iter()
        .flat_map(|row| row.components.iter())
        .find_map(|component| {
            if let serenity::all::ActionRowComponent::InputText(it) = component {
                if it.custom_id == "stats_input" {
                    return Some(it.value.clone().unwrap_or_default());
                }
            }
            None
        })
        .unwrap_or_default();

    let mut extracted_stats: Vec<Stat> = Vec::new();
    let mut line_matched = std::collections::HashSet::new();

    // Parse each line of the input to find stat values
    for line in input.lines() {
        for stat in stats.iter() {
            if line.contains(&stat.name) {
                line_matched.insert(stat.name.clone());

                // Value is expected after a colon, or the whole line if no colon
                let value_str = if let Some(colon_pos) = line.find(':') {
                    &line[colon_pos + 1..].trim()
                } else {
                    line.trim()
                };

                if let Some(value) = parse_stat_value(value_str, &stat.base_value) {
                    extracted_stats.push(create_stat_with_value(stat, value));
                } else {
                    return Err("create_character__type_mismatch".into());
                }
                break;
            }
        }
    }

    // For any stats not found in the input, use their default values
    for stat in stats.iter() {
        if !line_matched.contains(&stat.name) {
            extracted_stats.push(stat.clone());
        }
    }

    m.create_response(ctx.clone(), CreateInteractionResponse::Acknowledge).await?;

    let character_user_id = match component_interaction.message.embeds[0]
        .footer.as_ref()
        .and_then(|f| f.text.parse::<u64>().ok()) {
            Some(id) => id,
            None => return Err("create_character__invalid_footer".into()),
        };

    let character_name = match component_interaction.message.embeds[0].title.as_ref() {
        Some(title) => title.clone(),
        None => return Err("create_character__invalid_embed_title".into()),
    };

    let embed_fields = &component_interaction.message.embeds[0].fields;
    let _description = embed_fields.iter()
        .find(|f| f.name == tr_locale!(component_interaction.locale.as_str(), CHARACTER_DESCRIPTION))
        .map(|f| f.value.clone())
        .unwrap_or_default();

    let _story = embed_fields.iter()
        .find(|f| f.name == tr_locale!(component_interaction.locale.as_str(), CHARACTER_STORY))
        .map(|f| f.value.clone())
        .unwrap_or_default();

    let _special_request = embed_fields.iter()
        .find(|f| f.name == tr_locale!(component_interaction.locale.as_str(), CHARACTER_SPECIAL_REQUEST))
        .map(|f| f.value.clone())
        .unwrap_or_default();



    let character = Character {
        _id: Default::default(), 
        user_id: character_user_id,
        universe_id: server.universe_id,
        name: character_name,
        stats: extracted_stats,
    };

    let Ok(_character_result) = character.clone().update().await else { return Err("create_character__database_error".into()) };

    if let Some(player_role_id) = server.player_role_id {
        if let Ok(member) = ctx.http().get_member(guild_id, character_user_id.into()).await {
            let _ = member.add_role(&ctx.http(), player_role_id.id).await;
        }
    } else {return Err("accept_character__no_player_role_id".into())}

    let _ = if let Ok(member) = ctx.http().get_member(guild_id, character_user_id.into()).await {
        let nickname = if (character.clone().name.to_string() + "│" + member.user.display_name()).chars().count() > 32 {
            character.clone().name.to_string()
        } else { character.clone().name.to_string() + "│" + member.user.display_name() };
        ctx.http().edit_member(
            guild_id,
            character_user_id.into(),
            &EditMember::new().nickname(nickname),
            None).await
    } else { return Err("accept_character__member_not_found".into())};

    let message = component_interaction.message.clone();
    let original_embed: CreateEmbed = message.embeds[0].clone().into();

    let select_menu = serenity::all::CreateSelectMenu::new(
        ACCEPT_CHARACTER_CHOOSE_PLACE,
        serenity::all::CreateSelectMenuKind::Channel {
            channel_types: Some(vec![serenity::all::ChannelType::Category]),
            default_channels: None,
        }
    );

    let components = vec![CreateActionRow::SelectMenu(select_menu)];

    let _ = component_interaction.channel_id.edit_message(
        ctx,
        message.id,
        EditMessage::new().components(components).embed(
            original_embed.color(Color::from_rgb(255, 255, 0)) // Yellow while choosing place
        ),
    ).await;

    Ok("accept_character")
}

    
/// Handles the selection of a place (category) for a character after they've been accepted.
///
/// This function:
/// 1. Verifies moderator permissions.
/// 2. Validates that the selected category ID corresponds to a registered `Place`.
/// 3. Updates the character sheet message to remove the select menu and set the final color.
/// 4. If an invalid category is selected, sends an ephemeral message without acknowledging the interaction.
pub async fn choose_character_place(ctx: SerenityContext, component_interaction: ComponentInteraction) -> Result<&'static str, Error> {
    let guild_id = component_interaction.guild_id.ok_or("create_character__guild_only")?;
    let Ok(server) = get_server_by_id(guild_id.get()).await else { return Err("create_character__database_error".into()) };
    let Ok(server) = server.ok_or("create_character__no_universe_found") else { return Err("create_character__no_universe_found".into()) };

    let Ok(_) = verify_moderator_permission(&ctx, &component_interaction, &server).await else { return Err("create_character__no_permission".into()) };

    let selected_category_id = match &component_interaction.data.kind {
        serenity::all::ComponentInteractionDataKind::ChannelSelect { values } => {
            values.get(0).ok_or("create_character__invalid_interaction")?
        }
        _ => return Err("create_character__invalid_interaction".into()),
    };

    let db_client = crate::database::db_client::get_db_client().await;
    let filter = mongodb::bson::doc!{"category_id": selected_category_id.get().to_string()};
    let place: Option<Place> = db_client
        .database(VERSEENGINE_DB_NAME)
        .collection::<Place>(crate::database::db_namespace::PLACES_COLLECTION_NAME)
        .find_one(filter)
        .await
        .map_err(|_| Error::from("create_character__database_error"))?;

    let Some(place) = place else {
        // Bad category selected. Send ephemeral message and do NOT acknowledge/respond to the interaction normally
        // Actually, to send a message we might need to respond.
        // The issue says "WITHOUT AKNOWLEDGE the modal".
        // In Serenity, if we don't acknowledge, the user sees "Interaction failed".
        // But we want to ask them to change it.
        
        let content = tr_locale!(component_interaction.locale.as_str(), "create_character__invalid_place_selected");
        let _ = component_interaction.create_response(&ctx, CreateInteractionResponse::Message(
            serenity::all::CreateInteractionResponseMessage::new()
                .content(content)
                .ephemeral(true)
        )).await;
        
        return Ok("create_character__invalid_place_selected");
    };

    // Valid place selected
    let Ok(_) = component_interaction.create_response(&ctx, CreateInteractionResponse::Acknowledge).await else { return Err("create_character__database_error".into()) };

    // Extract user ID from embed footer
    let Ok(character_user_id) = component_interaction.message.embeds[0]
        .footer.as_ref()
        .and_then(|footer| footer.text.parse::<u64>().ok())
        .ok_or_else(|| -> Error { "create_character__invalid_footer".into() }) else { return Err("create_character__invalid_footer".into()) };

    if let Ok(member) = guild_id.member(&ctx, character_user_id).await {
        // Add place role
        let roles_to_add = vec![place.role.into(), server.player_role_id.unwrap().id.into()];
        let _ = member.add_roles(&ctx.http(), &roles_to_add).await;
        
        // Remove spectator role if player has it
        if let Some(spectator_role_id) = server.spectator_role_id {
            let _ = member.remove_role(&ctx.http(), spectator_role_id.id).await;
        }
    }

    let player_id = component_interaction.message.embeds.get(0).unwrap();
    let player_id = player_id.footer.clone().unwrap();
    let player_id = player_id.text.as_str();
    let character_filter = doc!{"user_id": player_id, "universe_id": server.universe_id};

    let Ok(Some(_character)) = db_client.database(VERSEENGINE_DB_NAME).collection::<Character>(CHARACTERS_COLLECTION_NAME)
        .find_one(character_filter).await else {return Err("create_character__database_error".into())};

    let player_move = PlayerMove{
        _id: Default::default(),
        universe_id: server.universe_id,
        server_id: server.server_id,
        actual_space_id: place.category_id,
        actual_space_type: Default::default(),
        is_in_move: false,
        is_end: false,
        step_start_timestamp: None,
        step_end_timestamp: None,
        road_role_id: None,
        road_id: None,
        road_server_id: None,
        destination_id: None,
        destination_role_id: None,
        destination_server_id: None,
        source_id: Some(place.category_id),
        source_role_id: Some(place.role),
        source_server_id: Some(place.server_id),
        modified_speed: 0.0,
        distance_traveled: 0.0,
        user_id: character_user_id,
    };

    // Supprime l'ancien mouvement s'il existe dans un autre univers
    let _ = player_move.remove().await;
    let Ok(_) = player_move.upsert().await else {return Err("create_character__database_error".into())};

    let original_embed: CreateEmbed = component_interaction.message.embeds[0].clone().into();
    let _ = component_interaction.channel_id.edit_message(
        &ctx,
        component_interaction.message.id,
        EditMessage::new().components(vec![]).embed(
            original_embed.color(Color::from_rgb(0, 0, 255))
                .field(tr_locale!(component_interaction.locale.as_str(), "create_character__start_place"), place.name, false)
        ),
    ).await;

    Ok("accept_character")
}
