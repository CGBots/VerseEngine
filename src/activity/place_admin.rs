//! Fiche d'un lieu : clic sur un lieu de la carte.
//!
//! Mode admin (`/api/admin/places/...`) :
//! - `GET  /admin/places/{id}` : informations du lieu (salons, bourg Azgaar, routes, joueurs
//!   présents), texte de la fiche, posts du forum wiki ;
//! - `PUT  /admin/places/{id}/description` : enregistre le texte (brouillon) ;
//! - `POST /admin/places/{id}/publish` : publie le texte enregistré dans le forum wiki de chaque
//!   serveur de l'univers (`place::wiki`), compte rendu par serveur ;
//! - `GET|PUT /admin/places/{id}/loot?target=category|central` : table de butin de la catégorie
//!   du lieu ou de son salon central (celles que la fouille tire, `loot::service`).
//!
//! Joueurs : `GET /places/{id}/lore` donne le **dernier texte publié** d'un lieu découvert (jamais
//! le brouillon, rien sur la table de butin).
//!
//! Droits (D3) : lire la fiche demande le mode admin (modérateur ou administrateur) ; modifier ou
//! publier, c'est éditer l'univers : administrateur seulement. Ils sont **recalculés à chaque
//! requête** d'après Discord (lu par le bot), et tout est limité à l'univers de la session (le
//! lieu est cherché par `(univers, catégorie)`).

use std::collections::HashSet;

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use futures::TryStreamExt;
use mongodb::bson::oid::ObjectId;
use serde::{Deserialize, Serialize};

use crate::activity::error::{ApiError, ApiResult};
use crate::activity::http::{bot_http, json_body, now_secs, parse_snowflake, ActionGuard, AppState, Authed, MapQuery};
use crate::activity::permissions::AccessLevel;
use crate::activity::session::Session;
use crate::database::loot_tables::get_loot_table_by_channel_id;
use crate::database::places::{get_place_by_category_id, set_place_description, set_place_publication, Place, PublishedText};
use crate::database::universe_maps::BurgStyle;
use crate::loot_table::service::{self as loot, EntryInput, Invalid, Rule, TableInput};
use crate::place::wiki::{self, PostError, ServerReport, ServerTarget, MAX_DESCRIPTION_CHARS};

/// Joueurs présents listés dans la fiche.
const MAX_PLAYERS_LISTED: usize = 100;

// ---------------------------------------------------------------------------
// Droits
// ---------------------------------------------------------------------------

/// Ce que permet un niveau d'accès : `(lire la fiche, la modifier)`.
pub fn sheet_rights(access: AccessLevel) -> (bool, bool) {
    (access.can_use_admin_mode(), access.can_edit_world())
}

/// Vérifie les droits d'après Discord et la base, jamais d'après la session seule. La session
/// sert de premier filtre (un joueur est refusé sans requête). Retourne l'univers et le niveau.
async fn require(state: &AppState, s: &Session, write: bool) -> ApiResult<(ObjectId, AccessLevel)> {
    let refuse = |access: AccessLevel| {
        let (read, edit) = sheet_rights(access);
        if !read {
            Some(ApiError::forbidden("activity__place_admin_only"))
        } else if write && !edit {
            Some(ApiError::forbidden("activity__place_edit_forbidden"))
        } else {
            None
        }
    };
    if let Some(e) = refuse(s.access) {
        return Err(e);
    }
    let universe_id = s.universe()?;
    let server = crate::database::server::get_server_by_id(s.guild_id).await?;
    // Le serveur a pu changer d'univers depuis l'ouverture de la session.
    if server.as_ref().map(|sv| sv.universe_id) != Some(universe_id) {
        return Err(ApiError::forbidden("activity__place_admin_only"));
    }
    let access = crate::activity::admin_import::access_on(state.discord.as_ref(), s.guild_id, s.user_id, server.as_ref())
        .await?
        .ok_or_else(|| ApiError::forbidden("activity__not_a_member"))?;
    match refuse(access) {
        Some(e) => Err(e),
        None => Ok((universe_id, access)),
    }
}

async fn place_of(universe_id: ObjectId, id: &str) -> ApiResult<Place> {
    let category_id = parse_snowflake(id)?;
    get_place_by_category_id(universe_id, category_id).await?.ok_or_else(|| ApiError::not_found("activity__place_not_found"))
}

