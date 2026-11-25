//! Panneau du mode admin (menu de gauche) : téléportation des joueurs, éditeur d'objets et
//! éditeur de recettes.
//!
//! - `GET  /api/admin/players` : joueurs de l'univers et leur position, lieux de destination ;
//! - `POST /api/admin/teleport` : téléporte un ou plusieurs joueurs (même service que `/teleport`) ;
//! - `GET  /api/admin/items`, `POST /api/admin/items`, `PUT|DELETE /api/admin/items/{id}` :
//!   mêmes services que `/item create` et `/item delete` (`item::service`) ;
//! - `GET  /api/admin/recipes`, `POST /api/admin/recipes`, `PUT|DELETE /api/admin/recipes/{id}` :
//!   même service que `/recipe create` (`recipe::service`).
//!
//! Sécurité : le mode admin n'est qu'un état du front. Chaque requête **recalcule les droits** à
//! partir de Discord (lu par le bot) et de la base, jamais de la session seule, et reste limitée
//! à l'univers de la session. Décision D3 : modérateurs et administrateurs consultent et
//! téléportent (les modérateurs gèrent les positions des joueurs) ; seuls les administrateurs
//! modifient l'univers (objets, recettes). Écritures : 1 action/s et une à la fois par admin.

use std::collections::{HashMap, HashSet};

use axum::extract::{Path, State};
use axum::routing::{get, post, put};
use axum::{Json, Router};
use futures::TryStreamExt;
use mongodb::bson::{doc, oid::ObjectId};
use serde::{Deserialize, Serialize};

use crate::activity::admin_import::access_on;
use crate::activity::craft_view::{usage_name, EffectDto};
use crate::activity::error::{ApiError, ApiResult};
use crate::activity::events::{Event, EventKind};
use crate::activity::http::{bot_http, json_body, now_secs, parse_object_id, parse_snowflake, service_error, ActionGuard, AppState, Authed};
use crate::activity::permissions::AccessLevel;
use crate::activity::session::Session;
use crate::database::characters::Character;
use crate::database::items::{get_items_by_universe, Item};
use crate::database::places::Place;
use crate::database::recipe::Recipe;
use crate::database::road::Road;
use crate::database::stats::Stat;
use crate::database::travel::{SpaceType, TravelGroup};
use crate::item::service::{effects_text, valid_item_name, validate_fields, ItemFields, WikiAction};
use crate::item::ItemUsage;
use crate::recipe::service::{recipe_text, valid_recipe_name, SaveError, MAX_LINE_QUANTITY, MAX_RECIPE_DELAY, MAX_RECIPE_LINES};

/// Joueurs téléportés au plus par requête.
pub const MAX_TELEPORT: usize = 50;

// ---------------------------------------------------------------------------
// Droits
// ---------------------------------------------------------------------------

/// Ce que la requête demande.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Need {
    /// Lire les listes (joueurs, objets, recettes).
    View,
    /// Téléporter des joueurs.
    Teleport,
    /// Créer, modifier ou supprimer des objets et des recettes.
    Edit,
}

/// D3 : modérateurs et administrateurs consultent ; seuls les administrateurs téléportent
/// (comme `/teleport`) et modifient l'univers.
pub fn allowed(need: Need, access: AccessLevel) -> bool {
    match need {
        Need::View => access.can_use_admin_mode(),
        // Comme `/teleport`, qui exige la permission Administrateur.
        Need::Teleport | Need::Edit => access.can_edit_world(),
    }
}

/// Vérifie les droits d'après Discord et la base ; renvoie l'univers de la session et le niveau
/// d'accès recalculé. Le serveur doit toujours être relié à l'univers de la session.
async fn require(state: &AppState, s: &Session, need: Need) -> ApiResult<(ObjectId, AccessLevel)> {
    let universe_id = s.universe()?;
    let server = crate::database::server::get_server_by_id(s.guild_id).await?;
    if server.as_ref().map(|sv| sv.universe_id) != Some(universe_id) {
        return Err(ApiError::forbidden("activity__admin_forbidden"));
    }
    let access = access_on(state.discord.as_ref(), s.guild_id, s.user_id, server.as_ref())
        .await?
        .ok_or_else(|| ApiError::forbidden("activity__not_a_member"))?;
    if !allowed(need, access) {
        return Err(ApiError::forbidden("activity__admin_forbidden"));
    }
    Ok((universe_id, access))
}

/// Objets et recettes ont changé : les joueurs relisent inventaire et livre de recettes.
fn publish_items_changed(state: &AppState, universe_id: ObjectId) {
    state.hub.publish(Event { kind: EventKind::InventoryUpdated, universe_id: Some(universe_id), users: vec![] });
}

