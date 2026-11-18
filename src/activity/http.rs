//! Routeur axum de l'activité.

use std::sync::Arc;
use std::time::Duration;

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{FromRequestParts, State};
use axum::http::request::Parts;
use axum::http::{header, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use futures::future::join_all;
use futures::TryStreamExt;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tower_http::limit::RequestBodyLimitLayer;
use tower_http::set_header::SetResponseHeaderLayer;

use crate::activity::discord_api::{DiscordError, DiscordGateway};
use crate::activity::dto::*;
use crate::activity::error::{ApiError, ApiResult};
use crate::activity::permissions::{compute_access, AccessInput};
use crate::activity::ratelimit::RateLimiter;
use crate::activity::session::{Session, SessionStore};
use crate::activity::map_view::{build_map_dto, computed_map, graph_hash, needs_layout, MapDto};
use crate::activity::travel_view::{travel_view, TravelView};
use crate::database::travel::{get_group_of_user, TravelGroup};
use crate::travel::service::{road_is_visible, Announce};
use std::collections::HashSet;
use crate::database::stats::{Stat, SPEED_STAT};
use crate::database::travel::SpaceType;

/// Corps de requête max pour les routes standard.
pub const MAX_BODY_BYTES: usize = 64 * 1024;

#[derive(Clone)]
pub struct AppState {
    pub sessions: Arc<SessionStore>,
    pub discord: Arc<dyn DiscordGateway>,
    /// /api/auth/token, par instance d'activité.
    pub auth_limiter: Arc<RateLimiter>,
    /// /api/auth/token, global (l'id d'instance est choisi par le client : sans ce plafond,
    /// en changer à chaque requête contournerait la limite ci-dessus).
    pub auth_global_limiter: Arc<RateLimiter>,
    /// Routes authentifiées, par utilisateur.
    pub api_limiter: Arc<RateLimiter>,
    /// Actions d'écriture (voyage…), par utilisateur : 1 req/s soutenu.
    pub write_limiter: Arc<RateLimiter>,
    /// Joueurs dont une action de jeu est en cours (un double clic ne lance pas deux départs).
    pub actions_in_flight: Arc<std::sync::Mutex<std::collections::HashSet<u64>>>,
    /// Client HTTP du bot, pour les actions de jeu (messages publics, rôles). `None` en test.
    pub bot_http: Option<Arc<serenity::all::Http>>,
}

impl AppState {
    pub fn new(discord: Arc<dyn DiscordGateway>, session_ttl: Duration) -> Self {
        Self {
            sessions: Arc::new(SessionStore::new(session_ttl)),
            discord,
            auth_limiter: Arc::new(RateLimiter::new(10, 10.0 / 60.0)),
            auth_global_limiter: Arc::new(RateLimiter::new(120, 5.0)),
            api_limiter: Arc::new(RateLimiter::new(20, 5.0)),
            write_limiter: Arc::new(RateLimiter::new(3, 1.0)),
            actions_in_flight: Arc::default(),
            bot_http: None,
        }
    }

    pub fn with_bot_http(mut self, http: Arc<serenity::all::Http>) -> Self {
        self.bot_http = Some(http);
        self
    }
}

pub fn now_secs() -> u64 { chrono::Utc::now().timestamp().max(0) as u64 }

pub fn router(state: AppState) -> Router {
    let api = Router::new()
        .route("/auth/token", post(auth_token))
        .route("/me", get(me))
        .route("/me/character", get(me_character))
        .route("/map", get(map))
        .route("/me/travel", get(me_travel))
        .route("/me/travel/start", post(travel_start))
        .route("/me/travel/stop", post(travel_stop))
        .route("/me/travel/join", post(travel_join))
        .route("/me/travel/leave", post(travel_leave))
        .route("/ws", get(ws_upgrade))
        .route("/i18n/{locale}", get(i18n))
        .fallback(|| async { ApiError::not_found("activity__route_not_found") });

    Router::new()
        .nest("/api", api)
        .layer(RequestBodyLimitLayer::new(MAX_BODY_BYTES))
        .layer(SetResponseHeaderLayer::if_not_present(header::X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff")))
        .layer(SetResponseHeaderLayer::if_not_present(header::REFERRER_POLICY, HeaderValue::from_static("no-referrer")))
        .layer(SetResponseHeaderLayer::if_not_present(header::CACHE_CONTROL, HeaderValue::from_static("no-store")))
        .with_state(state)
}

// ---------------------------------------------------------------------------
// Validation des entrées
// ---------------------------------------------------------------------------

/// Code OAuth2 Discord : alphanumérique (+ `-_`), 1..=128 caractères.
pub fn valid_code(code: &str) -> bool {
    (1..=128).contains(&code.len()) && code.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

/// Identifiant d'instance d'activité (ex. `i-1234567890-gc-…`).
pub fn valid_instance_id(id: &str) -> bool {
    (1..=128).contains(&id.len()) && id.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b':' | b'.'))
}

// ---------------------------------------------------------------------------
// Extracteur de session
// ---------------------------------------------------------------------------

pub struct Authed(pub Session);

fn bearer(parts: &Parts) -> Option<&str> {
    parts.headers.get(header::AUTHORIZATION)?.to_str().ok()?.strip_prefix("Bearer ").map(str::trim)
}

impl FromRequestParts<AppState> for Authed {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, Self::Rejection> {
        let token = bearer(parts).ok_or_else(ApiError::unauthorized)?;
        let session = state.sessions.get(token).ok_or_else(ApiError::unauthorized)?;
        state.api_limiter.check(&format!("user:{}", session.user_id)).map_err(ApiError::too_many)?;
        Ok(Authed(session))
    }
}

// ---------------------------------------------------------------------------
// POST /api/auth/token
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TokenRequest {
    code: String,
    instance_id: String,
    /// Langue du client Discord (`userSettingsGetLocale`), facultative.
    #[serde(default)]
    locale: Option<String>,
}

/// Langue des messages produits pour ce joueur : `fr` par défaut, `en-US` pour toute variante
/// d'anglais (le bot n'a que ces deux fichiers de traduction).
pub fn normalize_locale(locale: Option<&str>) -> &'static str {
    match locale {
        Some(l) if l.len() <= 16 && l.to_ascii_lowercase().starts_with("en") => "en-US",
        _ => "fr",
    }
}

fn map_discord(e: DiscordError) -> ApiError {
    match e {
        DiscordError::Rejected => ApiError::unauthorized(),
        DiscordError::Upstream(msg) => {
            tracing::warn!(%msg, "activity: erreur Discord");
            ApiError::upstream()
        }
    }
}

async fn auth_token(State(state): State<AppState>, body: Result<Json<TokenRequest>, axum::extract::rejection::JsonRejection>) -> ApiResult<Response> {
    let Json(req) = body.map_err(|r| {
        if r.status() == StatusCode::PAYLOAD_TOO_LARGE {
            ApiError::new(StatusCode::PAYLOAD_TOO_LARGE, "activity__payload_too_large")
        } else {
            ApiError::bad_request("activity__invalid_request")
        }
    })?;
    if !valid_code(&req.code) || !valid_instance_id(&req.instance_id) {
        return Err(ApiError::bad_request("activity__invalid_request"));
    }
    state.auth_global_limiter.check("global").map_err(ApiError::too_many)?;
    state.auth_limiter.check(&format!("instance:{}", req.instance_id)).map_err(ApiError::too_many)?;

    // 1. Échange du code (client secret côté serveur uniquement).
    let grant = state.discord.exchange_code(&req.code).await.map_err(map_discord)?;
    if !grant.scope.split_whitespace().any(|s| s == "identify") {
        return Err(ApiError::forbidden("activity__missing_scope"));
    }
    // 2. Identité réelle de l'utilisateur.
    let user = state.discord.current_user(&grant.access_token).await.map_err(map_discord)?;
    // 3. L'instance existe, l'utilisateur y participe, et elle est dans une guilde.
    let instance = state.discord.activity_instance(&req.instance_id).await.map_err(map_discord)?;
    if !instance.users.contains(&user.id) {
        return Err(ApiError::forbidden("activity__not_in_instance"));
    }
    let guild_id = instance.location.as_ref().and_then(|l| l.guild_id).ok_or_else(|| ApiError::forbidden("activity__guild_only"))?;
    // 4. Guilde → serveur VerseEngine → univers.
    let server = crate::database::server::get_server_by_id(guild_id).await?.ok_or_else(|| ApiError::not_found("activity__server_not_linked"))?;
    let universe = crate::database::universe::get_universe_by_id(server.universe_id)
        .await?
        .ok_or_else(|| ApiError::not_found("activity__universe_not_found"))?;
    // 5. Appartenance et droits, lus par le bot.
    let member = state.discord.member(guild_id, user.id).await.map_err(map_discord)?.ok_or_else(|| ApiError::forbidden("activity__not_a_member"))?;
    let access = compute_access(&AccessInput {
        user_id: user.id,
        guild_owner_id: member.guild_owner_id,
        member_roles: &member.roles,
        guild_roles: &member.guild_roles,
        guild_id,
        admin_role_id: server.admin_role_id.map(|i| i.id),
        moderator_role_id: server.moderator_role_id.map(|i| i.id),
    });

    let session_token = state.sessions.create(Session {
        user_id: user.id,
        username: user.global_name.clone().unwrap_or_else(|| user.username.clone()),
        guild_id,
        universe_id: server.universe_id,
        instance_id: req.instance_id.clone(),
        locale: normalize_locale(req.locale.as_deref()),
        access,
        expires_at: std::time::Instant::now(),
    });
    tracing::info!(user = user.id, guild = guild_id, ?access, "activity: session ouverte");

    Ok(Json(json!({
        "access_token": grant.access_token,
        "session_token": session_token,
        "expires_in": state.sessions.ttl().as_secs(),
        "user": UserDto { id: user.id.to_string(), username: user.username, global_name: user.global_name, avatar: user.avatar },
        "guild_id": guild_id.to_string(),
        "universe": UniverseDto { id: universe.universe_id.to_hex(), name: universe.name },
        "access": access,
        "admin_mode_available": access.can_use_admin_mode(),
        "server_time": now_secs(),
    }))
    .into_response())
}

// ---------------------------------------------------------------------------
// GET /api/me
// ---------------------------------------------------------------------------

async fn me(Authed(s): Authed) -> Json<serde_json::Value> {
    Json(json!({
        "user_id": s.user_id.to_string(),
        "username": s.username,
        "guild_id": s.guild_id.to_string(),
        "universe_id": s.universe_id.to_hex(),
        "access": s.access,
        "admin_mode_available": s.access.can_use_admin_mode(),
        "server_time": now_secs(),
    }))
}

// ---------------------------------------------------------------------------
// GET /api/me/character
// ---------------------------------------------------------------------------

async fn me_character(Authed(s): Authed) -> ApiResult<Json<CharacterDto>> {
    use crate::database::characters::get_character_by_user_id;
    let character = get_character_by_user_id(s.universe_id, s.user_id).await?.ok_or_else(|| ApiError::not_found("activity__no_character"))?;
    let universe = crate::database::universe::get_universe_by_id(s.universe_id).await?.ok_or_else(|| ApiError::not_found("activity__universe_not_found"))?;
    let universe_stats: Vec<Stat> = universe.get_stats().await?.try_collect().await?;
    let group = character.clone().get_player_move().await?;

    // Statistiques résolues avec la même fonction que le bot (`Stat::resolve`).
    let space_id = group.as_ref().map(|g| g.actual_space_id).unwrap_or(0);
    let resolved = join_all(universe_stats.iter().map(|st| {
        let st = st.clone();
        async move {
            let r = st.clone().resolve(space_id, s.user_id).await.ok().map(|(v, _)| v);
            (st, r)
        }
    }))
    .await;
    let mut speed = None;
    let stats = resolved
        .into_iter()
        .filter_map(|(st, v)| {
            let v = v?;
            if st.name == SPEED_STAT { speed = Some(v.as_f64()); }
            Some(StatDto { id: st._id.to_hex(), name: st.name.clone(), value: stat_value_json(&v) })
        })
        .collect();

    let statuses = character
        .stats
        .iter()
        .flat_map(|st| st.modifiers.iter().filter(|m| m.is_active()).map(move |m| StatusDto::from_modifier(&st.name, m)))
        .collect();

    let now = now_secs();
    let (location, travel, group_members) = group_state(s.universe_id, group.as_ref(), now).await?;

    Ok(Json(CharacterDto {
        id: character._id.to_hex(),
        name: character.name,
        description: character.description,
        // Résolution de la référence d'image en URL : phase b (via le client HTTP du bot).
        image_url: None,
        speed,
        stats,
        statuses,
        location,
        travel,
        group_members,
        server_time: now,
    }))
}

/// Position, voyage et membres du groupe d'un joueur.
async fn group_state(universe_id: mongodb::bson::oid::ObjectId, group: Option<&TravelGroup>, now: u64) -> ApiResult<(Option<LocationDto>, Option<TravelView>, Vec<String>)> {
    let Some(g) = group else { return Ok((None, None, vec![])) };
    let (location, road_distance) = match g.actual_space_type {
        SpaceType::Place => {
            let place = crate::database::places::get_place_by_category_id(universe_id, g.actual_space_id).await?;
            (LocationDto { kind: "place", id: g.actual_space_id.to_string(), name: place.map(|p| p.name), destination_id: None, destination_name: None }, None)
        }
        SpaceType::Road => {
            let road = crate::database::road::get_road_by_channel_id(universe_id, g.actual_space_id).await?;
            let dest_name = match g.destination_id {
                Some(d) => crate::database::places::get_place_by_category_id(universe_id, d).await?.map(|p| p.name),
                None => None,
            };
            let dist = road.as_ref().map(|r| r.distance as f64);
            (
                LocationDto {
                    kind: "road",
                    id: g.actual_space_id.to_string(),
                    name: road.map(|r| r.road_name),
                    destination_id: g.destination_id.map(|d| d.to_string()),
                    destination_name: dest_name,
                },
                dist,
            )
        }
    };
    Ok((Some(location), Some(travel_view(g, road_distance, now)), g.members.iter().map(u64::to_string).collect()))
}

// ---------------------------------------------------------------------------
// GET /api/map : carte filtrée côté serveur (routes secrètes, lieux cachés).
// ---------------------------------------------------------------------------

async fn map(Authed(s): Authed) -> ApiResult<Json<MapDto>> {
    use crate::database::places::{get_places_by_universe_id, Place};
    use crate::database::road::{get_roads_by_universe, Road};
    use crate::database::universe_maps::{get_universe_map, save_universe_map};

    let places: Vec<Place> = get_places_by_universe_id(s.universe_id).await?.try_collect().await?;
    let roads: Vec<Road> = get_roads_by_universe(s.universe_id).await?.try_collect().await?;
    let hash = graph_hash(&places, &roads);
    let stored = get_universe_map(s.universe_id).await?;
    let universe_map = match stored {
        Some(m) if !needs_layout(Some(&m), &hash) => m,
        _ => {
            // Calcul O(n²) : hors des threads de l'exécuteur async.
            let (universe_id, p, r, now) = (s.universe_id, places.clone(), roads.clone(), now_secs());
            let computed = tokio::task::spawn_blocking(move || computed_map(universe_id, &p, &r, now)).await.map_err(|e| {
                tracing::error!(error = %e, "activity: échec du placement");
                ApiError::internal()
            })?;
            save_universe_map(&computed).await?;
            computed
        }
    };

    let group = get_group_of_user(s.universe_id, s.user_id).await?;
    let known = crate::database::known_roads::known_road_ids(s.universe_id, s.user_id).await?;
    let current_road = group.as_ref().filter(|g| g.actual_space_type == SpaceType::Road).map(|g| g.actual_space_id);
    let visible: HashSet<u64> =
        roads.iter().filter(|r| road_is_visible(r, &known, current_road)).map(|r| r.channel_id).collect();
    let always: Vec<u64> = group
        .iter()
        .flat_map(|g| {
            let here = (g.actual_space_type == SpaceType::Place).then_some(g.actual_space_id);
            [here, g.source_id, g.destination_id]
        })
        .flatten()
        .collect();
    Ok(Json(build_map_dto(&universe_map, &places, &roads, &visible, &always, now_secs())))
}

// ---------------------------------------------------------------------------
// /api/me/travel* : mêmes services que les commandes slash (`travel::service`).
// ---------------------------------------------------------------------------

/// Vrai si `s` a la forme d'une clé Fluent (`domaine__message`).
pub fn is_fluent_key(s: &str) -> bool {
    match s.split_once("__") {
        Some((domain, rest)) => {
            let ok = |p: &str| !p.is_empty() && p.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_');
            ok(domain) && ok(rest) && s.len() <= 80
        }
        None => false,
    }
}

/// Erreur d'un service de jeu → réponse API. Une clé Fluent est renvoyée telle quelle (le client
/// affiche le même texte que la commande slash) ; tout autre message reste dans les logs.
pub fn service_error(e: &crate::discord::poise_structs::Error) -> ApiError {
    let message = e.to_string();
    if is_fluent_key(&message) {
        // `…_not_found`, `…_no_road_found` : ressource absente ; le reste : règle du jeu non respectée.
        let status = if message.ends_with("_found") { StatusCode::NOT_FOUND } else { StatusCode::CONFLICT };
        ApiError::new(status, message)
    } else {
        tracing::error!(error = %message, "activity: erreur d'un service de jeu");
        ApiError::internal()
    }
}

/// Réserve le droit d'agir pour ce joueur jusqu'à la fin de l'action (libéré au `drop`).
struct ActionGuard {
    set: Arc<std::sync::Mutex<HashSet<u64>>>,
    user_id: u64,
}

impl ActionGuard {
    fn acquire(state: &AppState, user_id: u64) -> ApiResult<Self> {
        state.write_limiter.check(&format!("user:{user_id}")).map_err(ApiError::too_many)?;
        let mut set = state.actions_in_flight.lock().unwrap_or_else(|p| p.into_inner());
        if !set.insert(user_id) {
            return Err(ApiError::new(StatusCode::CONFLICT, "activity__action_in_progress"));
        }
        Ok(Self { set: state.actions_in_flight.clone(), user_id })
    }
}

impl Drop for ActionGuard {
    fn drop(&mut self) {
        self.set.lock().unwrap_or_else(|p| p.into_inner()).remove(&self.user_id);
    }
}

fn bot_http(state: &AppState) -> ApiResult<Arc<serenity::all::Http>> {
    state.bot_http.clone().ok_or_else(ApiError::upstream)
}

/// Corps JSON strict, avec les mêmes erreurs que `/api/auth/token`.
fn json_body<T>(body: Result<Json<T>, axum::extract::rejection::JsonRejection>) -> ApiResult<T> {
    body.map(|Json(v)| v).map_err(|r| {
        if r.status() == StatusCode::PAYLOAD_TOO_LARGE {
            ApiError::new(StatusCode::PAYLOAD_TOO_LARGE, "activity__payload_too_large")
        } else {
            ApiError::bad_request("activity__invalid_request")
        }
    })
}

/// Identifiant Discord transmis en chaîne : 1 à 20 chiffres, non nul.
pub fn parse_snowflake(s: &str) -> ApiResult<u64> {
    if s.is_empty() || s.len() > 20 || !s.bytes().all(|b| b.is_ascii_digit()) {
        return Err(ApiError::bad_request("activity__invalid_request"));
    }
    s.parse::<u64>().ok().filter(|v| *v != 0).ok_or_else(|| ApiError::bad_request("activity__invalid_request"))
}

#[derive(Serialize)]
struct DestinationDto {
    id: String,
    name: String,
    distance_km: f64,
}

#[derive(Serialize)]
struct TravelStateDto {
    location: Option<LocationDto>,
    travel: Option<TravelView>,
    group_members: Vec<String>,
    destinations: Vec<DestinationDto>,
    /// Clé Fluent du résultat de l'action (réponses des POST uniquement).
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<&'static str>,
    server_time: u64,
}

async fn travel_state(s: &Session, result: Option<&'static str>) -> ApiResult<Json<TravelStateDto>> {
    let now = now_secs();
    let group = get_group_of_user(s.universe_id, s.user_id).await?;
    let (location, travel, group_members) = group_state(s.universe_id, group.as_ref(), now).await?;
    let destinations = match &group {
        Some(_) => crate::travel::service::destinations(s.guild_id, s.user_id)
            .await
            .map_err(|e| service_error(&e))?
            .into_iter()
            .map(|d| DestinationDto { id: d.place.category_id.to_string(), name: d.place.name, distance_km: d.distance_km })
            .collect(),
        None => vec![],
    };
    Ok(Json(TravelStateDto { location, travel, group_members, destinations, result, server_time: now }))
}

async fn me_travel(Authed(s): Authed) -> ApiResult<Json<TravelStateDto>> {
    travel_state(&s, None).await
}

fn actor(s: &Session) -> crate::travel::service::Actor<'_> {
    crate::travel::service::Actor { user_id: s.user_id, fallback_name: &s.username, locale: s.locale }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StartRequest {
    destination_id: String,
}

async fn travel_start(
    State(state): State<AppState>,
    Authed(s): Authed,
    body: Result<Json<StartRequest>, axum::extract::rejection::JsonRejection>,
) -> ApiResult<Json<TravelStateDto>> {
    let req = json_body(body)?;
    let destination_id = parse_snowflake(&req.destination_id)?;
    let _guard = ActionGuard::acquire(&state, s.user_id)?;
    let http = bot_http(&state)?;
    let key = crate::travel::service::start(&http, s.guild_id, actor(&s), destination_id, Announce::AtPosition)
        .await
        .map_err(|e| service_error(&e))?;
    travel_state(&s, Some(key)).await
}

async fn travel_stop(State(state): State<AppState>, Authed(s): Authed) -> ApiResult<Json<TravelStateDto>> {
    let _guard = ActionGuard::acquire(&state, s.user_id)?;
    let key = crate::travel::service::stop(s.guild_id, s.user_id).await.map_err(|e| service_error(&e))?;
    travel_state(&s, Some(key)).await
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JoinRequest {
    target_user_id: String,
}

async fn travel_join(
    State(state): State<AppState>,
    Authed(s): Authed,
    body: Result<Json<JoinRequest>, axum::extract::rejection::JsonRejection>,
) -> ApiResult<Json<TravelStateDto>> {
    let req = json_body(body)?;
    let target_id = parse_snowflake(&req.target_user_id)?;
    let _guard = ActionGuard::acquire(&state, s.user_id)?;
    let http = bot_http(&state)?;
    let fallback = format!("<@{target_id}>");
    let key = crate::travel::service::join(&http, s.guild_id, actor(&s), target_id, &fallback, Announce::AtPosition)
        .await
        .map_err(|e| service_error(&e))?;
    travel_state(&s, Some(key)).await
}

async fn travel_leave(State(state): State<AppState>, Authed(s): Authed) -> ApiResult<Json<TravelStateDto>> {
    let _guard = ActionGuard::acquire(&state, s.user_id)?;
    let http = bot_http(&state)?;
    let key = crate::travel::service::leave(&http, s.guild_id, actor(&s), Announce::AtPosition).await.map_err(|e| service_error(&e))?;
    travel_state(&s, Some(key)).await
}

// ---------------------------------------------------------------------------
// GET /api/i18n/{locale} : fichiers Fluent du bot (source unique des traductions,
// y compris les clés d'erreur `travel__*` renvoyées par l'API). Public, liste blanche.
// ---------------------------------------------------------------------------

pub const LOCALES: [&str; 2] = ["fr", "en-US"];

static FTL_CACHE: once_cell::sync::Lazy<std::collections::HashMap<&'static str, Option<String>>> = once_cell::sync::Lazy::new(|| {
    LOCALES.iter().map(|l| (*l, std::fs::read_to_string(format!("translations/{l}.ftl")).ok())).collect()
});