// ---------------------------------------------------------------------------
// Fiche
// ---------------------------------------------------------------------------

#[derive(Serialize)]
pub struct BurgDto {
    pub name: Option<String>,
    pub size: u8,
    pub capital: bool,
    pub port: bool,
    pub population: Option<u64>,
    pub state: Option<String>,
    pub culture: Option<String>,
    pub kind: Option<String>,
    pub group: Option<String>,
    pub features: Vec<String>,
}

impl From<&BurgStyle> for BurgDto {
    fn from(b: &BurgStyle) -> Self {
        let d = b.details.clone().unwrap_or_default();
        BurgDto { name: d.name, size: b.size, capital: b.capital, port: b.port, population: d.population, state: d.state, culture: d.culture, kind: d.kind, group: d.group, features: d.features }
    }
}

#[derive(Serialize)]
struct PostDto {
    server_id: String,
    thread_id: String,
    url: String,
}

#[derive(Serialize)]
struct RoadDto {
    id: String,
    name: String,
    to_id: String,
    to_name: Option<String>,
    distance_km: f64,
    secret: bool,
}

#[derive(Serialize)]
struct PlayerDto {
    user_id: String,
    name: Option<String>,
}

#[derive(Serialize)]
struct LootSummary {
    category: bool,
    central: bool,
}

#[derive(Serialize)]
pub struct SheetDto {
    id: String,
    name: String,
    server_id: String,
    role_id: String,
    central_channel_id: Option<String>,
    description: Option<String>,
    published: Option<PublishedText>,
    posts: Vec<PostDto>,
    burg: Option<BurgDto>,
    roads: Vec<RoadDto>,
    players: Vec<PlayerDto>,
    players_total: usize,
    loot: LootSummary,
    can_edit: bool,
    max_description: usize,
    server_time: u64,
}

/// Bourg Azgaar dont vient le lieu (import avec rendu façon Azgaar), s'il y en a un.
async fn burg_of(universe_id: ObjectId, category_id: u64) -> ApiResult<Option<BurgStyle>> {
    let Some(map) = crate::database::universe_maps::get_universe_map(universe_id).await? else { return Ok(None) };
    let burg_id = map.positions.iter().find(|p| p.place_id == category_id).and_then(|p| p.azgaar_burg_id);
    Ok(burg_id.and_then(|b| map.style.as_ref()?.burgs.iter().find(|s| s.burg_id == b).cloned()))
}

async fn sheet(universe_id: ObjectId, place: Place, access: AccessLevel) -> ApiResult<SheetDto> {
    use crate::database::road::{get_roads_of_place, Road};
    let category_id = place.category_id;
    let roads: Vec<Road> = get_roads_of_place(universe_id, category_id).await?.try_collect().await?;
    let mut road_dtos = Vec::new();
    for r in roads {
        let other = if r.place_one_id == category_id { r.place_two_id } else { r.place_one_id };
        let to_name = get_place_by_category_id(universe_id, other).await?.map(|p| p.name);
        road_dtos.push(RoadDto { id: r.channel_id.to_string(), name: r.road_name, to_id: other.to_string(), to_name, distance_km: r.distance as f64, secret: r.secret });
    }
    road_dtos.sort_by(|a, b| a.name.cmp(&b.name));

    let mut members: Vec<u64> = crate::database::travel::get_groups_at_place(universe_id, category_id).await?.into_iter().flat_map(|g| g.members).collect();
    members.sort_unstable();
    members.dedup();
    let players_total = members.len();
    let mut players = Vec::new();
    for user_id in members.into_iter().take(MAX_PLAYERS_LISTED) {
        let name = crate::database::characters::get_character_by_user_id(universe_id, user_id).await?.map(|c| c.name);
        players.push(PlayerDto { user_id: user_id.to_string(), name });
    }

    let loot = LootSummary {
        category: get_loot_table_by_channel_id(universe_id, category_id).await?.is_some(),
        central: match place.central_channel_id {
            Some(c) => get_loot_table_by_channel_id(universe_id, c).await?.is_some(),
            None => false,
        },
    };
    let burg = burg_of(universe_id, category_id).await?;
    Ok(SheetDto {
        id: category_id.to_string(),
        name: place.name,
        server_id: place.server_id.to_string(),
        role_id: place.role.to_string(),
        central_channel_id: place.central_channel_id.map(|c| c.to_string()),
        description: place.wiki.description,
        published: place.wiki.published,
        posts: place
            .wiki
            .posts
            .iter()
            .map(|p| PostDto { server_id: p.server_id.to_string(), thread_id: p.thread_id.to_string(), url: format!("https://discord.com/channels/{}/{}", p.server_id, p.thread_id) })
            .collect(),
        burg: burg.as_ref().map(BurgDto::from),
        roads: road_dtos,
        players,
        players_total,
        loot,
        can_edit: sheet_rights(access).1,
        max_description: MAX_DESCRIPTION_CHARS,
        server_time: now_secs(),
    })
}