// ---------------------------------------------------------------------------
// Joueurs et téléportation
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, PartialEq)]
pub struct PlayerLocation {
    /// `place` ou `road`.
    pub kind: &'static str,
    pub id: String,
    pub name: Option<String>,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct PlayerRow {
    pub user_id: String,
    pub name: String,
    pub location: Option<PlayerLocation>,
    /// Le groupe du joueur est en route.
    pub moving: bool,
    pub group_size: usize,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct PlaceChoice {
    pub id: String,
    pub name: String,
}

/// Lignes de la liste des joueurs, par nom. Un personnage sans groupe n'a pas de position.
pub fn player_rows(characters: &[Character], groups: &[TravelGroup], places: &[Place], roads: &[Road]) -> Vec<PlayerRow> {
    let place_names: HashMap<u64, &str> = places.iter().map(|p| (p.category_id, p.name.as_str())).collect();
    let road_names: HashMap<u64, &str> = roads.iter().map(|r| (r.channel_id, r.road_name.as_str())).collect();
    let group_of: HashMap<u64, &TravelGroup> = groups.iter().flat_map(|g| g.members.iter().map(move |m| (*m, g))).collect();
    let mut rows: Vec<PlayerRow> = characters
        .iter()
        .map(|c| {
            let g = group_of.get(&c.user_id);
            PlayerRow {
                user_id: c.user_id.to_string(),
                name: c.name.clone(),
                location: g.map(|g| match g.actual_space_type {
                    SpaceType::Place => PlayerLocation { kind: "place", id: g.actual_space_id.to_string(), name: place_names.get(&g.actual_space_id).map(|s| s.to_string()) },
                    SpaceType::Road => {
                        let road = g.road_id.unwrap_or(g.actual_space_id);
                        PlayerLocation { kind: "road", id: road.to_string(), name: road_names.get(&road).map(|s| s.to_string()) }
                    }
                }),
                moving: g.is_some_and(|g| g.is_in_move),
                group_size: g.map_or(1, |g| g.members.len()),
            }
        })
        .collect();
    rows.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()).then_with(|| a.user_id.cmp(&b.user_id)));
    rows
}

pub(crate) async fn characters_of(universe_id: ObjectId) -> mongodb::error::Result<Vec<Character>> {
    crate::database::db_client::get_db_client()
        .await
        .database(crate::database::db_namespace::VERSEENGINE_DB_NAME)
        .collection::<Character>(crate::database::db_namespace::CHARACTERS_COLLECTION_NAME)
        .find(doc! {"universe_id": universe_id})
        .await?
        .try_collect()
        .await
}

pub(crate) async fn groups_of(universe_id: ObjectId) -> mongodb::error::Result<Vec<TravelGroup>> {
    crate::database::db_client::get_db_client()
        .await
        .database(crate::database::db_namespace::VERSEENGINE_DB_NAME)
        .collection::<TravelGroup>(crate::database::db_namespace::TRAVELS_COLLECTION_NAME)
        .find(doc! {"universe_id": universe_id})
        .await?
        .try_collect()
        .await
}

