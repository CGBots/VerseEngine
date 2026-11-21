//! Routes admin de l'import de carte (étape E4 de `docs/activity/IMPORT_SPEC.md`).
//!
//! - `GET  /api/admin/import/servers` : serveurs utilisables pour l'import (I9) ;
//! - `POST /api/admin/import/map` : envoi de la sauvegarde `.map` d'Azgaar (32 Mio au plus) ;
//! - `POST /api/admin/import/plan` : aperçu, avec les choix de l'admin (I4, I7, I10, I16).
//!
//! Sécurité : rien n'est cru du client. Les droits (I1 : créateur de l'univers ou administrateur)
//! sont **recalculés à chaque requête** à partir de Discord (lu par le bot) et de la base ; chaque
//! serveur cible est vérifié de la même façon (l'admin doit y être administrateur, le bot présent,
//! et le serveur relié à aucun univers ou au même). Entrées bornées, `deny_unknown_fields`,
//! débit limité (1 action/s soutenue).

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use axum::body::Bytes;
use axum::extract::{DefaultBodyLimit, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use futures::TryStreamExt;
use mongodb::bson::oid::ObjectId;
use serde::{Deserialize, Serialize};
use serde_with::{serde_as, DisplayFromStr};

use crate::activity::discord_api::DiscordGateway;
use crate::activity::error::{ApiError, ApiResult};
use crate::activity::http::{AppState, Authed};
use crate::activity::map::azgaar::{match_places, resample_terrain, AzgaarMap, MatchStatus, Terrain};
use crate::activity::map::import_plan::{build_plan, ExistingPlace, ExistingRoad, ImportMode, ImportOptions, ImportPlan, KnownServer};
use crate::activity::map::network::Zone;
use crate::activity::map::partition::{ServerLoad, DEFAULT_MAX_PLACES_PER_SERVER};
use crate::activity::permissions::{compute_access, AccessInput, AccessLevel};
use crate::activity::session::Session;
use crate::database::server::{get_server_by_id, Server};
use crate::database::universe::{get_universe_by_id, Universe};
use crate::provisioning::server_setup::setup_budget;
use crate::universe::setup::setup_sub_command::SetupType;

/// Taille max d'une sauvegarde `.map` envoyée.
pub const MAX_MAP_BYTES: usize = 32 * 1024 * 1024;
/// Durée de vie d'une carte envoyée et pas encore importée.
pub const DRAFT_TTL: Duration = Duration::from_secs(3600);
/// Cartes en attente, tous admins confondus (mémoire bornée).
pub const MAX_DRAFTS: usize = 16;
/// Serveurs du bot examinés pour la liste des serveurs utilisables.
pub const MAX_GUILDS_LISTED: usize = 50;

/// I1 : import et génération réservés au créateur de l'univers et aux administrateurs.
pub fn may_import(access: AccessLevel, universe: Option<&Universe>, user_id: u64) -> bool {
    access == AccessLevel::Admin || universe.is_some_and(|u| u.creator_id == user_id)
}

// ---------------------------------------------------------------------------
// Cartes envoyées (en mémoire, par serveur et par admin)
// ---------------------------------------------------------------------------

pub struct Draft {
    pub map: Arc<AzgaarMap>,
    pub terrain: Option<Arc<Terrain>>,
    /// Carte générée (E7) : routes telles que dessinées.
    pub explicit_roads: Option<Arc<Vec<crate::activity::map::network::RoadCandidate>>>,
    pub created: Instant,
}

/// Ce que l'aperçu et l'exécution reprennent d'une carte en attente.
#[derive(Clone)]
pub struct DraftView {
    pub map: Arc<AzgaarMap>,
    pub explicit_roads: Option<Arc<Vec<crate::activity::map::network::RoadCandidate>>>,
}

impl DraftView {
    /// Carte générée par l'activité (et non importée d'Azgaar).
    pub fn generated(&self) -> bool {
        self.explicit_roads.is_some()
    }
}

#[derive(Default)]
pub struct ImportDrafts {
    inner: Mutex<HashMap<(u64, u64), Draft>>,
}

impl ImportDrafts {
    pub fn put(&self, guild_id: u64, user_id: u64, draft: Draft) {
        let mut map = self.inner.lock().unwrap_or_else(|p| p.into_inner());
        let now = Instant::now();
        map.retain(|_, d| now.duration_since(d.created) < DRAFT_TTL);
        if map.len() >= MAX_DRAFTS && !map.contains_key(&(guild_id, user_id)) {
            if let Some(oldest) = map.iter().min_by_key(|(_, d)| d.created).map(|(k, _)| *k) {
                map.remove(&oldest);
            }
        }
        map.insert((guild_id, user_id), draft);
    }

    pub fn get(&self, guild_id: u64, user_id: u64) -> Option<DraftView> {
        let map = self.inner.lock().unwrap_or_else(|p| p.into_inner());
        map.get(&(guild_id, user_id))
            .filter(|d| d.created.elapsed() < DRAFT_TTL)
            .map(|d| DraftView { map: d.map.clone(), explicit_roads: d.explicit_roads.clone() })
    }

    pub fn remove(&self, guild_id: u64, user_id: u64) {
        self.inner.lock().unwrap_or_else(|p| p.into_inner()).remove(&(guild_id, user_id));
    }
}

// ---------------------------------------------------------------------------
// Droits
// ---------------------------------------------------------------------------

/// Niveau d'accès de `user_id` sur `guild_id`, lu par le bot. `None` : pas membre.
async fn access_on(discord: &dyn DiscordGateway, guild_id: u64, user_id: u64, server: Option<&Server>) -> ApiResult<Option<AccessLevel>> {
    let member = discord.member(guild_id, user_id).await.map_err(|_| ApiError::upstream())?;
    Ok(member.map(|m| {
        compute_access(&AccessInput {
            user_id,
            guild_owner_id: m.guild_owner_id,
            member_roles: &m.roles,
            guild_roles: &m.guild_roles,
            guild_id,
            admin_role_id: server.and_then(|s| s.admin_role_id).map(|i| i.id),
            moderator_role_id: server.and_then(|s| s.moderator_role_id).map(|i| i.id),
        })
    }))
}

/// État du serveur de la session pour l'import.
pub struct ImportContext {
    pub server: Option<Server>,
    pub universe: Option<Universe>,
}

/// Vérifie, d'après Discord et la base (jamais la session seule), que l'auteur peut importer.
pub async fn require_importer(state: &AppState, s: &Session) -> ApiResult<ImportContext> {
    let server = get_server_by_id(s.guild_id).await?;
    let universe = match &server {
        Some(sv) => get_universe_by_id(sv.universe_id).await?,
        None => None,
    };
    let access = access_on(state.discord.as_ref(), s.guild_id, s.user_id, server.as_ref())
        .await?
        .ok_or_else(|| ApiError::forbidden("activity__not_a_member"))?;
    if !may_import(access, universe.as_ref(), s.user_id) {
        return Err(ApiError::forbidden("activity__import_forbidden"));
    }
    Ok(ImportContext { server, universe })
}

fn check_write(state: &AppState, s: &Session) -> ApiResult<()> {
    state.write_limiter.check(&format!("user:{}", s.user_id)).map_err(ApiError::too_many)
}

/// Un serveur cible : l'admin y est administrateur, le bot y est, et le serveur n'est relié à
/// aucun univers ou à celui de l'import. Retourne sa charge actuelle (+ son installation à venir
/// s'il n'est pas encore relié).
async fn check_target(state: &AppState, user_id: u64, guild_id: u64, universe_id: Option<ObjectId>, setup: SetupType) -> ApiResult<KnownServer> {
    let server = get_server_by_id(guild_id).await?;
    if let Some(sv) = &server {
        if Some(sv.universe_id) != universe_id {
            return Err(ApiError::new(axum::http::StatusCode::CONFLICT, "activity__import_server_in_other_universe"));
        }
    }
    match access_on(state.discord.as_ref(), guild_id, user_id, server.as_ref()).await? {
        Some(AccessLevel::Admin) => {}
        Some(_) => return Err(ApiError::forbidden("activity__import_not_admin_of_server")),
        None => return Err(ApiError::forbidden("activity__import_not_a_member_of_server")),
    }
    let counts = state.discord.guild_counts(guild_id).await.map_err(|_| ApiError::upstream())?;
    let mut load = ServerLoad { channels: counts.channels, roles: counts.roles };
    if server.is_none() {
        let (c, r) = setup_budget(setup);
        load.channels += c;
        load.roles += r;
    }
    Ok(KnownServer { guild_id, load })
}

// ---------------------------------------------------------------------------
// GET /api/admin/import/servers
// ---------------------------------------------------------------------------

#[serde_as]
#[derive(Serialize)]
struct ServerChoice {
    #[serde_as(as = "DisplayFromStr")]
    guild_id: u64,
    name: String,
    /// `none`, `this` (même univers) ou `other`.
    linked: &'static str,
    /// Utilisable pour l'import : administrateur, et pas relié à un autre univers.
    eligible: bool,
    current: bool,
}

async fn list_servers(State(state): State<AppState>, Authed(s): Authed) -> ApiResult<Json<serde_json::Value>> {
    let ctx = require_importer(&state, &s).await?;
    let universe_id = ctx.universe.as_ref().map(|u| u.universe_id);
    let guilds = state.discord.bot_guilds(MAX_GUILDS_LISTED).await.map_err(|_| ApiError::upstream())?;
    let mut out = vec![];
    for g in guilds {
        let server = get_server_by_id(g.id).await?;
        let linked = match &server {
            None => "none",
            Some(sv) if Some(sv.universe_id) == universe_id => "this",
            Some(_) => "other",
        };
        // Seuls les serveurs où l'admin est membre sont montrés (pas de fuite de la liste du bot).
        let Some(access) = access_on(state.discord.as_ref(), g.id, s.user_id, server.as_ref()).await? else { continue };
        out.push(ServerChoice { guild_id: g.id, name: g.name, linked, eligible: access == AccessLevel::Admin && linked != "other", current: g.id == s.guild_id });
    }
    out.sort_by_key(|c| (!c.current, c.name.to_lowercase()));
    Ok(Json(serde_json::json!({
        "servers": out,
        "max_places_per_server": DEFAULT_MAX_PLACES_PER_SERVER,
        // Ajouter un serveur : l'admin le crée dans Discord, puis y invite le bot avec ce lien.
        "invite_url": state.bot_invite_url,
    })))
}

// ---------------------------------------------------------------------------
// POST /api/admin/import/map
// ---------------------------------------------------------------------------

#[derive(Serialize)]
struct BurgDto {
    id: u64,
    name: String,
    x: f64,
    y: f64,
    capital: bool,
}

#[serde_as]
#[derive(Serialize)]
struct Suggestion {
    #[serde_as(as = "DisplayFromStr")]
    place_id: u64,
    place_name: String,
    /// `exact`, `ambiguous`, `partial` ou `none`.
    status: &'static str,
    burgs: Vec<u64>,
}

/// Rattachements proposés (« Créer en plus », I16), d'après les noms.
async fn suggestions(ctx: &ImportContext, map: &AzgaarMap) -> ApiResult<Vec<Suggestion>> {
    let mut suggestions = vec![];
    if let Some(u) = &ctx.universe {
        let places: Vec<crate::database::places::Place> = crate::database::places::get_places_by_universe_id(u.universe_id).await?.try_collect().await?;
        let names: Vec<(u64, String)> = places.iter().map(|p| (p.category_id, p.name.clone())).collect();
        for (place_id, status) in match_places(&names, &map.burgs) {
            let place_name = names.iter().find(|(id, _)| *id == place_id).map(|(_, n)| n.clone()).unwrap_or_default();
            let (status, burgs) = match status {
                MatchStatus::Exact(b) => ("exact", vec![b]),
                MatchStatus::Ambiguous(v) => ("ambiguous", v),
                MatchStatus::Partial(v) => ("partial", v),
                MatchStatus::None => ("none", vec![]),
            };
            suggestions.push(Suggestion { place_id, place_name, status, burgs });
        }
    }
    Ok(suggestions)
}

async fn upload_map(State(state): State<AppState>, Authed(s): Authed, body: Bytes) -> ApiResult<Json<serde_json::Value>> {
    check_write(&state, &s)?;
    let ctx = require_importer(&state, &s).await?;
    if body.is_empty() {
        return Err(ApiError::bad_request("activity__azgaar_invalid_json"));
    }
    let parsed = tokio::task::spawn_blocking(move || {
        crate::activity::map::azgaar_file::parse_map_file(&body).map(|f| {
            let terrain = resample_terrain(&f.map, crate::activity::map::azgaar::MAX_TERRAIN_SIDE);
            (f, terrain)
        })
    })
    .await
    .map_err(|_| ApiError::internal())?;
    let (file, terrain) = parsed.map_err(|e| ApiError::bad_request(e.key()))?;

    let suggestions = suggestions(&ctx, &file.map).await?;

    let map = &file.map;
    let response = serde_json::json!({
        "name": map.name,
        "version": file.version,
        "width": map.width,
        "height": map.height,
        "distance_unit": map.distance_unit,
        "route_count": map.routes.len(),
        "burgs": map.burgs.iter().map(|b| BurgDto { id: b.id, name: b.name.clone(), x: b.x, y: b.y, capital: b.capital }).collect::<Vec<_>>(),
        "terrain": terrain.as_ref().map(|t| serde_json::json!({ "cols": t.cols, "rows": t.rows, "heights": t.heights })),
        "suggestions": suggestions,
        "has_universe": ctx.universe.is_some(),
    });
    state.import_drafts.put(s.guild_id, s.user_id, Draft { map: Arc::new(file.map), terrain: terrain.map(Arc::new), explicit_roads: None, created: Instant::now() });
    Ok(Json(response))
}


// ---------------------------------------------------------------------------
// « Générer une map » (E7)
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GenerateRequest {
    /// Graine affichée (32 bits, lisible et exacte en JavaScript) ; absente : tirée au hasard.
    #[serde(default)]
    pub seed: Option<u32>,
    pub count: usize,
}

async fn generate_map(State(state): State<AppState>, Authed(s): Authed, Json(req): Json<GenerateRequest>) -> ApiResult<Json<crate::activity::map::generate::GeneratedMap>> {
    check_write(&state, &s)?;
    require_importer(&state, &s).await?;
    use crate::activity::map::generate::{generate, MAX_PLACES, MIN_PLACES};
    if !(MIN_PLACES..=MAX_PLACES).contains(&req.count) {
        return Err(ApiError::bad_request("activity__generate_bad_count"));
    }
    let seed = req.seed.unwrap_or_else(rand::random::<u32>);
    let map = tokio::task::spawn_blocking(move || generate(seed as u64, req.count)).await.map_err(|_| ApiError::internal())?;
    Ok(Json(map))
}

/// Carte générée puis modifiée par l'admin : vérifiée, puis gardée comme une carte envoyée.
async fn use_generated(State(state): State<AppState>, Authed(s): Authed, Json(map): Json<crate::activity::map::generate::GeneratedMap>) -> ApiResult<Json<serde_json::Value>> {
    check_write(&state, &s)?;
    let ctx = require_importer(&state, &s).await?;
    if map.seed > u32::MAX as u64 {
        return Err(ApiError::bad_request("activity__invalid_request"));
    }
    map.validate().map_err(ApiError::bad_request)?;
    let azgaar = map.to_azgaar();
    let roads = map.road_candidates();
    let response = serde_json::json!({
        "name": serde_json::Value::Null,
        "version": "generated",
        "width": azgaar.width,
        "height": azgaar.height,
        "distance_unit": "km",
        "route_count": roads.len(),
        "burgs": azgaar.burgs.iter().map(|b| BurgDto { id: b.id, name: b.name.clone(), x: b.x, y: b.y, capital: b.capital }).collect::<Vec<_>>(),
        "terrain": serde_json::Value::Null,
        "suggestions": suggestions(&ctx, &azgaar).await?,
        "has_universe": ctx.universe.is_some(),
        "generated": true,
    });
    state.import_drafts.put(s.guild_id, s.user_id, Draft { map: Arc::new(azgaar), terrain: None, explicit_roads: Some(Arc::new(roads)), created: Instant::now() });
    Ok(Json(response))
}

// ---------------------------------------------------------------------------
// POST /api/admin/import/plan
// ---------------------------------------------------------------------------

#[serde_as]
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AttachDto {
    pub burg_id: u64,
    #[serde_as(as = "DisplayFromStr")]
    pub place_id: u64,
}

#[serde_as]
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlanRequest {
    pub mode: ImportMode,
    #[serde(default)]
    pub include_sea: bool,
    #[serde(default)]
    pub burgs: Option<Vec<u64>>,
    #[serde(default)]
    pub secret: Vec<[u64; 2]>,
    #[serde(default)]
    pub attach: Vec<AttachDto>,
    /// Serveurs choisis (le serveur de la session est toujours ajouté en premier).
    #[serde_as(as = "Vec<DisplayFromStr>")]
    #[serde(default)]
    pub servers: Vec<u64>,
    /// Installation des serveurs qui ne sont pas encore reliés.
    #[serde(default = "default_setup")]
    pub setup: SetupType,
    #[serde(default)]
    pub zone: Vec<[f64; 2]>,
}

fn default_setup() -> SetupType {
    SetupType::FullSetup
}

/// Bornes des listes d'un `PlanRequest`.
pub const MAX_LIST: usize = 5000;
pub const MAX_SERVERS: usize = 50;
pub const MAX_ZONE_POINTS: usize = 500;

impl PlanRequest {
    pub fn validate(&self) -> Result<(), &'static str> {
        let too_long = self.burgs.as_ref().is_some_and(|b| b.len() > MAX_LIST) || self.secret.len() > MAX_LIST || self.attach.len() > MAX_LIST;
        if too_long || self.servers.len() > MAX_SERVERS || self.zone.len() > MAX_ZONE_POINTS {
            return Err("activity__invalid_request");
        }
        if self.zone.iter().flatten().any(|v| !v.is_finite()) {
            return Err("activity__invalid_request");
        }
        Ok(())
    }

    /// Serveurs dans l'ordre : celui de la session d'abord, sans doublon.
    pub fn ordered_servers(&self, current: u64) -> Vec<u64> {
        let mut out = vec![current];
        for g in &self.servers {
            if !out.contains(g) {
                out.push(*g);
            }
        }
        out
    }
}