async fn get_sheet(State(state): State<AppState>, Authed(s): Authed, Path(id): Path<String>) -> ApiResult<Json<SheetDto>> {
    let (universe_id, access) = require(&state, &s, false).await?;
    let place = place_of(universe_id, &id).await?;
    Ok(Json(sheet(universe_id, place, access).await?))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DescriptionRequest {
    pub description: String,
}

/// Texte de la fiche nettoyé : fins de ligne normalisées, espaces de fin retirés, aucun caractère
/// de contrôle hors retour à la ligne et tabulation. `None` : vide (la fiche est effacée).
pub fn clean_description(raw: &str) -> Result<Option<String>, &'static str> {
    let text = raw.replace("\r\n", "\n").replace('\r', "\n");
    let text = text.trim();
    if text.chars().any(|c| c.is_control() && c != '\n' && c != '\t') {
        return Err("activity__place_description_invalid");
    }
    if text.chars().count() > MAX_DESCRIPTION_CHARS {
        return Err("activity__place_description_too_long");
    }
    Ok((!text.is_empty()).then(|| text.to_string()))
}

async fn put_description(
    State(state): State<AppState>,
    Authed(s): Authed,
    Path(id): Path<String>,
    body: Result<Json<DescriptionRequest>, axum::extract::rejection::JsonRejection>,
) -> ApiResult<Json<SheetDto>> {
    let req = json_body(body)?;
    let description = clean_description(&req.description).map_err(ApiError::bad_request)?;
    let category_id = parse_snowflake(&id)?;
    let _guard = ActionGuard::acquire(&state, s.user_id)?;
    let (universe_id, access) = require(&state, &s, true).await?;
    if !set_place_description(universe_id, category_id, description.as_deref()).await? {
        return Err(ApiError::not_found("activity__place_not_found"));
    }
    // Relu en base : le front affiche ce qui est réellement enregistré.
    let place = place_of(universe_id, &id).await?;
    Ok(Json(sheet(universe_id, place, access).await?))
}

#[derive(Serialize)]
struct PublishResponse {
    sheet: SheetDto,
    report: Vec<ServerReport>,
}

fn post_error_key(e: PostError) -> &'static str {
    match e {
        PostError::Empty => "activity__place_nothing_to_publish",
        PostError::TooLong => "activity__place_post_too_long",
    }
}

async fn publish(State(state): State<AppState>, Authed(s): Authed, Path(id): Path<String>) -> ApiResult<Json<PublishResponse>> {
    parse_snowflake(&id)?;
    let _guard = ActionGuard::acquire(&state, s.user_id)?;
    let (universe_id, access) = require(&state, &s, true).await?;
    let http = bot_http(&state)?;
    let place = place_of(universe_id, &id).await?;
    let text = place.wiki.description.clone().unwrap_or_default();
    let burg = burg_of(universe_id, place.category_id).await?;
    let build = |locale: &str| {
        let label = |k: &str| crate::translation::get_by_locale(locale, k, None, None);
        wiki::build_post(&place.name, &text, burg.as_ref(), &label)
    };
    // Vérifié avant tout appel à Discord (les libellés des deux langues ont des tailles proches).
    for locale in ["fr", "en-US"] {
        build(locale).map_err(|e| ApiError::bad_request(post_error_key(e)))?;
    }
    let servers: Vec<ServerTarget> = crate::database::universe::get_servers_from_universe(&universe_id)
        .await?
        .try_collect::<Vec<_>>()
        .await?
        .into_iter()
        .map(|sv| ServerTarget { server_id: sv.server_id, forum_id: sv.rp_wiki_channel_id.map(|c| c.id) })
        .collect();
    let forum = wiki::LiveForum(http);
    let (posts, report) = wiki::publish(&forum, &servers, &place.wiki.posts, &build).await;
    let published = PublishedText { text: text.trim().to_string(), at: now_secs() };
    if !set_place_publication(universe_id, place.category_id, &published, &posts).await? {
        return Err(ApiError::not_found("activity__place_not_found"));
    }
    let fresh = place_of(universe_id, &id).await?;
    Ok(Json(PublishResponse { sheet: sheet(universe_id, fresh, access).await?, report }))
}