async fn list_players(State(state): State<AppState>, Authed(s): Authed) -> ApiResult<Json<serde_json::Value>> {
    let (universe_id, _) = require(&state, &s, Need::View).await?;
    let places = crate::teleport::service::all_places(universe_id).await?;
    let roads: Vec<Road> = crate::database::road::get_roads_by_universe(universe_id).await?.try_collect().await?;
    let players = player_rows(&characters_of(universe_id).await?, &groups_of(universe_id).await?, &places, &roads);
    let mut choices: Vec<PlaceChoice> = places.iter().map(|p| PlaceChoice { id: p.category_id.to_string(), name: p.name.clone() }).collect();
    choices.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    Ok(Json(serde_json::json!({ "players": players, "places": choices, "server_time": now_secs() })))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TeleportRequest {
    user_ids: Vec<String>,
    destination_id: String,
}

/// Joueurs à téléporter : 1 à [`MAX_TELEPORT`] identifiants valides, sans doublon (ordre gardé).
pub fn teleport_targets(user_ids: &[String]) -> ApiResult<Vec<u64>> {
    if user_ids.is_empty() || user_ids.len() > MAX_TELEPORT {
        return Err(ApiError::bad_request("activity__invalid_request"));
    }
    let mut seen = HashSet::new();
    let mut out = Vec::with_capacity(user_ids.len());
    for id in user_ids {
        let id = parse_snowflake(id)?;
        if seen.insert(id) {
            out.push(id);
        }
    }
    Ok(out)
}

#[derive(Serialize)]
struct TeleportResult {
    user_id: String,
    ok: bool,
    /// Clé Fluent : `teleport__player_success` ou l'erreur.
    key: String,
}

async fn teleport(
    State(state): State<AppState>,
    Authed(s): Authed,
    body: Result<Json<TeleportRequest>, axum::extract::rejection::JsonRejection>,
) -> ApiResult<Json<serde_json::Value>> {
    let req = json_body(body)?;
    let targets = teleport_targets(&req.user_ids)?;
    let destination = parse_snowflake(&req.destination_id)?;
    let (universe_id, _) = require(&state, &s, Need::Teleport).await?;
    let _guard = ActionGuard::acquire(&state, s.user_id)?;
    let http = bot_http(&state)?;
    let places = crate::teleport::service::all_places(universe_id).await?;
    let dest = places.iter().find(|p| p.category_id == destination).ok_or_else(|| ApiError::not_found("teleport__place_not_found"))?;
    // Seuls les joueurs de cet univers (un personnage) : l'admin ne choisit que dans la liste.
    let players: HashSet<u64> = characters_of(universe_id).await?.into_iter().map(|c| c.user_id).collect();
    let (known, unknown): (Vec<u64>, Vec<u64>) = targets.iter().partition(|id| players.contains(id));
    let outcomes = crate::teleport::service::teleport_players(&http, universe_id, &known, dest).await;
    let mut results: Vec<TeleportResult> = outcomes
        .into_iter()
        .map(|o| match o.result {
            Ok(_) => TeleportResult { user_id: o.user_id.to_string(), ok: true, key: "teleport__player_success".into() },
            Err(key) => {
                let key = if crate::activity::http::is_fluent_key(&key) { key } else { "activity__internal_error".into() };
                TeleportResult { user_id: o.user_id.to_string(), ok: false, key }
            }
        })
        .collect();
    results.extend(unknown.into_iter().map(|id| TeleportResult { user_id: id.to_string(), ok: false, key: "teleport__not_a_player".into() }));
    Ok(Json(serde_json::json!({ "results": results, "place": dest.name, "server_time": now_secs() })))
}

// ---------------------------------------------------------------------------
// Objets
// ---------------------------------------------------------------------------

#[derive(Serialize)]
struct AdminItemDto {
    id: String,
    name: String,
    usage: &'static str,
    description: Option<String>,
    secret: Option<String>,
    image: Option<String>,
    inventory_size: u64,
    /// Effets dans la syntaxe de `/item create` (champ éditable).
    effects_text: String,
    effects: Vec<EffectDto>,
}

fn item_dto(item: &Item, stats: &[Stat]) -> AdminItemDto {
    // Mêmes effets affichés que les cartes d'inventaire du joueur.
    let card = crate::activity::craft_view::inventory_cards(
        &[crate::database::inventory::Inventory {
            _id: None,
            universe_id: item.universe_id,
            holder: crate::database::inventory::Holder { holder_type: crate::database::inventory::HolderType::Character, holder_id: item._id },
            item_id: item._id,
            quantity: 1,
        }],
        std::slice::from_ref(item),
        stats,
    );
    AdminItemDto {
        id: item._id.to_hex(),
        name: item.item_name.clone(),
        usage: usage_name(&item.item_usage),
        description: item.description.clone(),
        secret: item.secret_informations.clone(),
        image: item.image.clone(),
        inventory_size: item.inventory_size,
        effects_text: effects_text(&item.effects, stats),
        effects: card.into_iter().next().map(|c| c.effects).unwrap_or_default(),
    }
}

async fn universe_stats(universe_id: ObjectId) -> ApiResult<Vec<Stat>> {
    let universe = crate::database::universe::get_universe_by_id(universe_id).await?.ok_or_else(|| ApiError::not_found("activity__universe_not_found"))?;
    Ok(universe.get_stats().await?.try_collect().await?)
}

async fn list_items(State(state): State<AppState>, Authed(s): Authed) -> ApiResult<Json<serde_json::Value>> {
    let (universe_id, access) = require(&state, &s, Need::View).await?;
    let stats = universe_stats(universe_id).await?;
    let mut items = get_items_by_universe(universe_id).await?;
    items.sort_by_key(|i| i.item_name.to_lowercase());
    let items: Vec<AdminItemDto> = items.iter().map(|i| item_dto(i, &stats)).collect();
    let stat_names: Vec<&str> = stats.iter().map(|s| s.name.as_str()).collect();
    Ok(Json(serde_json::json!({ "items": items, "stats": stat_names, "can_edit": allowed(Need::Edit, access), "server_time": now_secs() })))
}

pub fn parse_usage(s: &str) -> ApiResult<ItemUsage> {
    Ok(match s {
        "consumable" => ItemUsage::Consumable,
        "usable" => ItemUsage::Usable,
        "placeable" => ItemUsage::Placeable,
        "wearable" => ItemUsage::Wearable,
        "none" => ItemUsage::None,
        _ => return Err(ApiError::bad_request("activity__invalid_request")),
    })
}

/// Texte facultatif : vide → `None`.
fn optional(s: Option<String>) -> Option<String> {
    s.map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ItemBody {
    /// Ignoré en modification : le nom d'un objet ne change pas.
    #[serde(default)]
    name: Option<String>,
    usage: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    secret: Option<String>,
    #[serde(default)]
    image: Option<String>,
    #[serde(default)]
    inventory_size: u64,
    #[serde(default)]
    effects: String,
    /// `keep`, `publish` ou `remove` (création : `publish` ou non).
    #[serde(default = "default_wiki")]
    wiki: String,
}

fn default_wiki() -> String { "keep".into() }

/// Champs d'un objet validés (bornes, usage, URL d'image).
pub fn item_fields(body: &ItemBody) -> ApiResult<(ItemFields, WikiAction)> {
    let fields = ItemFields {
        usage: parse_usage(&body.usage)?,
        description: optional(body.description.clone()),
        secret: optional(body.secret.clone()),
        image: optional(body.image.clone()),
        inventory_size: body.inventory_size,
        effects_raw: body.effects.trim().to_string(),
    };
    validate_fields(&fields).map_err(ApiError::bad_request)?;
    let wiki = match body.wiki.as_str() {
        "keep" => WikiAction::Keep,
        "publish" => WikiAction::Publish,
        "remove" => WikiAction::Remove,
        _ => return Err(ApiError::bad_request("activity__invalid_request")),
    };
    Ok((fields, wiki))
}

/// Effets relus avant tout enregistrement : une faute de frappe donne une erreur claire (le
/// message du parseur, en français libre, n'est pas une clé traduisible).
async fn check_effects(universe_id: ObjectId, raw: &str) -> ApiResult<()> {
    if raw.is_empty() {
        return Ok(());
    }
    crate::utility::effect_parser::EffectParser::parse(raw, universe_id, universe_id)
        .await
        .map(|_| ())
        .map_err(|_| ApiError::bad_request("activity__invalid_effects"))
}

async fn create_item(
    State(state): State<AppState>,
    Authed(s): Authed,
    body: Result<Json<ItemBody>, axum::extract::rejection::JsonRejection>,
) -> ApiResult<Json<serde_json::Value>> {
    let body = json_body(body)?;
    let (fields, wiki) = item_fields(&body)?;
    let name = body.name.as_deref().map(str::trim).unwrap_or_default().to_string();
    if !valid_item_name(&name) {
        return Err(ApiError::bad_request("create_item__invalid_name"));
    }
    let (universe_id, _) = require(&state, &s, Need::Edit).await?;
    let _guard = ActionGuard::acquire(&state, s.user_id)?;
    let http = bot_http(&state)?;
    check_effects(universe_id, &fields.effects_raw).await?;
    let (item, _) = crate::item::service::create_item(&http, universe_id, &name, fields, wiki == WikiAction::Publish, s.locale)
        .await
        .map_err(|e| item_error(&e))?;
    publish_items_changed(&state, universe_id);
    Ok(Json(serde_json::json!({ "id": item._id.to_hex(), "result": "activity_ui__admin_item_created" })))
}

/// `create_item__already_exists` est un conflit, `…_invalid_name` une requête invalide.
fn item_error(e: &crate::discord::poise_structs::Error) -> ApiError {
    match e.to_string().as_str() {
        "create_item__invalid_name" => ApiError::bad_request("create_item__invalid_name"),
        _ => service_error(e),
    }
}

async fn update_item(
    State(state): State<AppState>,
    Authed(s): Authed,
    Path(id): Path<String>,
    body: Result<Json<ItemBody>, axum::extract::rejection::JsonRejection>,
) -> ApiResult<Json<serde_json::Value>> {
    let item_id = parse_object_id(&id)?;
    let body = json_body(body)?;
    let (fields, wiki) = item_fields(&body)?;
    let (universe_id, _) = require(&state, &s, Need::Edit).await?;
    let _guard = ActionGuard::acquire(&state, s.user_id)?;
    let http = bot_http(&state)?;
    check_effects(universe_id, &fields.effects_raw).await?;
    crate::item::service::update_item(&http, universe_id, item_id, fields, wiki, s.locale).await.map_err(|e| item_error(&e))?;
    publish_items_changed(&state, universe_id);
    Ok(Json(serde_json::json!({ "id": item_id.to_hex(), "result": "activity_ui__admin_item_saved" })))
}

async fn delete_item(State(state): State<AppState>, Authed(s): Authed, Path(id): Path<String>) -> ApiResult<Json<serde_json::Value>> {
    let item_id = parse_object_id(&id)?;
    let (universe_id, _) = require(&state, &s, Need::Edit).await?;
    let _guard = ActionGuard::acquire(&state, s.user_id)?;
    let http = bot_http(&state)?;
    let item = crate::database::items::get_item_by_id(item_id)
        .await?
        .filter(|i| i.universe_id == universe_id)
        .ok_or_else(|| ApiError::not_found("item_delete__not_found"))?;
    let deleted = crate::item::service::delete_item(&http, universe_id, &item.item_name, s.locale).await.map_err(|e| service_error(&e))?;
    publish_items_changed(&state, universe_id);
    Ok(Json(serde_json::json!({ "name": deleted.name, "affected_recipes": deleted.affected_recipes })))
}

// ---------------------------------------------------------------------------
// Recettes
// ---------------------------------------------------------------------------

#[derive(Serialize)]
struct LineDto {
    item_id: String,
    name: String,
    quantity: u64,
}

#[derive(Serialize)]
struct AdminRecipeDto {
    id: String,
    name: String,
    delay: u64,
    ingredients: Vec<LineDto>,
    results: Vec<LineDto>,
    tools: Vec<LineDto>,
    /// Publiée dans le forum wiki.
    in_wiki: bool,
}

fn recipe_dto(r: &Recipe, names: &HashMap<ObjectId, String>) -> Option<AdminRecipeDto> {
    let line = |(q, id): &(u64, ObjectId)| LineDto { item_id: id.to_hex(), name: names.get(id).cloned().unwrap_or_else(|| "?".into()), quantity: *q };
    Some(AdminRecipeDto {
        id: r._id?.to_hex(),
        name: r.recipe_name.clone(),
        delay: r.delay,
        ingredients: r.ingredients.iter().map(line).collect(),
        results: r.result.iter().map(line).collect(),
        tools: r.tools_needed.iter().map(|id| line(&(1, *id))).collect(),
        in_wiki: r.wiki_posts_id.as_ref().is_some_and(|p| !p.is_empty()),
    })
}

async fn list_recipes(State(state): State<AppState>, Authed(s): Authed) -> ApiResult<Json<serde_json::Value>> {
    let (universe_id, access) = require(&state, &s, Need::View).await?;
    let mut items = get_items_by_universe(universe_id).await?;
    items.sort_by_key(|i| i.item_name.to_lowercase());
    let names: HashMap<ObjectId, String> = items.iter().map(|i| (i._id, i.item_name.clone())).collect();
    let mut recipes: Vec<AdminRecipeDto> = Recipe::get_by_universe(universe_id).await?.iter().filter_map(|r| recipe_dto(r, &names)).collect();
    recipes.sort_by_key(|r| r.name.to_lowercase());
    let choices: Vec<PlaceChoice> = items.iter().map(|i| PlaceChoice { id: i._id.to_hex(), name: i.item_name.clone() }).collect();
    Ok(Json(serde_json::json!({ "recipes": recipes, "items": choices, "can_edit": allowed(Need::Edit, access), "server_time": now_secs() })))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LineBody {
    item_id: String,
    #[serde(default = "one")]
    quantity: u64,
}

fn one() -> u64 { 1 }

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecipeBody {
    name: String,
    #[serde(default)]
    delay: u64,
    #[serde(default)]
    ingredients: Vec<LineBody>,
    #[serde(default)]
    results: Vec<LineBody>,
    #[serde(default)]
    tools: Vec<String>,
    #[serde(default)]
    into_wiki: bool,
}

/// Recette validée : identifiants des objets (ingrédients et résultats avec quantité, outils).
#[derive(Debug, PartialEq)]
pub struct RecipeLines {
    pub ingredients: Vec<(u64, ObjectId)>,
    pub results: Vec<(u64, ObjectId)>,
    pub tools: Vec<ObjectId>,
}

/// Bornes d'une recette : nom, délai, nombre de lignes, quantités, au moins un ingrédient ou
/// un résultat (comme le parseur de `/recipe create`).
pub fn validate_recipe(body: &RecipeBody) -> ApiResult<RecipeLines> {
    if !valid_recipe_name(&body.name) {
        return Err(ApiError::bad_request("activity__admin_invalid_recipe_name"));
    }
    if body.delay > MAX_RECIPE_DELAY || body.ingredients.len() + body.results.len() + body.tools.len() > MAX_RECIPE_LINES {
        return Err(ApiError::bad_request("activity__invalid_request"));
    }
    if body.ingredients.is_empty() && body.results.is_empty() {
        return Err(ApiError::bad_request("recipe__empty_recipe"));
    }
    let lines = |v: &[LineBody]| -> ApiResult<Vec<(u64, ObjectId)>> {
        v.iter()
            .map(|l| {
                if !(1..=MAX_LINE_QUANTITY).contains(&l.quantity) {
                    return Err(ApiError::bad_request("recipe__invalid_quantity"));
                }
                Ok((l.quantity, parse_object_id(&l.item_id)?))
            })
            .collect()
    };
    Ok(RecipeLines { ingredients: lines(&body.ingredients)?, results: lines(&body.results)?, tools: body.tools.iter().map(|t| parse_object_id(t)).collect::<ApiResult<_>>()? })
}

/// Texte de la recette à partir des noms des objets de l'univers ; un objet inconnu (ou d'un
/// autre univers) est refusé.
pub fn recipe_body_text(lines: &RecipeLines, names: &HashMap<ObjectId, String>) -> ApiResult<String> {
    let name = |id: &ObjectId| names.get(id).cloned().ok_or_else(|| ApiError::not_found("recipe__item_not_found"));
    let pairs = |v: &[(u64, ObjectId)]| v.iter().map(|(q, id)| Ok((*q, name(id)?))).collect::<ApiResult<Vec<_>>>();
    let tools = lines.tools.iter().map(name).collect::<ApiResult<Vec<_>>>()?;
    Ok(recipe_text(&pairs(&lines.ingredients)?, &pairs(&lines.results)?, &tools))
}

fn save_error(e: SaveError) -> ApiError {
    match e {
        // Message du parseur (`recipe__item_not_found:<nom>`, `recipe__empty_recipe`…).
        SaveError::Parse(m) if m.starts_with("recipe__item_not_found") => ApiError::not_found("recipe__item_not_found"),
        SaveError::Parse(m) if crate::activity::http::is_fluent_key(&m) => ApiError::bad_request(&m),
        SaveError::Parse(m) => {
            tracing::error!(error = %m, "activity: recette illisible");
            ApiError::internal()
        }
        SaveError::Other(e) if e.to_string() == "recipe__name_taken" => ApiError::new(axum::http::StatusCode::CONFLICT, "recipe__name_taken"),
        SaveError::Other(e) => service_error(&e),
    }
}

async fn save_recipe(state: &AppState, s: &Session, id: Option<ObjectId>, body: RecipeBody) -> ApiResult<Json<serde_json::Value>> {
    let lines = validate_recipe(&body)?;
    let (universe_id, _) = require(state, s, Need::Edit).await?;
    let _guard = ActionGuard::acquire(state, s.user_id)?;
    let http = bot_http(state)?;
    let names: HashMap<ObjectId, String> = get_items_by_universe(universe_id).await?.into_iter().map(|i| (i._id, i.item_name)).collect();
    let text = recipe_body_text(&lines, &names)?;
    let recipe = crate::recipe::service::save_recipe_by_id(&http, universe_id, id, &body.name, body.delay, text, body.into_wiki).await.map_err(save_error)?;
    publish_items_changed(state, universe_id);
    Ok(Json(serde_json::json!({ "id": recipe._id.map(|i| i.to_hex()), "result": "activity_ui__admin_recipe_saved" })))
}

async fn create_recipe(State(state): State<AppState>, Authed(s): Authed, body: Result<Json<RecipeBody>, axum::extract::rejection::JsonRejection>) -> ApiResult<Json<serde_json::Value>> {
    save_recipe(&state, &s, None, json_body(body)?).await
}

async fn update_recipe(
    State(state): State<AppState>,
    Authed(s): Authed,
    Path(id): Path<String>,
    body: Result<Json<RecipeBody>, axum::extract::rejection::JsonRejection>,
) -> ApiResult<Json<serde_json::Value>> {
    let id = parse_object_id(&id)?;
    save_recipe(&state, &s, Some(id), json_body(body)?).await
}

async fn delete_recipe(State(state): State<AppState>, Authed(s): Authed, Path(id): Path<String>) -> ApiResult<Json<serde_json::Value>> {
    let id = parse_object_id(&id)?;
    let (universe_id, _) = require(&state, &s, Need::Edit).await?;
    let _guard = ActionGuard::acquire(&state, s.user_id)?;
    let http = bot_http(&state)?;
    let name = crate::recipe::service::delete_recipe(&http, universe_id, id).await.map_err(|e| service_error(&e))?;
    publish_items_changed(&state, universe_id);
    Ok(Json(serde_json::json!({ "name": name })))
}

/// Routes du panneau admin (corps limités à 64 Kio par le routeur principal).
pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/admin/players", get(list_players))
        .route("/admin/teleport", post(teleport))
        .route("/admin/items", get(list_items).post(create_item))
        .route("/admin/items/{id}", put(update_item).delete(delete_item))
        .route("/admin/recipes", get(list_recipes).post(create_recipe))
        .route("/admin/recipes/{id}", put(update_recipe).delete(delete_recipe))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rights_follow_decision_d3() {
        use AccessLevel::*;
        for need in [Need::View, Need::Teleport, Need::Edit] {
            assert!(!allowed(need, Player), "{need:?}");
            assert!(allowed(need, Admin), "{need:?}");
        }
        assert!(allowed(Need::View, Moderator));
        assert!(!allowed(Need::Teleport, Moderator), "comme /teleport : administrateurs seulement");
        assert!(!allowed(Need::Edit, Moderator), "ils n'éditent pas l'univers");
    }

    #[test]
    fn teleport_targets_are_bounded_valid_and_deduplicated() {
        let ids = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(teleport_targets(&ids(&["12", "34", "12"])).unwrap(), vec![12, 34]);
        assert!(teleport_targets(&[]).is_err());
        assert!(teleport_targets(&ids(&["12", "abc"])).is_err());
        assert!(teleport_targets(&ids(&["0"])).is_err());
        let many: Vec<String> = (1..=MAX_TELEPORT as u64 + 1).map(|i| i.to_string()).collect();
        assert!(teleport_targets(&many).is_err());
        assert_eq!(teleport_targets(&many[..MAX_TELEPORT]).unwrap().len(), MAX_TELEPORT);
    }

    fn character(user_id: u64, name: &str) -> Character {
        Character { _id: ObjectId::new(), user_id, universe_id: ObjectId::new(), name: name.into(), stats: vec![], description: None, image: None }
    }

    fn place(id: u64, name: &str) -> Place {
        Place { _id: ObjectId::new(), universe_id: ObjectId::new(), server_id: 1, category_id: id, role: id + 1, name: name.into(), modifiers: vec![], central_channel_id: None, wiki: Default::default() }
    }

    #[test]
    fn players_are_listed_by_name_with_their_position() {
        let at_port = TravelGroup { _id: ObjectId::new(), members: vec![1], actual_space_type: SpaceType::Place, actual_space_id: 10, ..Default::default() };
        let on_road = TravelGroup { _id: ObjectId::new(), members: vec![2, 3], actual_space_type: SpaceType::Road, actual_space_id: 99, road_id: Some(99), is_in_move: true, ..Default::default() };
        let rows = player_rows(&[character(1, "Zoé"), character(2, "anna"), character(4, "Bob")], &[at_port, on_road], &[place(10, "Port")], &[]);
        let summary: Vec<_> = rows.iter().map(|r| (r.name.as_str(), r.location.as_ref().map(|l| (l.kind, l.id.as_str(), l.name.clone())), r.moving, r.group_size)).collect();
        assert_eq!(
            summary,
            vec![
                ("anna", Some(("road", "99", None)), true, 2),
                ("Bob", None, false, 1),
                ("Zoé", Some(("place", "10", Some("Port".to_string()))), false, 1),
            ]
        );
    }

    fn body(json: &str) -> RecipeBody {
        serde_json::from_str(json).unwrap()
    }

    #[test]
    fn a_recipe_body_is_validated_before_anything_else() {
        let (a, b) = (ObjectId::new(), ObjectId::new());
        let ok = body(&format!(r#"{{"name":"Pain","delay":60,"ingredients":[{{"item_id":"{a}","quantity":2}}],"results":[{{"item_id":"{b}"}}],"tools":["{a}"]}}"#));
        assert_eq!(validate_recipe(&ok).unwrap(), RecipeLines { ingredients: vec![(2, a)], results: vec![(1, b)], tools: vec![a] });
        let err = |json: String| validate_recipe(&body(&json)).unwrap_err().key;
        assert_eq!(err(r#"{"name":"Pain"}"#.into()), "recipe__empty_recipe");
        assert_eq!(err(format!(r#"{{"name":"","results":[{{"item_id":"{b}"}}]}}"#)), "activity__admin_invalid_recipe_name");
        assert_eq!(err(format!(r#"{{"name":"Pain","results":[{{"item_id":"{b}","quantity":0}}]}}"#)), "recipe__invalid_quantity");
        assert_eq!(err(r#"{"name":"Pain","results":[{"item_id":"zz"}]}"#.into()), "activity__invalid_request");
        assert_eq!(err(format!(r#"{{"name":"Pain","delay":{},"results":[{{"item_id":"{b}"}}]}}"#, MAX_RECIPE_DELAY + 1)), "activity__invalid_request");
        assert!(serde_json::from_str::<RecipeBody>(r#"{"name":"Pain","secret":true}"#).is_err(), "champs inconnus refusés");
    }

    #[test]
    fn the_recipe_text_names_items_of_the_universe_only() {
        let (wood, chair, other) = (ObjectId::new(), ObjectId::new(), ObjectId::new());
        let names = HashMap::from([(wood, "Bois".to_string()), (chair, "Chaise".to_string())]);
        let lines = RecipeLines { ingredients: vec![(3, wood)], results: vec![(1, chair)], tools: vec![] };
        assert_eq!(recipe_body_text(&lines, &names).unwrap(), "< Bois 3\n> Chaise 1");
        let foreign = RecipeLines { tools: vec![other], ..lines };
        assert_eq!(recipe_body_text(&foreign, &names).unwrap_err().key, "recipe__item_not_found");
    }

    #[test]
    fn item_bodies_are_validated() {
        let parse = |json: &str| serde_json::from_str::<ItemBody>(json).unwrap();
        let (fields, wiki) = item_fields(&parse(r#"{"usage":"consumable","description":"  ","effects":" Force: +1 ","wiki":"publish"}"#)).unwrap();
        assert!(matches!(fields.usage, ItemUsage::Consumable));
        assert_eq!(fields.description, None, "texte vide → aucun");
        assert_eq!(fields.effects_raw, "Force: +1");
        assert_eq!(wiki, WikiAction::Publish);
        assert_eq!(item_fields(&parse(r#"{"usage":"magic"}"#)).unwrap_err().key, "activity__invalid_request");
        assert_eq!(item_fields(&parse(r#"{"usage":"none","wiki":"maybe"}"#)).unwrap_err().key, "activity__invalid_request");
        assert_eq!(item_fields(&parse(r#"{"usage":"none","image":"http://x"}"#)).unwrap_err().key, "activity__invalid_image_url");
        assert!(serde_json::from_str::<ItemBody>(r#"{"usage":"none","owner":"1"}"#).is_err(), "champs inconnus refusés");
    }

    // ---- routes : rien n'atteint la base ni Discord sans session, ni avec une entrée invalide ----

    mod routes {
        use super::*;
        use crate::activity::discord_api::*;
        use crate::activity::session::test_session;
        use axum::body::{to_bytes, Body};
        use axum::http::{Request, StatusCode};
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::Arc;
        use std::time::Duration;
        use tower::ServiceExt;

        /// Faux Discord qui compte les appels : aucun ne doit avoir lieu dans ces tests.
        #[derive(Default)]
        struct Fake(AtomicUsize);

        impl DiscordGateway for Fake {
            fn exchange_code<'a>(&'a self, _c: &'a str) -> BoxFuture<'a, Result<TokenGrant, DiscordError>> {
                self.0.fetch_add(1, Ordering::SeqCst);
                Box::pin(async { Err(DiscordError::Rejected) })
            }
            fn current_user<'a>(&'a self, _t: &'a str) -> BoxFuture<'a, Result<DiscordUser, DiscordError>> {
                self.0.fetch_add(1, Ordering::SeqCst);
                Box::pin(async { Err(DiscordError::Rejected) })
            }
            fn activity_instance<'a>(&'a self, _i: &'a str) -> BoxFuture<'a, Result<ActivityInstance, DiscordError>> {
                self.0.fetch_add(1, Ordering::SeqCst);
                Box::pin(async { Err(DiscordError::Rejected) })
            }
            fn member<'a>(&'a self, _g: u64, _u: u64) -> BoxFuture<'a, Result<Option<MemberInfo>, DiscordError>> {
                self.0.fetch_add(1, Ordering::SeqCst);
                Box::pin(async { Ok(None) })
            }
            fn bot_guilds<'a>(&'a self, _l: usize) -> BoxFuture<'a, Result<Vec<GuildSummary>, DiscordError>> {
                self.0.fetch_add(1, Ordering::SeqCst);
                Box::pin(async { Ok(vec![]) })
            }
            fn guild_counts<'a>(&'a self, _g: u64) -> BoxFuture<'a, Result<GuildCounts, DiscordError>> {
                self.0.fetch_add(1, Ordering::SeqCst);
                Box::pin(async { Ok(GuildCounts { channels: 0, roles: 0 }) })
            }
        }

        fn app() -> (Router, AppState, Arc<Fake>) {
            let fake = Arc::new(Fake::default());
            let state = AppState::new(fake.clone(), Duration::from_secs(60));
            (crate::activity::http::router(state.clone()), state, fake)
        }

        async fn call(app: &Router, method: &str, uri: &str, token: Option<&str>, body: Option<&str>) -> (StatusCode, serde_json::Value) {
            let mut req = Request::builder().method(method).uri(uri);
            if let Some(t) = token {
                req = req.header("authorization", format!("Bearer {t}"));
            }
            let req = match body {
                Some(b) => req.header("content-type", "application/json").body(Body::from(b.to_owned())).unwrap(),
                None => req.body(Body::empty()).unwrap(),
            };
            let res = app.clone().oneshot(req).await.unwrap();
            let status = res.status();
            let bytes = to_bytes(res.into_body(), 1 << 20).await.unwrap();
            (status, serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null))
        }

        const ID: &str = "0123456789abcdef01234567";

        #[tokio::test]
        async fn every_admin_panel_route_requires_a_session() {
            let (app, _, fake) = app();
            for (method, uri) in [
                ("GET", "/api/admin/players"),
                ("POST", "/api/admin/teleport"),
                ("GET", "/api/admin/items"),
                ("POST", "/api/admin/items"),
                ("PUT", "/api/admin/items/0123456789abcdef01234567"),
                ("DELETE", "/api/admin/items/0123456789abcdef01234567"),
                ("GET", "/api/admin/recipes"),
                ("POST", "/api/admin/recipes"),
                ("PUT", "/api/admin/recipes/0123456789abcdef01234567"),
                ("DELETE", "/api/admin/recipes/0123456789abcdef01234567"),
            ] {
                assert_eq!(call(&app, method, uri, None, None).await.0, StatusCode::UNAUTHORIZED, "{method} {uri}");
            }
            assert_eq!(fake.0.load(Ordering::SeqCst), 0);
        }

        #[tokio::test]
        async fn invalid_inputs_are_refused_before_the_database_and_discord() {
            let (app, state, fake) = app();
            let token = state.sessions.create(test_session(42));
            let cases: [(&str, String, Option<String>, &str); 8] = [
                ("POST", "/api/admin/teleport".into(), Some(r#"{"user_ids":[],"destination_id":"5"}"#.into()), "activity__invalid_request"),
                ("POST", "/api/admin/teleport".into(), Some(r#"{"user_ids":["1"],"destination_id":"x"}"#.into()), "activity__invalid_request"),
                ("POST", "/api/admin/teleport".into(), Some(r#"{"user_ids":["1"],"destination_id":"5","force":true}"#.into()), "activity__invalid_request"),
                ("PUT", "/api/admin/items/nope".into(), Some(r#"{"usage":"none"}"#.into()), "activity__invalid_request"),
                ("POST", "/api/admin/items".into(), Some(r#"{"name":"Épée!","usage":"none"}"#.into()), "create_item__invalid_name"),
                ("DELETE", "/api/admin/recipes/nope".into(), None, "activity__invalid_request"),
                ("POST", "/api/admin/recipes".into(), Some(r#"{"name":"Pain"}"#.into()), "recipe__empty_recipe"),
                ("PUT", format!("/api/admin/recipes/{ID}"), Some(format!(r#"{{"name":"Pain","results":[{{"item_id":"{ID}","quantity":0}}]}}"#)), "recipe__invalid_quantity"),
            ];
            for (method, uri, body, key) in cases {
                let (st, b) = call(&app, method, &uri, Some(&token), body.as_deref()).await;
                assert_eq!(st, StatusCode::BAD_REQUEST, "{method} {uri}");
                assert_eq!(b["error"], key, "{method} {uri}");
            }
            assert_eq!(fake.0.load(Ordering::SeqCst), 0, "Discord jamais appelé");
        }

        #[tokio::test]
        async fn a_session_without_universe_gets_nothing() {
            let (app, state, fake) = app();
            let token = state.sessions.create(Session { universe_id: None, access: AccessLevel::Admin, ..test_session(42) });
            for uri in ["/api/admin/players", "/api/admin/items", "/api/admin/recipes"] {
                let (st, b) = call(&app, "GET", uri, Some(&token), None).await;
                assert_eq!(st, StatusCode::NOT_FOUND, "{uri}");
                assert_eq!(b["error"], "activity__server_not_linked");
            }
            assert_eq!(fake.0.load(Ordering::SeqCst), 0);
        }
    }
}