async fn i18n(axum::extract::Path(locale): axum::extract::Path<String>) -> ApiResult<Response> {
    let content = LOCALES
        .iter()
        .find(|l| **l == locale)
        .and_then(|l| FTL_CACHE.get(l).cloned().flatten())
        .ok_or_else(|| ApiError::not_found("activity__route_not_found"))?;
    let mut res = content.into_response();
    res.headers_mut().insert(header::CONTENT_TYPE, HeaderValue::from_static("text/plain; charset=utf-8"));
    res.headers_mut().insert(header::CACHE_CONTROL, HeaderValue::from_static("public, max-age=300"));
    Ok(res)
}

// ---------------------------------------------------------------------------
// WebSocket /api/ws : authentification par premier message, puis battements.
// Les évènements (change streams) arrivent en phase b.
// ---------------------------------------------------------------------------

const WS_MAX_MESSAGE: usize = 4 * 1024;

async fn ws_upgrade(State(state): State<AppState>, ws: WebSocketUpgrade) -> Response {
    ws.max_message_size(WS_MAX_MESSAGE).max_frame_size(WS_MAX_MESSAGE).on_upgrade(move |socket| ws_session(state, socket))
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum ClientMessage {
    Auth { token: String },
    Ping,
}

async fn ws_session(state: AppState, mut socket: WebSocket) {
    let auth = tokio::time::timeout(Duration::from_secs(10), socket.recv()).await;
    let session = match auth {
        Ok(Some(Ok(Message::Text(t)))) => match serde_json::from_str::<ClientMessage>(&t) {
            Ok(ClientMessage::Auth { token }) => state.sessions.get(&token),
            _ => None,
        },
        _ => None,
    };
    let Some(session) = session else {
        let _ = socket.send(Message::Text(json!({"type": "error", "error": "activity__unauthorized"}).to_string().into())).await;
        let _ = socket.send(Message::Close(None)).await;
        return;
    };
    if socket.send(Message::Text(json!({"type": "hello", "server_time": now_secs()}).to_string().into())).await.is_err() {
        return;
    }
    let mut heartbeat = tokio::time::interval(Duration::from_secs(25));
    heartbeat.tick().await;
    loop {
        tokio::select! {
            _ = heartbeat.tick() => {
                if session.expires_at <= std::time::Instant::now() { break; }
                if socket.send(Message::Text(json!({"type": "heartbeat", "server_time": now_secs()}).to_string().into())).await.is_err() { break; }
            }
            msg = socket.recv() => match msg {
                Some(Ok(Message::Text(t))) => {
                    if state.api_limiter.check(&format!("user:{}", session.user_id)).is_err() { break; }
                    if let Ok(ClientMessage::Ping) = serde_json::from_str::<ClientMessage>(&t) {
                        if socket.send(Message::Text(json!({"type": "pong", "server_time": now_secs()}).to_string().into())).await.is_err() { break; }
                    }
                }
                Some(Ok(Message::Close(_))) | None | Some(Err(_)) => break,
                _ => {}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::activity::discord_api::*;
    use crate::activity::session::test_session;
    use axum::body::{to_bytes, Body};
    use axum::http::Request;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tower::ServiceExt;

    /// Faux Discord : compte les appels pour vérifier qu'une requête invalide n'atteint jamais Discord.
    #[derive(Default)]
    struct FakeDiscord {
        calls: AtomicUsize,
        scope: &'static str,
        instance_users: Vec<u64>,
        guild: Option<u64>,
    }

    impl DiscordGateway for FakeDiscord {
        fn exchange_code<'a>(&'a self, code: &'a str) -> BoxFuture<'a, Result<TokenGrant, DiscordError>> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            let ok = code == "good";
            let scope = self.scope.to_string();
            Box::pin(async move {
                if ok { Ok(TokenGrant { access_token: "at".into(), expires_in: 600, scope }) } else { Err(DiscordError::Rejected) }
            })
        }
        fn current_user<'a>(&'a self, _t: &'a str) -> BoxFuture<'a, Result<DiscordUser, DiscordError>> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Box::pin(async { Ok(DiscordUser { id: 42, username: "u".into(), global_name: None, avatar: None }) })
        }
        fn activity_instance<'a>(&'a self, id: &'a str) -> BoxFuture<'a, Result<ActivityInstance, DiscordError>> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            let users = self.instance_users.clone();
            let guild = self.guild;
            let id = id.to_string();
            Box::pin(async move {
                Ok(ActivityInstance { instance_id: id, location: Some(InstanceLocation { guild_id: guild, channel_id: Some(5) }), users })
            })
        }
        fn member<'a>(&'a self, _g: u64, _u: u64) -> BoxFuture<'a, Result<Option<MemberInfo>, DiscordError>> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Box::pin(async { Ok(None) })
        }
    }

    fn app_with(fake: Arc<FakeDiscord>) -> (Router, AppState) {
        let state = AppState::new(fake, Duration::from_secs(60));
        (router(state.clone()), state)
    }

    async fn call(app: Router, req: Request<Body>) -> (StatusCode, serde_json::Value, axum::http::HeaderMap) {
        let res = app.oneshot(req).await.unwrap();
        let status = res.status();
        let headers = res.headers().clone();
        let bytes = to_bytes(res.into_body(), 1 << 20).await.unwrap();
        (status, serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null), headers)
    }

    fn post_token(body: &str) -> Request<Body> {
        Request::post("/api/auth/token").header("content-type", "application/json").body(Body::from(body.to_owned())).unwrap()
    }

    /// Chaque clé `activity__*` utilisée dans le code existe dans les deux langues.
    #[test]
    fn every_activity_key_is_translated() {
        let re = regex::Regex::new(r#""(activity__[a-z_]+)""#).unwrap();
        let mut keys = std::collections::BTreeSet::new();
        for entry in std::fs::read_dir("src/activity").unwrap().chain(std::fs::read_dir("src/activity/map").unwrap()) {
            let path = entry.unwrap().path();
            if path.extension().is_some_and(|e| e == "rs") {
                let src = std::fs::read_to_string(&path).unwrap();
                keys.extend(re.captures_iter(&src).map(|c| c[1].to_string()));
            }
        }
        assert!(keys.len() > 10);
        crate::translation::read_ftl().expect("les fichiers Fluent du bot doivent rester valides");
        for locale in LOCALES {
            let ftl = std::fs::read_to_string(format!("translations/{locale}.ftl")).unwrap();
            for k in &keys {
                assert!(ftl.lines().any(|l| l.starts_with(&format!("{k} ="))), "clé {k} absente de {locale}.ftl");
            }
        }
    }

    #[tokio::test]
    async fn i18n_serves_whitelisted_locales_only() {
        let (app, _) = app_with(Arc::default());
        let res = app.clone().oneshot(Request::get("/api/i18n/fr").body(Body::empty()).unwrap()).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let body = to_bytes(res.into_body(), 1 << 22).await.unwrap();
        assert!(String::from_utf8_lossy(&body).contains("activity_ui__inventory"));
        for bad in ["/api/i18n/de", "/api/i18n/..%2Fsrc%2Fmain.rs", "/api/i18n/fr.ftl"] {
            let (st, _, _) = call(app.clone(), Request::get(bad).body(Body::empty()).unwrap()).await;
            assert_eq!(st, StatusCode::NOT_FOUND, "{bad}");
        }
    }

    #[test]
    fn input_validation() {
        assert!(valid_code("AbC_12-x"));
        assert!(!valid_code(""));
        assert!(!valid_code("a b"));
        assert!(!valid_code(&"a".repeat(129)));
        assert!(valid_instance_id("i-1234567890-gc-111-222"));
        assert!(!valid_instance_id("i/../../x"));
        assert!(!valid_instance_id(""));
    }

    #[tokio::test]
    async fn me_requires_session() {
        let (app, _) = app_with(Arc::default());
        let (st, body, _) = call(app.clone(), Request::get("/api/me").body(Body::empty()).unwrap()).await;
        assert_eq!(st, StatusCode::UNAUTHORIZED);
        assert_eq!(body["error"], "activity__unauthorized");
        let req = Request::get("/api/me").header("authorization", format!("Bearer {}", "a".repeat(64))).body(Body::empty()).unwrap();
        assert_eq!(call(app, req).await.0, StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn me_returns_session_without_trusting_client() {
        let (app, state) = app_with(Arc::default());
        let token = state.sessions.create(test_session(77));
        let req = Request::get("/api/me").header("authorization", format!("Bearer {token}")).body(Body::empty()).unwrap();
        let (st, body, headers) = call(app, req).await;
        assert_eq!(st, StatusCode::OK);
        assert_eq!(body["user_id"], "77");
        assert_eq!(body["admin_mode_available"], false);
        assert_eq!(headers.get("x-content-type-options").unwrap(), "nosniff");
        assert_eq!(headers.get("cache-control").unwrap(), "no-store");
    }

    #[tokio::test]
    async fn token_rejects_malformed_bodies_without_calling_discord() {
        let fake = Arc::new(FakeDiscord::default());
        let (app, _) = app_with(fake.clone());
        for body in [
            "not json",
            r#"{"code":"good"}"#,
            r#"{"code":"good","instance_id":"i-1","guild_id":"123"}"#,
            r#"{"code":"bad code","instance_id":"i-1"}"#,
            r#"{"code":"good","instance_id":"../x"}"#,
        ] {
            let (st, b, _) = call(app.clone(), post_token(body)).await;
            assert_eq!(st, StatusCode::BAD_REQUEST, "{body}");
            assert_eq!(b["error"], "activity__invalid_request");
        }
        assert_eq!(fake.calls.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn token_rejects_bad_code_and_missing_scope() {
        let (app, _) = app_with(Arc::new(FakeDiscord { scope: "identify", ..Default::default() }));
        assert_eq!(call(app, post_token(r#"{"code":"nope","instance_id":"i-1"}"#)).await.0, StatusCode::UNAUTHORIZED);
        let (app, _) = app_with(Arc::new(FakeDiscord { scope: "guilds", ..Default::default() }));
        let (st, b, _) = call(app, post_token(r#"{"code":"good","instance_id":"i-1"}"#)).await;
        assert_eq!(st, StatusCode::FORBIDDEN);
        assert_eq!(b["error"], "activity__missing_scope");
    }

    #[tokio::test]
    async fn token_rejects_user_absent_from_instance_or_outside_guild() {
        let (app, _) = app_with(Arc::new(FakeDiscord { scope: "identify", instance_users: vec![1, 2], guild: Some(9), ..Default::default() }));
        let (st, b, _) = call(app, post_token(r#"{"code":"good","instance_id":"i-1"}"#)).await;
        assert_eq!(st, StatusCode::FORBIDDEN);
        assert_eq!(b["error"], "activity__not_in_instance");

        let (app, _) = app_with(Arc::new(FakeDiscord { scope: "identify", instance_users: vec![42], guild: None, ..Default::default() }));
        let (st, b, _) = call(app, post_token(r#"{"code":"good","instance_id":"i-1"}"#)).await;
        assert_eq!(st, StatusCode::FORBIDDEN);
        assert_eq!(b["error"], "activity__guild_only");
    }

    #[tokio::test]
    async fn token_endpoint_is_rate_limited_per_instance() {
        let (app, _) = app_with(Arc::new(FakeDiscord::default()));
        let mut last = StatusCode::OK;
        let mut headers = None;
        for _ in 0..11 {
            let (st, _, h) = call(app.clone(), post_token(r#"{"code":"nope","instance_id":"i-rl"}"#)).await;
            last = st;
            headers = Some(h);
        }
        assert_eq!(last, StatusCode::TOO_MANY_REQUESTS);
        assert!(headers.unwrap().get("retry-after").is_some());
    }

    #[tokio::test]
    async fn oversized_body_is_rejected() {
        let (app, _) = app_with(Arc::new(FakeDiscord::default()));
        let big = format!(r#"{{"code":"{}","instance_id":"i-1"}}"#, "a".repeat(MAX_BODY_BYTES + 10));
        let (st, _, _) = call(app, post_token(&big)).await;
        assert_eq!(st, StatusCode::PAYLOAD_TOO_LARGE);
    }

    #[tokio::test]
    async fn unknown_api_route_is_json_404() {
        let (app, _) = app_with(Arc::new(FakeDiscord::default()));
        let (st, b, _) = call(app, Request::get("/api/nope").body(Body::empty()).unwrap()).await;
        assert_eq!(st, StatusCode::NOT_FOUND);
        assert_eq!(b["error"], "activity__route_not_found");
    }

    // ---- routes de voyage et carte ----

    fn authed(method: &str, uri: &str, token: &str, body: Option<&str>) -> Request<Body> {
        let b = Request::builder().method(method).uri(uri).header("authorization", format!("Bearer {token}"));
        match body {
            Some(json) => b.header("content-type", "application/json").body(Body::from(json.to_owned())).unwrap(),
            None => b.body(Body::empty()).unwrap(),
        }
    }

    #[tokio::test]
    async fn travel_and_map_routes_require_a_session() {
        let (app, _) = app_with(Arc::default());
        for (method, uri) in [
            ("GET", "/api/map"),
            ("GET", "/api/me/travel"),
            ("POST", "/api/me/travel/start"),
            ("POST", "/api/me/travel/stop"),
            ("POST", "/api/me/travel/join"),
            ("POST", "/api/me/travel/leave"),
        ] {
            let req = Request::builder().method(method).uri(uri).body(Body::empty()).unwrap();
            assert_eq!(call(app.clone(), req).await.0, StatusCode::UNAUTHORIZED, "{method} {uri}");
        }
    }

    #[tokio::test]
    async fn travel_bodies_are_validated_before_anything_else() {
        let (app, state) = app_with(Arc::default());
        let token = state.sessions.create(test_session(5));
        for (uri, body) in [
            ("/api/me/travel/start", "{}"),
            ("/api/me/travel/start", r#"{"destination_id":"12","extra":1}"#),
            ("/api/me/travel/start", r#"{"destination_id":"abc"}"#),
            ("/api/me/travel/start", r#"{"destination_id":"0"}"#),
            ("/api/me/travel/start", r#"{"destination_id":"123456789012345678901"}"#),
            ("/api/me/travel/start", r#"{"destination_id":12}"#),
            ("/api/me/travel/join", r#"{"target_user_id":"-1"}"#),
        ] {
            let (st, b, _) = call(app.clone(), authed("POST", uri, &token, Some(body))).await;
            assert_eq!(st, StatusCode::BAD_REQUEST, "{uri} {body}");
            assert_eq!(b["error"], "activity__invalid_request");
        }
        assert!(state.actions_in_flight.lock().unwrap().is_empty(), "no action was started");
    }

    #[tokio::test]
    async fn a_valid_action_without_the_bot_client_is_a_502() {
        let (app, state) = app_with(Arc::default());
        let token = state.sessions.create(test_session(5));
        let (st, b, _) = call(app, authed("POST", "/api/me/travel/start", &token, Some(r#"{"destination_id":"42"}"#))).await;
        assert_eq!(st, StatusCode::BAD_GATEWAY);
        assert_eq!(b["error"], "activity__discord_unavailable");
        assert!(state.actions_in_flight.lock().unwrap().is_empty(), "the guard is released");
    }

    #[tokio::test]
    async fn write_actions_are_rate_limited_per_user() {
        let (app, state) = app_with(Arc::default());
        let token = state.sessions.create(test_session(6));
        let mut statuses = vec![];
        for _ in 0..4 {
            statuses.push(call(app.clone(), authed("POST", "/api/me/travel/start", &token, Some(r#"{"destination_id":"42"}"#))).await.0);
        }
        assert_eq!(statuses.last(), Some(&StatusCode::TOO_MANY_REQUESTS), "{statuses:?}");
    }

    #[test]
    fn a_second_action_of_the_same_player_waits_for_the_first() {
        let state = AppState::new(Arc::new(FakeDiscord::default()), Duration::from_secs(60));
        let first = ActionGuard::acquire(&state, 9).unwrap();
        let second = ActionGuard::acquire(&state, 9);
        assert_eq!(second.err().map(|e| e.key), Some("activity__action_in_progress".to_string()));
        assert!(ActionGuard::acquire(&state, 10).is_ok(), "another player is not blocked");
        drop(first);
        assert!(ActionGuard::acquire(&state, 9).is_ok());
    }

    #[test]
    fn snowflakes_are_parsed_strictly() {
        assert_eq!(parse_snowflake("1080809463201677332").unwrap(), 1080809463201677332);
        for bad in ["", "0", "+1", " 1", "1e3", "18446744073709551616", "abc"] {
            assert!(parse_snowflake(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn service_errors_expose_only_fluent_keys() {
        let key: crate::discord::poise_structs::Error = "travel__no_road_found".into();
        let e = service_error(&key);
        assert_eq!((e.status, e.key.as_str()), (StatusCode::NOT_FOUND, "travel__no_road_found"));
        let rule: crate::discord::poise_structs::Error = "travel__only_leader_can_stop".into();
        assert_eq!(service_error(&rule).status, StatusCode::CONFLICT);
        let internal: crate::discord::poise_structs::Error = "Kind: Server selection timeout, mongodb://admin:secret@db".into();
        let e = service_error(&internal);
        assert_eq!((e.status, e.key.as_str()), (StatusCode::INTERNAL_SERVER_ERROR, "activity__internal_error"));
    }

    #[test]
    fn fluent_keys_are_recognised() {
        assert!(is_fluent_key("travel__cannot_move_while_crafting"));
        for bad in ["travel", "__x", "travel__", "Travel__x", "travel__a b", "No active move found for user 1"] {
            assert!(!is_fluent_key(bad), "{bad}");
        }
    }

    #[test]
    fn locales_are_normalised() {
        assert_eq!(normalize_locale(Some("en-GB")), "en-US");
        assert_eq!(normalize_locale(Some("en-US")), "en-US");
        assert_eq!(normalize_locale(Some("fr")), "fr");
        assert_eq!(normalize_locale(Some("de")), "fr");
        assert_eq!(normalize_locale(None), "fr");
    }

    #[tokio::test]
    async fn the_locale_field_is_accepted_by_the_token_route() {
        let fake = Arc::new(FakeDiscord::default());
        let (app, _) = app_with(fake.clone());
        let (st, _, _) = call(app, post_token(r#"{"code":"nope","instance_id":"i-1","locale":"en-GB"}"#)).await;
        assert_eq!(st, StatusCode::UNAUTHORIZED, "parsed, then rejected by Discord");
        assert_eq!(fake.calls.load(Ordering::SeqCst), 1);
    }

    /// Bout en bout contre MongoDB : la route secrète inconnue n'apparaît pas dans la réponse.
    #[test]
    #[ignore = "integration test: needs a running MongoDB (MONGODB_* env vars); run with --ignored"]
    fn the_map_route_filters_secret_roads() {
        use crate::database::db_namespace::*;
        use crate::database::places::Place;
        use crate::database::travel::TravelGroup;
        use crate::test_support::{block_on, random_id};
        use mongodb::bson::{doc, oid::ObjectId};
        block_on(async {
            let universe_id = ObjectId::new();
            let user = random_id();
            let (a, b, c) = (random_id(), random_id(), random_id());
            for (id, name) in [(a, "A"), (b, "B"), (c, "C")] {
                Place { _id: ObjectId::new(), universe_id, server_id: 1, category_id: id, role: 1, name: name.into(), modifiers: vec![] }.insert().await.unwrap();
            }
            let (public, secret) = (random_id(), random_id());
            crate::travel::service::tests_support_road(universe_id, public, a, b, false).insert().await.unwrap();
            crate::travel::service::tests_support_road(universe_id, secret, a, c, true).insert().await.unwrap();
            TravelGroup { _id: ObjectId::new(), universe_id, actual_space_id: a, members: vec![user], server_id: 1, ..Default::default() }.insert().await.unwrap();

            let (app, state) = app_with(Arc::default());
            let token = state.sessions.create(crate::activity::session::Session { universe_id, ..test_session(user) });
            let (st, body, _) = call(app.clone(), authed("GET", "/api/map", &token, None)).await;
            let cached = crate::database::universe_maps::get_universe_map(universe_id).await.unwrap();

            let db = crate::database::db_client::get_db_client().await.database(VERSEENGINE_DB_NAME);
            for name in [PLACES_COLLECTION_NAME, ROADS_COLLECTION_NAME, TRAVELS_COLLECTION_NAME, UNIVERSE_MAPS_COLLECTION_NAME] {
                db.collection::<mongodb::bson::Document>(name).delete_many(doc! {"universe_id": universe_id}).await.unwrap();
            }

            assert_eq!(st, StatusCode::OK, "{body}");
            let text = body.to_string();
            assert!(!text.contains(&secret.to_string()), "secret road leaked: {text}");
            assert!(!text.contains(&c.to_string()), "hidden place leaked: {text}");
            assert_eq!(body["roads"].as_array().unwrap().len(), 1);
            assert_eq!(body["places"].as_array().unwrap().len(), 2);
            assert_eq!(body["source"], "Computed");
            assert_eq!(cached.unwrap().positions.len(), 3, "the layout uses every place server-side");
        });
    }
}