// ---------------------------------------------------------------------------
// Table de butin
// ---------------------------------------------------------------------------

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct LootQuery {
    #[serde(default)]
    target: Option<String>,
}

/// Table éditée : celle de la catégorie du lieu, ou celle de son salon central.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LootTarget {
    Category,
    Central,
}

impl LootTarget {
    pub fn parse(q: &LootQuery) -> ApiResult<Self> {
        match q.target.as_deref() {
            None | Some("category") => Ok(Self::Category),
            Some("central") => Ok(Self::Central),
            Some(_) => Err(ApiError::bad_request("activity__invalid_request")),
        }
    }

    fn channel(self, place: &Place) -> ApiResult<u64> {
        match self {
            Self::Category => Ok(place.category_id),
            Self::Central => place.central_channel_id.ok_or_else(|| ApiError::not_found("activity__place_no_central_channel")),
        }
    }
}

#[derive(Serialize)]
struct LootDto {
    target: LootTarget,
    channel_id: String,
    central_available: bool,
    exists: bool,
    entries: Vec<EntryInput>,
    rate_limit: Option<u64>,
    delay: Option<u64>,
    raw_text: String,
    /// Objets de l'univers, triés : la liste dans laquelle l'éditeur choisit.
    items: Vec<String>,
    can_edit: bool,
    server_time: u64,
}

async fn item_names(universe_id: ObjectId) -> ApiResult<Vec<String>> {
    let mut names: Vec<String> = crate::database::items::get_items_by_universe(universe_id).await?.into_iter().map(|i| i.item_name).collect();
    names.sort_by_key(|n| n.to_lowercase());
    names.dedup();
    Ok(names)
}

async fn loot_state(universe_id: ObjectId, place: &Place, target: LootTarget, access: AccessLevel) -> ApiResult<LootDto> {
    let channel_id = target.channel(place)?;
    let table = get_loot_table_by_channel_id(universe_id, channel_id).await?;
    Ok(LootDto {
        target,
        channel_id: channel_id.to_string(),
        central_available: place.central_channel_id.is_some(),
        exists: table.is_some(),
        entries: table.as_ref().map(|t| loot::to_input(&t.entries)).unwrap_or_default(),
        rate_limit: table.as_ref().and_then(|t| t.rate_limit),
        delay: table.as_ref().and_then(|t| t.delay),
        raw_text: table.map(|t| t.raw_text).unwrap_or_default(),
        items: item_names(universe_id).await?,
        can_edit: sheet_rights(access).1,
        server_time: now_secs(),
    })
}

async fn get_loot(State(state): State<AppState>, Authed(s): Authed, Path(id): Path<String>, Query(q): Query<LootQuery>) -> ApiResult<Json<LootDto>> {
    let target = LootTarget::parse(&q)?;
    let (universe_id, access) = require(&state, &s, false).await?;
    let place = place_of(universe_id, &id).await?;
    Ok(Json(loot_state(universe_id, &place, target, access).await?))
}

/// Règle de validation → clé Fluent affichée par le front.
pub fn rule_key(rule: Rule) -> &'static str {
    match rule {
        Rule::TooManyEntries => "activity__loot_too_many_entries",
        Rule::TooManySetItems => "activity__loot_too_many_set_items",
        Rule::EmptySet => "activity__loot_empty_set",
        Rule::EmptyName => "activity__loot_empty_name",
        Rule::NameTooLong => "activity__loot_name_too_long",
        Rule::NameNotWritable => "activity__loot_name_not_writable",
        Rule::UnknownItem => "activity__loot_unknown_item",
        Rule::Probability => "activity__loot_bad_probability",
        Rule::MinMax => "activity__loot_min_above_max",
        Rule::Quantity => "activity__loot_bad_quantity",
        Rule::Stock => "activity__loot_bad_stock",
        Rule::Seconds => "activity__loot_bad_seconds",
    }
}