/// Tout ce qu'il faut pour construire (et plus tard exécuter) un plan.
pub struct PlanInputs {
    pub map: Arc<AzgaarMap>,
    /// Carte générée par l'activité (enregistrée comme `Manual`, sans relief).
    pub generated: bool,
    pub options: ImportOptions,
    pub existing_places: Vec<ExistingPlace>,
    pub existing_roads: Vec<ExistingRoad>,
}

/// Rassemble les données du plan : carte envoyée, lieux et routes existants, serveurs vérifiés.
pub async fn plan_inputs(state: &AppState, s: &Session, ctx: &ImportContext, req: &PlanRequest) -> ApiResult<PlanInputs> {
    req.validate().map_err(ApiError::bad_request)?;
    let draft = state.import_drafts.get(s.guild_id, s.user_id).ok_or_else(|| ApiError::not_found("activity__import_no_map"))?;
    let map = draft.map.clone();
    let universe_id = ctx.universe.as_ref().map(|u| u.universe_id);

    let mut servers = vec![];
    for g in req.ordered_servers(s.guild_id) {
        servers.push(check_target(state, s.user_id, g, universe_id, req.setup).await?);
    }

    let (mut existing_places, mut existing_roads) = (vec![], vec![]);
    if let Some(uid) = universe_id {
        let burg_of: HashMap<u64, u64> = crate::database::universe_maps::get_universe_map(uid)
            .await?
            .map(|m| m.positions.iter().filter_map(|p| p.azgaar_burg_id.map(|b| (p.place_id, b))).collect())
            .unwrap_or_default();
        let places: Vec<crate::database::places::Place> = crate::database::places::get_places_by_universe_id(uid).await?.try_collect().await?;
        existing_places = places
            .into_iter()
            .map(|p| ExistingPlace { burg_id: burg_of.get(&p.category_id).copied(), category_id: p.category_id, name: p.name, server_id: p.server_id })
            .collect();
        let roads: Vec<crate::database::road::Road> = crate::database::road::get_roads_by_universe(uid).await?.try_collect().await?;
        existing_roads = roads
            .into_iter()
            .map(|r| ExistingRoad { channel_id: r.channel_id, place_a: r.place_one_id, place_b: r.place_two_id, distance: r.distance, secret: r.secret, server_id: r.server_id })
            .collect();
    }
    if req.mode == ImportMode::Reimport && universe_id.is_none() {
        return Err(ApiError::bad_request("activity__import_nothing_to_reimport"));
    }

    let (c, r) = setup_budget(req.setup);
    let options = ImportOptions {
        mode: req.mode,
        zone: Zone { polygon: req.zone.iter().map(|p| (p[0], p[1])).collect() },
        include_sea: req.include_sea,
        burgs: req.burgs.clone(),
        secret: req.secret.iter().map(|p| (p[0].min(p[1]), p[0].max(p[1]))).collect::<HashSet<_>>(),
        attach: req.attach.iter().map(|a| (a.burg_id, a.place_id)).collect(),
        servers,
        // Un serveur à créer : son installation, plus les rôles @everyone et du bot.
        new_server_load: ServerLoad { channels: c, roles: r + 2 },
        max_places: DEFAULT_MAX_PLACES_PER_SERVER,
        explicit_roads: draft.explicit_roads.as_ref().map(|r| r.as_ref().clone()),
    };
    Ok(PlanInputs { map, generated: draft.generated(), options, existing_places, existing_roads })
}

