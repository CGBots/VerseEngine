//! Joueur sans personnage : demander un personnage depuis l'activité, ou regarder en spectateur.
//!
//! - `POST /api/me/character_request` : envoie la fiche (nom, description, histoire, demande
//!   particulière) dans le salon de création de personnage du serveur, déjà soumise. Les
//!   modérateurs l'acceptent ou la refusent avec les boutons habituels (`/character_create`).
//! - `POST /api/me/spectator` : donne le rôle spectateur du serveur s'il est configuré.
//! - `GET /api/spectator/players` : tous les groupes et leur position, pour la carte du
//!   spectateur (dévoilée, sans brouillard, routes secrètes exclues : voir `http::map`).
//!
//! Spectateur = session sans personnage dans l'univers ; le mode admin y a aussi droit.

use axum::extract::State;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use serenity::all::{ButtonStyle, ChannelId, Color, CreateActionRow, CreateButton, CreateEmbed, CreateEmbedAuthor, CreateEmbedFooter, CreateMessage, GuildId, RoleId, UserId};

use crate::activity::dto::LocationDto;
use crate::activity::error::{ApiError, ApiResult};
use crate::activity::http::{bot_http, json_body, now_secs, AppState, Authed};
use crate::activity::session::Session;
use crate::activity::travel_view::TravelView;
use crate::characters::create_character_sub_command as cc;
use crate::translation::get_by_locale;

pub const MAX_NAME: usize = 32;
pub const MAX_TEXT: usize = 1024;