async fn put_loot(
    State(state): State<AppState>,
    Authed(s): Authed,
    Path(id): Path<String>,
    Query(q): Query<LootQuery>,
    body: Result<Json<TableInput>, axum::extract::rejection::JsonRejection>,
) -> ApiResult<Json<LootDto>> {
    // Forme et bornes du corps d'abord : rien n'est lu en base pour une requête mal formée.
    let input = json_body(body)?;
    let target = LootTarget::parse(&q)?;
    parse_snowflake(&id)?;
    let _guard = ActionGuard::acquire(&state, s.user_id)?;
    let (universe_id, access) = require(&state, &s, true).await?;
    let place = place_of(universe_id, &id).await?;
    let channel_id = target.channel(&place)?;
    let known: HashSet<String> = item_names(universe_id).await?.into_iter().collect();
    let entries = loot::validate(&input, &known).map_err(|Invalid { rule, .. }| ApiError::new(StatusCode::UNPROCESSABLE_ENTITY, rule_key(rule)))?;
    loot::replace_table(universe_id, channel_id, &input, entries).await?;
    Ok(Json(loot_state(universe_id, &place, target, access).await?))
}

// ---------------------------------------------------------------------------
// Joueurs : texte publié d'un lieu découvert
// ---------------------------------------------------------------------------

#[derive(Serialize)]
struct LoreDto {
    id: String,
    name: String,
    /// Dernier texte publié ; `None` si le lieu n'a pas encore de fiche publiée.
    text: Option<String>,
    published_at: Option<u64>,
    /// Ce que le joueur peut savoir du bourg (même données que la fiche admin).
    burg: Option<BurgDto>,
    /// Routes connues : non secrètes, vers un lieu que le joueur voit sur sa carte.
    roads: Vec<LoreRoadDto>,
}

#[derive(Serialize)]
struct LoreRoadDto {
    to_id: String,
    to_name: String,
    distance_km: f64,
}

/// Le joueur connaît-il ce lieu ? Même règle que la carte qu'il reçoit : sous brouillard, un lieu
/// découvert (une destination seulement entrevue ne suffit pas) ; sinon, un lieu affiché.
pub fn lore_visible(map: &crate::activity::map_view::MapDto, id: &str) -> bool {
    match &map.fog {
        Some(fog) => fog.discovered.iter().any(|d| d == id),
        None => map.places.iter().any(|p| p.id == id),
    }
}