pub async fn compute_plan(inputs: &PlanInputs) -> ApiResult<ImportPlan> {
    let (map, options, places, roads) = (inputs.map.clone(), inputs.options.clone(), inputs.existing_places.clone(), inputs.existing_roads.clone());
    tokio::task::spawn_blocking(move || build_plan(&map, &options, &places, &roads))
        .await
        .map_err(|_| ApiError::internal())?
        .map_err(|e| ApiError::bad_request(e.key()))
}

async fn plan(State(state): State<AppState>, Authed(s): Authed, Json(req): Json<PlanRequest>) -> ApiResult<Json<serde_json::Value>> {
    check_write(&state, &s)?;
    let ctx = require_importer(&state, &s).await?;
    let inputs = plan_inputs(&state, &s, &ctx, &req).await?;
    let plan = compute_plan(&inputs).await?;
    // Ce que la suppression toucherait : joueurs à déplacer, objets posés (I15, Q2).
    let removal = match (&ctx.universe, crate::activity::import_exec::removes_something(&plan)) {
        (Some(u), true) => Some(crate::activity::import_removal::load_removal(u.universe_id, req.mode, &plan).await?.info),
        _ => None,
    };
    Ok(Json(serde_json::json!({
        "removal": removal,
        "plan": plan,
        "servers_to_create": plan.servers_to_create(),
        "discord_objects": plan.discord_objects_to_create(),
        "universe_name": ctx.universe.as_ref().map(|u| u.name.clone()),
    })))
}