/// La session regarde-t-elle en spectateur ? Pas de personnage dans l'univers.
pub async fn is_spectator(s: &Session) -> ApiResult<bool> {
    let universe_id = s.universe()?;
    Ok(crate::database::characters::get_character_by_user_id(universe_id, s.user_id).await?.is_none())
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CharacterRequest {
    name: String,
    description: String,
    story: String,
    #[serde(default)]
    special_request: String,
}

/// Champs nettoyés et vérifiés (mêmes limites que la fenêtre de `/character_create`).
pub fn validate(r: &CharacterRequest) -> ApiResult<(String, String, String, String)> {
    let clean = |s: &str| s.trim().to_string();
    let (name, description, story, special) = (clean(&r.name), clean(&r.description), clean(&r.story), clean(&r.special_request));
    let len = |s: &str| s.chars().count();
    if name.is_empty() || description.is_empty() || story.is_empty() {
        return Err(ApiError::bad_request("activity__character_request_missing"));
    }
    if len(&name) > MAX_NAME || len(&description) > MAX_TEXT || len(&story) > MAX_TEXT || len(&special) > MAX_TEXT {
        return Err(ApiError::bad_request("activity__character_request_too_long"));
    }
    Ok((name, description, story, special))
}

async fn character_request(
    State(state): State<AppState>,
    Authed(s): Authed,
    body: Result<Json<CharacterRequest>, axum::extract::rejection::JsonRejection>,
) -> ApiResult<Json<serde_json::Value>> {
    let req = json_body(body)?;
    let (name, description, story, special) = validate(&req)?;
    state.write_limiter.check(&format!("user:{}", s.user_id)).map_err(ApiError::too_many)?;
    if !is_spectator(&s).await? {
        return Err(ApiError::new(axum::http::StatusCode::CONFLICT, "create_character__character_already_existing"));
    }
    let server = crate::database::server::get_server_by_id(s.guild_id).await?.ok_or_else(|| ApiError::not_found("activity__server_not_linked"))?;
    let channel = server.rp_character_channel_id.ok_or_else(|| ApiError::not_found("activity__character_channel_missing"))?;
    let http = bot_http(&state)?;
    let locale = s.locale;
    let tr = |k: &str| get_by_locale(locale, k, None, None);
    // Même fiche que `/character_create`, directement soumise : boutons Accepter / Refuser.
    let embed = CreateEmbed::new()
        .footer(CreateEmbedFooter::new(s.user_id.to_string()))
        .title(name)
        .field(tr(cc::CHARACTER_DESCRIPTION), description, true)
        .field(tr(cc::CHARACTER_STORY), story, true)
        .field(tr(cc::CHARACTER_SPECIAL_REQUEST), special, false)
        .author(CreateEmbedAuthor::new(s.username.as_str()))
        .color(Color::from_rgb(0, 255, 0));
    let buttons = vec![CreateActionRow::Buttons(vec![
        CreateButton::new(cc::ACCEPT_CHARACTER_BUTTON_CUSTOM_ID).label(tr(cc::ACCEPT_CHARACTER_BUTTON_CUSTOM_ID)).style(ButtonStyle::Success),
        CreateButton::new(cc::REJECT_CHARACTER_BUTTON_CUSTOM_ID).label(tr(cc::REJECT_CHARACTER_BUTTON_CUSTOM_ID)).style(ButtonStyle::Danger),
    ])];
    let message = ChannelId::new(channel.id)
        .send_message(http.as_ref(), CreateMessage::new().content(format!("<@{}>", s.user_id)).embed(embed).components(buttons))
        .await
        .map_err(|e| {
            tracing::warn!(error = %e, "activity: fiche de personnage non envoyée");
            ApiError::upstream()
        })?;
    if let Some(log) = server.log_channel_id {
        let text = format!("{} {}", tr(cc::CREATE_CHARACTER_SUBMIT_NOTIFICATION), message.link());
        let _ = ChannelId::new(log.id).send_message(http.as_ref(), CreateMessage::new().content(text)).await;
    }
    Ok(Json(serde_json::json!({ "result": "activity__character_request_sent", "url": message.link() })))
}

async fn become_spectator(State(state): State<AppState>, Authed(s): Authed) -> ApiResult<Json<serde_json::Value>> {
    if !is_spectator(&s).await? {
        return Err(ApiError::new(axum::http::StatusCode::CONFLICT, "create_character__character_already_existing"));
    }
    let server = crate::database::server::get_server_by_id(s.guild_id).await?;
    let mut role_given = false;
    if let Some(role) = server.and_then(|sv| sv.spectator_role_id) {
        let http = bot_http(&state)?;
        role_given = http
            .add_member_role(GuildId::new(s.guild_id), UserId::new(s.user_id), RoleId::new(role.id), Some("VerseEngine : mode spectateur choisi dans l'activité"))
            .await
            .map_err(|e| tracing::warn!(error = %e, "activity: rôle spectateur non donné"))
            .is_ok();
    }
    Ok(Json(serde_json::json!({ "role_given": role_given })))
}

#[derive(Serialize)]
struct MemberDto {
    user_id: String,
    name: String,
}

#[derive(Serialize)]
struct GroupDto {
    members: Vec<MemberDto>,
    location: Option<LocationDto>,
    travel: Option<TravelView>,
}

async fn players(Authed(s): Authed) -> ApiResult<Json<serde_json::Value>> {
    if !(s.access.can_use_admin_mode() || is_spectator(&s).await?) {
        return Err(ApiError::forbidden("activity__spectator_only"));
    }
    let universe_id = s.universe()?;
    let now = now_secs();
    let characters = crate::activity::admin_panel::characters_of(universe_id).await?;
    let groups = crate::activity::admin_panel::groups_of(universe_id).await?;
    let mut out = Vec::new();
    for g in &groups {
        let members: Vec<MemberDto> = g
            .members
            .iter()
            .filter_map(|m| characters.iter().find(|c| c.user_id == *m).map(|c| MemberDto { user_id: m.to_string(), name: c.name.clone() }))
            .collect();
        if members.is_empty() {
            continue;
        }
        let (location, travel, _) = crate::activity::http::group_state(universe_id, Some(g), now).await?;
        out.push(GroupDto { members, location, travel });
    }
    Ok(Json(serde_json::json!({ "groups": out, "server_time": now })))
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/me/character_request", post(character_request))
        .route("/me/spectator", post(become_spectator))
        .route("/spectator/players", get(players))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn req(name: &str, description: &str, story: &str, special: &str) -> CharacterRequest {
        CharacterRequest { name: name.into(), description: description.into(), story: story.into(), special_request: special.into() }
    }

    #[test]
    fn character_requests_follow_the_modal_limits() {
        assert!(validate(&req(" Aldric ", "Marchand", "Né à Kerival", "")).is_ok());
        assert_eq!(validate(&req("Aldric", "", "x", "")).unwrap_err().key, "activity__character_request_missing");
        assert_eq!(validate(&req(&"a".repeat(33), "d", "s", "")).unwrap_err().key, "activity__character_request_too_long");
        assert!(validate(&req(&"é".repeat(32), &"d".repeat(1024), "s", "")).is_ok(), "limits count characters, not bytes");
        assert_eq!(validate(&req("A", "d", "s", &"x".repeat(1025))).unwrap_err().key, "activity__character_request_too_long");
    }
}