async fn get_lore(Authed(s): Authed, Path(id): Path<String>) -> ApiResult<Json<LoreDto>> {
    let category_id = parse_snowflake(&id)?;
    let universe_id = s.universe()?;
    // La carte telle que le joueur la reçoit (vue joueur, brouillard compris, même pour un admin).
    // Spectateur (sans personnage) : la carte dévoilée qu'il voit.
    let query = if crate::activity::spectator::is_spectator(&s).await? { MapQuery::spectator_query() } else { MapQuery::default() };
    let Json(map) = crate::activity::http::map(Authed(s.clone()), Query(query)).await?;
    // Inconnu ou caché : même réponse qu'un lieu inexistant (rien ne fuit).
    if !lore_visible(&map, &category_id.to_string()) {
        return Err(ApiError::not_found("activity__place_not_found"));
    }
    let place = place_of(universe_id, &id).await?;
    let published = place.wiki.published;
    let burg = burg_of(universe_id, category_id).await?;
    let roads: Vec<crate::database::road::Road> = crate::database::road::get_roads_of_place(universe_id, category_id).await?.try_collect().await?;
    let mut road_dtos: Vec<LoreRoadDto> = roads
        .into_iter()
        .filter(|r| !r.secret)
        .filter_map(|r| {
            let other = if r.place_one_id == category_id { r.place_two_id } else { r.place_one_id }.to_string();
            let to = map.places.iter().find(|p| p.id == other)?;
            Some(LoreRoadDto { to_name: to.name.clone(), to_id: other, distance_km: r.distance as f64 })
        })
        .collect();
    road_dtos.sort_by(|a, b| a.to_name.cmp(&b.to_name));
    Ok(Json(LoreDto {
        id: category_id.to_string(),
        name: place.name,
        published_at: published.as_ref().map(|p| p.at),
        text: published.map(|p| p.text),
        burg: burg.as_ref().map(BurgDto::from),
        roads: road_dtos,
    }))
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/admin/places/{id}", get(get_sheet))
        .route("/admin/places/{id}/description", axum::routing::put(put_description))
        .route("/admin/places/{id}/publish", post(publish))
        .route("/admin/places/{id}/loot", get(get_loot).put(put_loot))
        .route("/places/{id}/lore", get(get_lore))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::activity::discord_api::*;
    use crate::activity::session::Session;
    use axum::body::{to_bytes, Body};
    use axum::http::Request;
    use futures::future::BoxFuture;
    use std::sync::Arc;
    use tower::ServiceExt;

    /// Discord jamais appelé : les refus testés ici viennent avant.
    struct NoDiscord;
    impl DiscordGateway for NoDiscord {
        fn exchange_code<'a>(&'a self, _c: &'a str) -> BoxFuture<'a, Result<TokenGrant, DiscordError>> { unreachable!() }
        fn current_user<'a>(&'a self, _t: &'a str) -> BoxFuture<'a, Result<DiscordUser, DiscordError>> { unreachable!() }
        fn activity_instance<'a>(&'a self, _i: &'a str) -> BoxFuture<'a, Result<ActivityInstance, DiscordError>> { unreachable!() }
        fn member<'a>(&'a self, _g: u64, _u: u64) -> BoxFuture<'a, Result<Option<MemberInfo>, DiscordError>> { unreachable!() }
        fn bot_guilds<'a>(&'a self, _l: usize) -> BoxFuture<'a, Result<Vec<GuildSummary>, DiscordError>> { unreachable!() }
        fn guild_counts<'a>(&'a self, _g: u64) -> BoxFuture<'a, Result<GuildCounts, DiscordError>> { unreachable!() }
    }

    fn app(access: AccessLevel) -> (Router, String) {
        let state = AppState::new(Arc::new(NoDiscord), std::time::Duration::from_secs(60));
        let token = state.sessions.create(Session {
            user_id: 42,
            username: "u".into(),
            guild_id: 7,
            universe_id: Some(ObjectId::new()),
            instance_id: "i-1".into(),
            locale: "fr",
            access,
            expires_at: std::time::Instant::now(),
        });
        (crate::activity::http::router(state), token)
    }

    async fn call(app: &Router, method: &str, uri: &str, token: Option<&str>, body: Option<&str>) -> (StatusCode, serde_json::Value) {
        let mut req = Request::builder().method(method).uri(uri);
        if let Some(t) = token {
            req = req.header("authorization", format!("Bearer {t}"));
        }
        if body.is_some() {
            req = req.header("content-type", "application/json");
        }
        let res = app.clone().oneshot(req.body(body.map(|b| Body::from(b.to_owned())).unwrap_or_else(Body::empty)).unwrap()).await.unwrap();
        let status = res.status();
        let bytes = to_bytes(res.into_body(), 1 << 20).await.unwrap();
        (status, serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null))
    }

    #[test]
    fn moderators_read_the_sheet_and_only_admins_edit_it() {
        assert_eq!(sheet_rights(AccessLevel::Player), (false, false));
        assert_eq!(sheet_rights(AccessLevel::Moderator), (true, false));
        assert_eq!(sheet_rights(AccessLevel::Admin), (true, true));
    }

    #[tokio::test]
    async fn place_routes_need_a_session() {
        let (app, _) = app(AccessLevel::Admin);
        for (m, uri) in [("GET", "/api/admin/places/1"), ("POST", "/api/admin/places/1/publish"), ("GET", "/api/admin/places/1/loot"), ("GET", "/api/places/1/lore")] {
            assert_eq!(call(&app, m, uri, None, None).await.0, StatusCode::UNAUTHORIZED, "{uri}");
        }
    }

    #[tokio::test]
    async fn a_player_cannot_open_nor_edit_the_admin_sheet() {
        let (app, token) = app(AccessLevel::Player);
        for (m, uri, body) in [
            ("GET", "/api/admin/places/1", None),
            ("GET", "/api/admin/places/1/loot", None),
            ("PUT", "/api/admin/places/1/description", Some(r#"{"description":"x"}"#)),
            ("POST", "/api/admin/places/1/publish", None),
            ("PUT", "/api/admin/places/1/loot", Some(r#"{"entries":[]}"#)),
        ] {
            let (status, body) = call(&app, m, uri, Some(&token), body).await;
            assert_eq!((status, body["error"].as_str()), (StatusCode::FORBIDDEN, Some("activity__place_admin_only")), "{m} {uri}");
        }
    }

    #[tokio::test]
    async fn a_moderator_cannot_write() {
        let (app, token) = app(AccessLevel::Moderator);
        for (m, uri, body) in [
            ("PUT", "/api/admin/places/1/description", Some(r#"{"description":"x"}"#)),
            ("POST", "/api/admin/places/1/publish", None),
            ("PUT", "/api/admin/places/1/loot?target=central", Some(r#"{"entries":[]}"#)),
        ] {
            let (status, body) = call(&app, m, uri, Some(&token), body).await;
            assert_eq!((status, body["error"].as_str()), (StatusCode::FORBIDDEN, Some("activity__place_edit_forbidden")), "{m} {uri}");
        }
    }

    #[tokio::test]
    async fn bodies_and_ids_are_validated_before_anything_else() {
        let (app, token) = app(AccessLevel::Admin);
        let cases = [
            ("PUT", "/api/admin/places/1/description", Some(r#"{"description":"x","extra":1}"#)),
            ("PUT", "/api/admin/places/abc/description", Some(r#"{"description":"x"}"#)),
            ("PUT", "/api/admin/places/1/loot", Some(r#"{"entries":[{"kind":"item","name":"or"}]}"#)),
            ("PUT", "/api/admin/places/1/loot?target=elsewhere", Some(r#"{"entries":[]}"#)),
            ("POST", "/api/admin/places/0/publish", None),
        ];
        for (m, uri, body) in cases {
            let (status, b) = call(&app, m, uri, Some(&token), body).await;
            assert_eq!((status, b["error"].as_str()), (StatusCode::BAD_REQUEST, Some("activity__invalid_request")), "{m} {uri}");
        }
        let long = format!(r#"{{"description":"{}"}}"#, "a".repeat(MAX_DESCRIPTION_CHARS + 1));
        let (status, b) = call(&app, "PUT", "/api/admin/places/1/description", Some(&token), Some(&long)).await;
        assert_eq!((status, b["error"].as_str()), (StatusCode::BAD_REQUEST, Some("activity__place_description_too_long")));
    }

    #[test]
    fn descriptions_are_cleaned_and_bounded() {
        assert_eq!(clean_description("  Une ville.\r\nDeux lignes.\t \n"), Ok(Some("Une ville.\nDeux lignes.".into())));
        assert_eq!(clean_description("   "), Ok(None), "empty clears the sheet");
        assert_eq!(clean_description("a\u{0007}b"), Err("activity__place_description_invalid"));
        assert!(clean_description(&"é".repeat(MAX_DESCRIPTION_CHARS)).is_ok());
        assert_eq!(clean_description(&"é".repeat(MAX_DESCRIPTION_CHARS + 1)), Err("activity__place_description_too_long"));
    }

    #[test]
    fn every_validation_rule_has_a_translated_key() {
        use Rule::*;
        let ftl = std::fs::read_to_string("translations/fr.ftl").unwrap();
        for r in [TooManyEntries, TooManySetItems, EmptySet, EmptyName, NameTooLong, NameNotWritable, UnknownItem, Probability, MinMax, Quantity, Stock, Seconds] {
            assert!(ftl.lines().any(|l| l.starts_with(&format!("{} =", rule_key(r)))), "{r:?}");
        }
    }

    #[test]
    fn players_only_read_the_lore_of_discovered_places() {
        use crate::activity::fog::FogDto;
        use crate::activity::map_view::{MapDto, MapPlaceDto};
        let place = |id: &str| MapPlaceDto { id: id.into(), name: id.into(), x: 0.0, y: 0.0, z: 0.0, burg: None };
        let mut map = MapDto {
            source: crate::database::universe_maps::MapSource::Computed,
            width: 1.0,
            height: 1.0,
            places: vec![place("1"), place("2")],
            roads: vec![],
            terrain: None,
            zone: None,
            background: None,
            style: None,
            server_time: 0,
            fog: None,
        };
        assert!(lore_visible(&map, "1") && !lore_visible(&map, "3"), "without fog: the places on the map");
        map.fog = Some(FogDto { radius: 1.0, discovered: vec!["1".into()], glimpsed: vec!["2".into()], roads: vec![] });
        assert!(lore_visible(&map, "1"));
        assert!(!lore_visible(&map, "2"), "a glimpsed destination is not discovered");
    }
}