// ---------------------------------------------------------------------------
// POST /api/admin/universe/fog (E8)
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FogRequest {
    pub enabled: bool,
}

/// I17 : le brouillard est le choix du **créateur** de l'univers (vérifié en base).
async fn set_fog(State(state): State<AppState>, Authed(s): Authed, Json(req): Json<FogRequest>) -> ApiResult<Json<serde_json::Value>> {
    check_write(&state, &s)?;
    let universe = get_universe_by_id(s.universe()?).await?.ok_or_else(|| ApiError::not_found("activity__universe_not_found"))?;
    if universe.creator_id != s.user_id {
        return Err(ApiError::forbidden("activity__fog_creator_only"));
    }
    crate::database::universe::set_fog_enabled(universe.universe_id, req.enabled).await?;
    // Les cartes ouvertes se rechargent (avec ou sans brouillard).
    state.hub.publish(crate::activity::events::Event { kind: crate::activity::events::EventKind::MapUpdated, universe_id: Some(universe.universe_id), users: vec![] });
    Ok(Json(serde_json::json!({ "fog_enabled": req.enabled })))
}

/// Routes admin de l'import. L'envoi de la carte a sa propre limite de taille.
pub fn routes() -> (Router<AppState>, Router<AppState>) {
    let standard = Router::new()
        .route("/admin/import/servers", get(list_servers))
        .route("/admin/import/plan", post(plan))
        .route("/admin/import/generate", post(generate_map))
        .route("/admin/universe/fog", post(set_fog));
    // Une carte générée de 600 lieux, renvoyée après modification, dépasse 64 Kio.
    let upload = Router::new()
        .route("/admin/import/map", post(upload_map))
        .route("/admin/import/generated", post(use_generated))
        .layer(DefaultBodyLimit::max(MAX_MAP_BYTES));
    (standard, upload)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn universe(creator: u64) -> Universe {
        Universe { universe_id: ObjectId::new(), name: "U".into(), creator_id: creator, global_time_modifier: 100, time_origin_timestamp: 0, creation_timestamp: 0, fog_enabled: false }
    }

    #[test]
    fn only_admins_and_the_creator_may_import() {
        assert!(may_import(AccessLevel::Admin, None, 1));
        assert!(!may_import(AccessLevel::Moderator, None, 1));
        assert!(may_import(AccessLevel::Player, Some(&universe(7)), 7), "the creator");
        assert!(!may_import(AccessLevel::Moderator, Some(&universe(7)), 8));
    }

    fn request(json: &str) -> Result<PlanRequest, serde_json::Error> {
        serde_json::from_str(json)
    }

    #[test]
    fn a_plan_request_is_strict_and_bounded() {
        assert!(request(r#"{"mode":"merge","unknown":1}"#).is_err(), "unknown fields are refused");
        let r = request(r#"{"mode":"replace","servers":["1100433828931194931"],"secret":[[3,1]]}"#).unwrap();
        assert_eq!(r.servers, vec![1100433828931194931]);
        assert!(r.validate().is_ok());
        let big = format!(r#"{{"mode":"replace","burgs":[{}]}}"#, vec!["1"; MAX_LIST + 1].join(","));
        assert_eq!(request(&big).unwrap().validate(), Err("activity__invalid_request"));
        let nan = request(r#"{"mode":"replace","zone":[[1e999,0]]}"#);
        assert!(nan.is_err() || nan.unwrap().validate().is_err());
    }

    #[test]
    fn the_invite_link_asks_for_the_bot_and_its_commands() {
        let url = crate::activity::http::bot_invite_url(123);
        assert_eq!(url, "https://discord.com/oauth2/authorize?client_id=123&scope=bot%20applications.commands&permissions=8");
    }

    #[test]
    fn the_current_server_comes_first_once() {
        let r = request(r#"{"mode":"replace","servers":["5","1","5"]}"#).unwrap();
        assert_eq!(r.ordered_servers(1), vec![1, 5]);
    }

    #[test]
    fn drafts_expire_and_are_bounded() {
        let d = ImportDrafts::default();
        let map = || Draft { map: Arc::new(crate::activity::map::azgaar::parse(r#"{"pack":{"burgs":[{},{"i":1,"name":"A","x":1,"y":1}]}}"#).unwrap()), terrain: None, explicit_roads: None, created: Instant::now() };
        for i in 0..(MAX_DRAFTS as u64 + 3) {
            d.put(1, i, map());
        }
        assert_eq!(d.inner.lock().unwrap().len(), MAX_DRAFTS);
        assert!(d.get(1, MAX_DRAFTS as u64 + 2).is_some(), "the newest is kept");
        let mut old = map();
        old.created = Instant::now() - DRAFT_TTL - Duration::from_secs(1);
        d.put(2, 2, old);
        assert!(d.get(2, 2).is_none(), "an expired draft is not returned");
        d.remove(1, MAX_DRAFTS as u64 + 2);
        assert!(d.get(1, MAX_DRAFTS as u64 + 2).is_none());
    }

    /// Réponses `map` et `plan` calculées sur une vraie carte (`AZGAAR_MAP_FILE`), écrites dans
    /// `IMPORT_FIXTURE_DIR` : la page de test du front (`web/activity/import-harness.html`) les
    /// sert à la place de l'API pour vérifier l'écran d'import dans un vrai navigateur.
    #[test]
    #[ignore = "integration test: needs AZGAAR_MAP_FILE and IMPORT_FIXTURE_DIR"]
    fn a_real_map_preview_fixture_is_written() {
        use crate::activity::map::import_plan::{build_plan, ImportOptions, KnownServer};
        let path = std::env::var("AZGAAR_MAP_FILE").expect("set AZGAAR_MAP_FILE");
        let dir = std::path::PathBuf::from(std::env::var("IMPORT_FIXTURE_DIR").expect("set IMPORT_FIXTURE_DIR"));
        let f = crate::activity::map::azgaar_file::parse_map_file(&std::fs::read(&path).unwrap()).unwrap();
        let terrain = resample_terrain(&f.map, crate::activity::map::azgaar::MAX_TERRAIN_SIDE);
        let upload = serde_json::json!({
            "name": f.map.name, "version": f.version, "width": f.map.width, "height": f.map.height,
            "distance_unit": f.map.distance_unit, "route_count": f.map.routes.len(),
            "burgs": f.map.burgs.iter().map(|b| BurgDto { id: b.id, name: b.name.clone(), x: b.x, y: b.y, capital: b.capital }).collect::<Vec<_>>(),
            "terrain": terrain.as_ref().map(|t| serde_json::json!({ "cols": t.cols, "rows": t.rows, "heights": t.heights })),
            "suggestions": [], "has_universe": false,
        });
        let options = ImportOptions { servers: vec![KnownServer { guild_id: 1100433828931194931, load: ServerLoad { channels: 14, roles: 8 } }], ..Default::default() };
        let plan = build_plan(&f.map, &options, &[], &[]).unwrap();
        let plan = serde_json::json!({ "plan": plan, "servers_to_create": plan.servers_to_create(), "discord_objects": plan.discord_objects_to_create(), "universe_name": null });
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("upload.json"), upload.to_string()).unwrap();
        std::fs::write(dir.join("plan.json"), plan.to_string()).unwrap();
        // Carte générée (E7), pour l'éditeur.
        std::fs::write(dir.join("generated.json"), serde_json::to_string(&crate::activity::map::generate::generate(4242, 40)).unwrap()).unwrap();
    }

    /// Bout en bout à travers le routeur, contre MongoDB, avec un faux Discord.
    mod routes_integration {
        use super::*;
        use crate::activity::discord_api::*;
        use crate::activity::session::test_session;
        use crate::test_support::{block_on, random_id};
        use axum::body::{to_bytes, Body};
        use axum::http::{Request, StatusCode};
        use tower::ServiceExt;

        /// Faux Discord : l'utilisateur 42 est `level` sur chaque serveur de `guilds`.
        struct Fake {
            guilds: Vec<u64>,
            level: AccessLevel,
        }

        impl DiscordGateway for Fake {
            fn exchange_code<'a>(&'a self, _c: &'a str) -> BoxFuture<'a, Result<TokenGrant, DiscordError>> {
                Box::pin(async { Ok(TokenGrant { access_token: "at".into(), expires_in: 600, scope: "identify".into() }) })
            }
            fn current_user<'a>(&'a self, _t: &'a str) -> BoxFuture<'a, Result<DiscordUser, DiscordError>> {
                Box::pin(async { Ok(DiscordUser { id: 42, username: "u".into(), global_name: None, avatar: None }) })
            }
            fn activity_instance<'a>(&'a self, id: &'a str) -> BoxFuture<'a, Result<ActivityInstance, DiscordError>> {
                let (id, guild) = (id.to_string(), self.guilds[0]);
                Box::pin(async move { Ok(ActivityInstance { instance_id: id, location: Some(InstanceLocation { guild_id: Some(guild), channel_id: Some(5) }), users: vec![42] }) })
            }
            fn member<'a>(&'a self, g: u64, _u: u64) -> BoxFuture<'a, Result<Option<MemberInfo>, DiscordError>> {
                let known = self.guilds.contains(&g);
                let admin = self.level == AccessLevel::Admin;
                Box::pin(async move {
                    Ok(known.then(|| MemberInfo {
                        roles: vec![77],
                        guild_owner_id: 1,
                        guild_roles: vec![(g, 0), (77, if admin { crate::activity::permissions::ADMINISTRATOR } else { 0 })],
                    }))
                })
            }
            fn bot_guilds<'a>(&'a self, _l: usize) -> BoxFuture<'a, Result<Vec<GuildSummary>, DiscordError>> {
                let g = self.guilds.clone();
                Box::pin(async move { Ok(g.into_iter().map(|id| GuildSummary { id, name: format!("G{id}") }).collect()) })
            }
            fn guild_counts<'a>(&'a self, _g: u64) -> BoxFuture<'a, Result<GuildCounts, DiscordError>> {
                Box::pin(async { Ok(GuildCounts { channels: 3, roles: 2 }) })
            }
        }

        fn app(guilds: Vec<u64>, level: AccessLevel) -> (Router, AppState, String) {
            let state = AppState::new(Arc::new(Fake { guilds: guilds.clone(), level }), Duration::from_secs(60));
            let token = state.sessions.create(Session { user_id: 42, guild_id: guilds[0], universe_id: None, access: level, ..test_session(42) });
            (crate::activity::http::router(state.clone()), state, token)
        }

        async fn call(app: &Router, method: &str, uri: &str, token: &str, body: Body, json: bool) -> (StatusCode, serde_json::Value) {
            let mut req = Request::builder().method(method).uri(uri).header("authorization", format!("Bearer {token}"));
            if json {
                req = req.header("content-type", "application/json");
            }
            let res = app.clone().oneshot(req.body(body).unwrap()).await.unwrap();
            let status = res.status();
            let bytes = to_bytes(res.into_body(), 64 << 20).await.unwrap();
            (status, serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null))
        }

        fn sample() -> Body {
            Body::from(crate::activity::map::azgaar_file::tests::sample_map_text())
        }

        #[test]
        #[ignore = "integration test: needs a running MongoDB (MONGODB_* env vars); run with --ignored"]
        fn an_admin_of_a_server_without_universe_uploads_a_map_and_gets_a_plan() {
            block_on(async {
                let guild = random_id();
                let (app, _, token) = app(vec![guild], AccessLevel::Admin);
                let (st, body) = call(&app, "POST", "/api/admin/import/map", &token, sample(), false).await;
                assert_eq!(st, StatusCode::OK, "{body}");
                assert!(body["burgs"].as_array().unwrap().len() >= 2);
                assert_eq!(body["has_universe"], false);

                let (st, body) = call(&app, "POST", "/api/admin/import/plan", &token, Body::from(r#"{"mode":"replace"}"#), true).await;
                assert_eq!(st, StatusCode::OK, "{body}");
                assert_eq!(body["servers_to_create"], 0);
                assert_eq!(body["plan"]["servers"][0]["guild_id"], guild.to_string());
                assert!(body["plan"]["places"].as_array().unwrap().iter().all(|p| p["action"]["kind"] == "create"));

                let (st, body) = call(&app, "GET", "/api/admin/import/servers", &token, Body::empty(), false).await;
                assert_eq!(st, StatusCode::OK);
                assert_eq!(body["servers"][0]["eligible"], true);
                assert_eq!(body["servers"][0]["current"], true);
            });
        }

        #[test]
        #[ignore = "integration test: needs a running MongoDB (MONGODB_* env vars); run with --ignored"]
        fn a_player_cannot_import() {
            block_on(async {
                let (app, _, token) = app(vec![random_id()], AccessLevel::Player);
                let (st, body) = call(&app, "POST", "/api/admin/import/map", &token, sample(), false).await;
                assert_eq!((st, body["error"].as_str()), (StatusCode::FORBIDDEN, Some("activity__import_forbidden")));
                let (st, _) = call(&app, "GET", "/api/admin/import/servers", &token, Body::empty(), false).await;
                assert_eq!(st, StatusCode::FORBIDDEN);
            });
        }

        #[test]
        #[ignore = "integration test: needs a running MongoDB (MONGODB_* env vars); run with --ignored"]
        fn a_server_of_another_universe_or_unknown_is_refused_as_target() {
            block_on(async {
                let (guild, other, foreign) = (random_id(), random_id(), random_id());
                let mut sv = Server::default();
                sv.server_id(other).universe_id(ObjectId::new());
                sv.insert_server().await.unwrap();
                let (app, _, token) = app(vec![guild, other], AccessLevel::Admin);
                let (st, _) = call(&app, "POST", "/api/admin/import/map", &token, sample(), false).await;
                assert_eq!(st, StatusCode::OK);
                let req = format!(r#"{{"mode":"replace","servers":["{other}"]}}"#);
                let (st, body) = call(&app, "POST", "/api/admin/import/plan", &token, Body::from(req), true).await;
                assert_eq!((st, body["error"].as_str()), (StatusCode::CONFLICT, Some("activity__import_server_in_other_universe")));
                // Un serveur où l'admin n'est pas : refusé, même si le client l'envoie.
                tokio::time::sleep(Duration::from_millis(1100)).await;
                let req = format!(r#"{{"mode":"replace","servers":["{foreign}"]}}"#);
                let (st, body) = call(&app, "POST", "/api/admin/import/plan", &token, Body::from(req), true).await;
                assert_eq!((st, body["error"].as_str()), (StatusCode::FORBIDDEN, Some("activity__import_not_a_member_of_server")));
                get_db_client_delete_server(other).await;
            });
        }

        async fn get_db_client_delete_server(guild: u64) {
            crate::database::db_client::get_db_client()
                .await
                .database(crate::database::db_namespace::VERSEENGINE_DB_NAME)
                .collection::<mongodb::bson::Document>(crate::database::db_namespace::SERVERS_COLLECTION_NAME)
                .delete_one(mongodb::bson::doc! {"server_id": guild.to_string()})
                .await
                .unwrap();
        }

        /// E7 : une carte générée, modifiée par l'admin (renommage, route supprimée, lieu et route
        /// ajoutés), puis renvoyée, donne un aperçu avec exactement les routes dessinées.
        #[test]
        #[ignore = "integration test: needs a running MongoDB (MONGODB_* env vars); run with --ignored"]
        fn a_generated_and_edited_map_gets_a_plan() {
            use crate::activity::map::generate::{GenPlace, GenRoad, GeneratedMap};
            block_on(async {
                let (app, _, token) = app(vec![random_id()], AccessLevel::Admin);
                let (st, body) = call(&app, "POST", "/api/admin/import/generate", &token, Body::from(r#"{"seed":1234,"count":12}"#), true).await;
                assert_eq!(st, StatusCode::OK, "{body}");
                let mut map: GeneratedMap = serde_json::from_value(body.clone()).unwrap();
                assert_eq!(map.places.len(), 12);
                // Même graine : même carte.
                tokio::time::sleep(Duration::from_millis(1100)).await;
                let (_, again) = call(&app, "POST", "/api/admin/import/generate", &token, Body::from(r#"{"seed":1234,"count":12}"#), true).await;
                assert_eq!(again, body);

                map.places[0].name = "Ma capitale".into();
                let removed = map.roads.pop().unwrap();
                let new_id = 100;
                map.places.push(GenPlace { id: new_id, name: "Nouveau".into(), x: 10.0, y: 10.0, capital: false });
                map.roads.push(GenRoad { a: new_id, b: map.places[0].id, control: [20.0, 20.0] });
                tokio::time::sleep(Duration::from_millis(1100)).await;
                let (st, uploaded) = call(&app, "POST", "/api/admin/import/generated", &token, Body::from(serde_json::to_string(&map).unwrap()), true).await;
                assert_eq!(st, StatusCode::OK, "{uploaded}");
                assert_eq!(uploaded["generated"], true);
                assert!(uploaded["burgs"].as_array().unwrap().iter().any(|b| b["name"] == "Ma capitale"));

                tokio::time::sleep(Duration::from_millis(1100)).await;
                let (st, plan) = call(&app, "POST", "/api/admin/import/plan", &token, Body::from(r#"{"mode":"replace"}"#), true).await;
                assert_eq!(st, StatusCode::OK, "{plan}");
                let roads = plan["plan"]["roads"].as_array().unwrap();
                assert_eq!(roads.len(), map.roads.len(), "exactly the drawn roads");
                let pair = |r: &serde_json::Value| (r["burg_a"].as_u64().unwrap(), r["burg_b"].as_u64().unwrap());
                assert!(!roads.iter().any(|r| pair(r) == (removed.a.min(removed.b), removed.a.max(removed.b))), "the deleted road is gone");
                assert_eq!(plan["plan"]["places"].as_array().unwrap().len(), 13);

                // Une carte invalide est refusée.
                tokio::time::sleep(Duration::from_millis(1100)).await;
                map.roads.push(GenRoad { a: new_id, b: 9999, control: [0.0, 0.0] });
                let (st, body) = call(&app, "POST", "/api/admin/import/generated", &token, Body::from(serde_json::to_string(&map).unwrap()), true).await;
                assert_eq!((st, body["error"].as_str()), (StatusCode::BAD_REQUEST, Some("activity__generate_bad_road")));
            });
        }

        /// I2 : l'activité s'ouvre sur un serveur sans univers pour un administrateur, pas pour un
        /// joueur.
        #[test]
        #[ignore = "integration test: needs a running MongoDB (MONGODB_* env vars); run with --ignored"]
        fn the_activity_opens_without_universe_for_admins_only() {
            block_on(async {
                let body = r#"{"code":"good","instance_id":"i-1"}"#;
                let post = || Request::builder().method("POST").uri("/api/auth/token").header("content-type", "application/json").body(Body::from(body)).unwrap();
                let (admin, _, _) = app(vec![random_id()], AccessLevel::Admin);
                let res = admin.oneshot(post()).await.unwrap();
                assert_eq!(res.status(), StatusCode::OK);
                let json: serde_json::Value = serde_json::from_slice(&to_bytes(res.into_body(), 1 << 20).await.unwrap()).unwrap();
                assert_eq!(json["universe"], serde_json::Value::Null);
                assert_eq!(json["can_import"], true);

                let (player, _, _) = app(vec![random_id()], AccessLevel::Player);
                let res = player.oneshot(post()).await.unwrap();
                assert_eq!(res.status(), StatusCode::NOT_FOUND);
            });
        }

        /// La limite de 64 Kio reste sur les autres routes ; seule la carte peut être plus grosse.
        #[tokio::test]
        async fn only_the_map_upload_accepts_large_bodies() {
            let (app, _, token) = app(vec![1], AccessLevel::Admin);
            let big = vec![b'x'; 200 * 1024];
            let (st, _) = call(&app, "POST", "/api/admin/import/plan", &token, Body::from(big.clone()), true).await;
            assert_eq!(st, StatusCode::PAYLOAD_TOO_LARGE);
            let too_big = vec![b'x'; MAX_MAP_BYTES + 1];
            let (st, _) = call(&app, "POST", "/api/admin/import/map", &token, Body::from(too_big), false).await;
            assert_eq!(st, StatusCode::PAYLOAD_TOO_LARGE);
        }
    }
}
