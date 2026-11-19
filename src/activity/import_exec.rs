//! Exécution d'un import de carte (étape E4b de `docs/activity/IMPORT_SPEC.md`).
//!
//! `POST /api/admin/import/execute` recalcule le plan côté serveur (jamais celui du client),
//! vérifie la double confirmation (I12 : saisie du nom de l'univers), puis lance la création en
//! tâche de fond. `GET /api/admin/import/jobs/{id}` donne la progression (I13).
//!
//! Ordre de création, tout passant par le même [`Provisioner`] (journal écrit en base) :
//! 1. l'univers et son premier serveur, s'il n'existe pas (I2, I14) ;
//! 2. les autres serveurs choisis, rattachés et installés ;
//! 3. les catégories de routes manquantes (50 salons par catégorie) ;
//! 4. les lieux (rôle, catégorie, salon générique) ;
//! 5. les routes ;
//! 6. dans **une transaction** : mises à jour des routes existantes et carte `universe_maps`.
//!
//! Au moindre échec, tout ce qui a été créé est supprimé, avec sa propre progression ; si le bot
//! s'arrête au milieu, le démarrage suivant fait de même (`rollback_interrupted_jobs`).

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use axum::extract::{Path, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use mongodb::bson::doc;
use mongodb::bson::oid::ObjectId;
use serde::{Deserialize, Serialize};
use serenity::all::{ChannelType, CreateChannel, Http, RoleId};
use tokio::sync::watch;

use crate::activity::admin_import::{compute_plan, plan_inputs, require_importer, ImportContext, PlanInputs, PlanRequest};
use crate::activity::error::{ApiError, ApiResult};
use crate::activity::http::{AppState, Authed};
use crate::activity::map::azgaar::{burg_height, AzgaarMap};
use crate::activity::map::import_plan::{ImportPlan, PlaceAction, RoadAction};
use crate::activity::map::partition::DISCORD_MAX_CHANNELS_PER_CATEGORY;
use crate::database::db_namespace::{ROADS_COLLECTION_NAME, UNIVERSE_MAPS_COLLECTION_NAME, VERSEENGINE_DB_NAME};
use crate::database::places::Place;
use crate::database::server::{get_server_by_id, Server};
use crate::database::universe_maps::{MapPosition, MapSource, MapTerrain, MapZone, OutsideZone, RoadPath, UniverseMap};
use crate::discord::channels::get_road_category_permission_set;
use crate::place::create_place_sub_command::guild_locale;
use crate::provisioning::place::provision_place;
use crate::provisioning::road::provision_road;
use crate::provisioning::universe::{add_server, create_universe, valid_universe_name, NewUniverse};
use crate::provisioning::{Phase, Progress, Provisioner};
use crate::translation::get_by_locale;
use crate::universe::setup::setup_sub_command::SetupType;

// ---------------------------------------------------------------------------
// Requête
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NewUniverseDto {
    pub name: String,
    /// Vitesse du temps en %, facultative (temps réel par défaut, I14).
    #[serde(default)]
    pub time_modifier: Option<u32>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecuteRequest {
    pub plan: PlanRequest,
    /// Nom de l'univers saisi par l'admin (I12).
    pub confirm_name: String,
    /// Obligatoire si le serveur n'est relié à aucun univers.
    #[serde(default)]
    pub new_universe: Option<NewUniverseDto>,
}

/// I12 : le nom saisi doit être exactement celui de l'univers (espaces de bord ignorés).
pub fn name_confirmed(expected: &str, typed: &str) -> bool {
    !expected.trim().is_empty() && expected.trim() == typed.trim()
}

/// Ce que l'exécution ne sait pas encore faire, ou qui manque : clé d'erreur, ou `None`.
pub fn blocking_reason(plan: &ImportPlan) -> Option<&'static str> {
    if plan.servers_to_create() > 0 {
        Some("activity__import_servers_missing")
    } else if !plan.remove_places.is_empty() || !plan.remove_roads.is_empty() {
        // Suppression des lieux existants : personnages, voyages et objets posés (E6).
        Some("activity__import_removal_not_ready")
    } else {
        None
    }
}

// ---------------------------------------------------------------------------
// Tâches en cours
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum Outcome {
    Running,
    Done { universe_id: String },
    Failed { error: &'static str, rolled_back: bool },
}

struct JobEntry {
    guild_id: u64,
    progress: watch::Receiver<Progress>,
    outcome: Arc<Mutex<Outcome>>,
}

/// Tâches d'import du processus. Une seule à la fois par serveur.
#[derive(Default)]
pub struct ImportJobs {
    inner: Mutex<HashMap<String, JobEntry>>,
}

impl ImportJobs {
    fn running_on(&self, guild_id: u64) -> bool {
        self.inner.lock().unwrap_or_else(|p| p.into_inner()).values().any(|j| j.guild_id == guild_id && *j.outcome.lock().unwrap_or_else(|p| p.into_inner()) == Outcome::Running)
    }
}

// ---------------------------------------------------------------------------
// Exécution
// ---------------------------------------------------------------------------

/// Paramètres de l'exécution, déjà vérifiés.
pub struct Execution {
    pub user_id: u64,
    pub guild_id: u64,
    pub universe: Option<crate::database::universe::Universe>,
    pub new_universe: Option<(String, Option<u32>)>,
    pub setup: SetupType,
    pub plan: ImportPlan,
    pub map: Arc<AzgaarMap>,
    pub zone: Vec<[f64; 2]>,
    /// Tests : échec simulé après ce nombre de lieux créés.
    pub fail_after_places: Option<usize>,
}

/// Catégorie de routes de `server` qui a encore de la place, ou une nouvelle.
struct RoadCategories {
    /// (catégorie, places libres)
    slots: Vec<(u64, u32)>,
}

async fn road_categories(p: &mut Provisioner, server: &Server, needed: u32, locale: &str) -> Result<RoadCategories, &'static str> {
    let mut slots = vec![];
    if let Some(main) = server.road_category_id {
        let used = crate::database::db_client::get_db_client()
            .await
            .database(VERSEENGINE_DB_NAME)
            .collection::<mongodb::bson::Document>(ROADS_COLLECTION_NAME)
            .count_documents(doc! {"server_id": server.server_id.to_string()})
            .await
            .map_err(|_| "activity__internal_error")? as u32;
        slots.push((main.id, DISCORD_MAX_CHANNELS_PER_CATEGORY.saturating_sub(used)));
    }
    let free: u32 = slots.iter().map(|s| s.1).sum();
    let missing = needed.saturating_sub(free).div_ceil(DISCORD_MAX_CHANNELS_PER_CATEGORY);
    let role = |id: Option<crate::database::server::Id>| id.map(|i| RoleId::new(i.id));
    let (Some(player), Some(spectator), Some(moderator)) = (role(server.player_role_id), role(server.spectator_role_id), role(server.moderator_role_id)) else {
        return Err("setup__roles_setup_failed");
    };
    for n in 0..missing {
        let _ = n;
        let name = format!("{} {}", get_by_locale(locale, "road_channel_name", None, None), slots.len() + 1);
        let perms = get_road_category_permission_set(RoleId::new(server.server_id), player, spectator, moderator);
        let c = p
            .create_channel(server.server_id, CreateChannel::new(name).kind(ChannelType::Category).permissions(perms))
            .await
            .map_err(|_| "setup__road_category_not_created")?;
        p.step_done();
        slots.push((c.id.get(), DISCORD_MAX_CHANNELS_PER_CATEGORY));
    }
    Ok(RoadCategories { slots })
}

impl RoadCategories {
    fn take(&mut self) -> Option<u64> {
        let slot = self.slots.iter_mut().find(|s| s.1 > 0)?;
        slot.1 -= 1;
        Some(slot.0)
    }
}

/// Exécute le plan. En cas d'échec, tout est défait et la clé d'erreur est retournée.
pub async fn execute(p: &mut Provisioner, exec: Execution) -> Result<ObjectId, &'static str> {
    match run(p, &exec).await {
        Ok(id) => {
            p.finish().await;
            Ok(id)
        }
        Err(key) => {
            let left = p.rollback().await;
            if !left.is_empty() {
                eprintln!("[import] rollback incomplete: {left:?}");
            }
            Err(key)
        }
    }
}

async fn run(p: &mut Provisioner, exec: &Execution) -> Result<ObjectId, &'static str> {
    let http: Arc<Http> = p.http().clone();
    let plan = &exec.plan;

    // 1. Univers.
    let universe_id = match (&exec.universe, &exec.new_universe) {
        (Some(u), _) => u.universe_id,
        (None, Some((name, time_modifier))) => {
            let locale = guild_locale(&http, exec.guild_id).await;
            let new = NewUniverse { name, creator_id: exec.user_id, guild_id: exec.guild_id, setup: exec.setup, time_modifier: *time_modifier, locale: &locale };
            create_universe(p, new).await.map_err(|e| e.key)?.0.universe_id
        }
        (None, None) => return Err("activity__import_universe_required"),
    };

    // 2. Serveurs (dans l'ordre du plan ; le premier est celui de la session).
    let mut servers: Vec<(Server, String)> = vec![];
    for planned in &plan.servers {
        let guild = planned.guild_id.ok_or("activity__import_servers_missing")?;
        let locale = guild_locale(&http, guild).await;
        let server = match get_server_by_id(guild).await.map_err(|_| "activity__internal_error")? {
            Some(s) if s.universe_id == universe_id => s,
            Some(_) => return Err("activity__import_server_in_other_universe"),
            None => add_server(p, universe_id, guild, exec.setup, &locale).await.map_err(|e| e.key)?,
        };
        servers.push((server, locale));
    }

    // 3. Catégories de routes.
    let mut categories = vec![];
    for (i, (server, locale)) in servers.iter().enumerate() {
        let needed = plan.roads.iter().filter(|r| r.host == i && r.action == RoadAction::Create).count() as u32;
        categories.push(road_categories(p, server, needed, locale).await?);
    }

    // 4. Lieux.
    let existing: HashMap<u64, Place> = {
        use futures::TryStreamExt;
        let all: Vec<Place> = crate::database::places::get_places_by_universe_id(universe_id).await.map_err(|_| "activity__internal_error")?.try_collect().await.map_err(|_| "activity__internal_error")?;
        all.into_iter().map(|pl| (pl.category_id, pl)).collect()
    };
    let mut place_of_burg: HashMap<u64, Place> = HashMap::new();
    let mut created = 0usize;
    for planned in &plan.places {
        let place = match &planned.action {
            PlaceAction::Keep { category_id } | PlaceAction::Attach { category_id } => existing.get(category_id).cloned().ok_or("activity__import_unknown_place")?,
            PlaceAction::Create => {
                if exec.fail_after_places == Some(created) {
                    return Err("activity__internal_error");
                }
                let (server, locale) = &servers[planned.server];
                let place = provision_place(p, universe_id, server.server_id, &planned.name, locale).await.map_err(|e| e.key)?;
                for _ in 0..3 {
                    p.step_done();
                }
                created += 1;
                place
            }
        };
        place_of_burg.insert(planned.burg_id, place);
    }

    // 5. Routes.
    let mut updates = vec![];
    for road in &plan.roads {
        let (a, b) = (&place_of_burg[&road.burg_a], &place_of_burg[&road.burg_b]);
        match &road.action {
            RoadAction::Create => {
                let host = &servers[road.host].0;
                let category = categories[road.host].take().ok_or("setup__road_category_not_created")?;
                provision_road(p, host.server_id, category, a, b, road.distance, road.secret).await.map_err(|e| e.key)?;
                p.step_done();
            }
            RoadAction::Update { channel_id } => updates.push((*channel_id, road.distance, road.secret)),
            RoadAction::Keep { .. } => {}
        }
    }

    // 6. Mises à jour et carte, en une transaction.
    let map = universe_map(universe_id, &exec.map, plan, &place_of_burg, &exec.zone);
    save_in_transaction(universe_id, &updates, &map).await.map_err(|_| "activity__internal_error")?;
    Ok(universe_id)
}

/// Carte de l'univers après l'import : positions (avec le bourg d'origine, pour un ré-import),
/// tracés, relief (0–100 d'Azgaar ramené à 0–255) et zone.
pub fn universe_map(universe_id: ObjectId, map: &AzgaarMap, plan: &ImportPlan, place_of_burg: &HashMap<u64, Place>, zone: &[[f64; 2]]) -> UniverseMap {
    let burgs: HashMap<u64, &crate::activity::map::azgaar::Burg> = map.burgs.iter().map(|b| (b.id, b)).collect();
    let positions = plan
        .places
        .iter()
        .filter_map(|pl| {
            let place = place_of_burg.get(&pl.burg_id)?;
            let z = burgs.get(&pl.burg_id).and_then(|b| burg_height(map, b)).map(|h| h as f64 / 100.0).unwrap_or(0.0);
            Some(MapPosition { place_id: place.category_id, x: pl.x, y: pl.y, z, azgaar_burg_id: Some(pl.burg_id) })
        })
        .collect();
    let road_paths = plan
        .roads
        .iter()
        .filter_map(|r| {
            let (a, b) = (place_of_burg.get(&r.burg_a)?.category_id, place_of_burg.get(&r.burg_b)?.category_id);
            let mut points: Vec<[f64; 2]> = r.path.iter().map(|&(x, y)| [x, y]).collect();
            // Tracé orienté du plus petit identifiant de lieu vers le plus grand.
            if a > b {
                points.reverse();
            }
            Some(RoadPath { a: a.min(b), b: a.max(b), points })
        })
        .collect();
    let terrain = crate::activity::map::azgaar::resample_terrain(map, crate::activity::map::azgaar::MAX_TERRAIN_SIDE).map(|t| MapTerrain {
        cols: t.cols as u32,
        rows: t.rows as u32,
        heights: t.heights.iter().map(|&h| ((h.min(100) as u32 * 255) / 100) as u8).collect(),
    });
    UniverseMap {
        id: ObjectId::new(),
        universe_id,
        source: MapSource::Azgaar,
        width: map.width,
        height: map.height,
        positions,
        terrain,
        zone: (zone.len() >= 3).then(|| MapZone { polygon: zone.to_vec(), outside: OutsideZone::Grey }),
        graph_hash: None,
        road_paths,
        background: None,
        updated_at: chrono::Utc::now().timestamp().max(0) as u64,
    }
}

async fn save_in_transaction(universe_id: ObjectId, updates: &[(u64, u64, bool)], map: &UniverseMap) -> mongodb::error::Result<()> {
    let client = crate::database::db_client::get_db_client().await;
    let db = client.database(VERSEENGINE_DB_NAME);
    let previous = crate::database::universe_maps::get_universe_map(universe_id).await?;
    let mut stored = map.clone();
    if let Some(p) = &previous {
        stored.id = p.id;
        stored.background = p.background.clone();
    }
    let mut session = client.start_session().await?;
    session.start_transaction().await?;
    let result: mongodb::error::Result<()> = async {
        for (channel_id, distance, secret) in updates {
            db.collection::<mongodb::bson::Document>(ROADS_COLLECTION_NAME)
                .update_one(doc! {"universe_id": universe_id, "channel_id": channel_id.to_string()}, doc! {"$set": {"distance": distance.to_string(), "secret": secret}})
                .session(&mut session)
                .await?;
        }
        db.collection::<UniverseMap>(UNIVERSE_MAPS_COLLECTION_NAME)
            .replace_one(doc! {"universe_id": universe_id}, &stored)
            .upsert(true)
            .session(&mut session)
            .await?;
        Ok(())
    }
    .await;
    match result {
        Ok(()) => session.commit_transaction().await,
        Err(e) => {
            let _ = session.abort_transaction().await;
            Err(e)
        }
    }
}

// ---------------------------------------------------------------------------
// Routes
// ---------------------------------------------------------------------------

async fn execute_route(State(state): State<AppState>, Authed(s): Authed, Json(req): Json<ExecuteRequest>) -> ApiResult<Json<serde_json::Value>> {
    state.write_limiter.check(&format!("user:{}", s.user_id)).map_err(ApiError::too_many)?;
    let ctx: ImportContext = require_importer(&state, &s).await?;
    let http = state.bot_http.clone().ok_or_else(ApiError::upstream)?;
    if state.import_jobs.running_on(s.guild_id) {
        return Err(ApiError::new(axum::http::StatusCode::CONFLICT, "activity__import_already_running"));
    }

    // Double vérification (I12).
    let new_universe = match (&ctx.universe, &req.new_universe) {
        (Some(u), _) => {
            if !name_confirmed(&u.name, &req.confirm_name) {
                return Err(ApiError::bad_request("activity__import_name_mismatch"));
            }
            None
        }
        (None, Some(n)) => {
            let name = valid_universe_name(&n.name).ok_or_else(|| ApiError::bad_request("create_universe__invalid_name"))?;
            if !name_confirmed(&name, &req.confirm_name) {
                return Err(ApiError::bad_request("activity__import_name_mismatch"));
            }
            Some((name, n.time_modifier))
        }
        (None, None) => return Err(ApiError::bad_request("activity__import_universe_required")),
    };

    // Plan recalculé ici, jamais repris du client.
    let inputs: PlanInputs = plan_inputs(&state, &s, &ctx, &req.plan).await?;
    let plan = compute_plan(&inputs).await?;
    if let Some(key) = blocking_reason(&plan) {
        return Err(ApiError::new(axum::http::StatusCode::CONFLICT, key));
    }

    let universe_id = ctx.universe.as_ref().map(|u| u.universe_id);
    let total = plan.discord_objects_to_create();
    let mut p = Provisioner::with_job(http, "map_import", universe_id, total).await?;
    let job_id = p.job_id().map(|i| i.to_hex()).unwrap_or_default();
    let outcome = Arc::new(Mutex::new(Outcome::Running));
    state.import_jobs.inner.lock().unwrap_or_else(|e| e.into_inner()).insert(job_id.clone(), JobEntry { guild_id: s.guild_id, progress: p.subscribe(), outcome: outcome.clone() });

    let exec = Execution {
        user_id: s.user_id,
        guild_id: s.guild_id,
        universe: ctx.universe,
        new_universe,
        setup: req.plan.setup,
        plan,
        map: inputs.map,
        zone: req.plan.zone.clone(),
        fail_after_places: None,
    };
    let (sessions, drafts, guild_id, user_id) = (state.sessions.clone(), state.import_drafts.clone(), s.guild_id, s.user_id);
    tokio::spawn(async move {
        let result = execute(&mut p, exec).await;
        let rolled_back = matches!(p.subscribe().borrow().phase, Phase::RolledBack);
        *outcome.lock().unwrap_or_else(|e| e.into_inner()) = match result {
            Ok(id) => {
                sessions.link_universe(guild_id, id);
                drafts.remove(guild_id, user_id);
                tracing::info!(guild = guild_id, universe = %id, "activity: import terminé");
                Outcome::Done { universe_id: id.to_hex() }
            }
            Err(error) => {
                tracing::warn!(guild = guild_id, error, "activity: import échoué");
                Outcome::Failed { error, rolled_back }
            }
        };
    });
    Ok(Json(serde_json::json!({ "job_id": job_id, "total": total })))
}

async fn job_route(State(state): State<AppState>, Authed(s): Authed, Path(id): Path<String>) -> ApiResult<Json<serde_json::Value>> {
    if id.len() != 24 || !id.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(ApiError::bad_request("activity__invalid_request"));
    }
    let jobs = state.import_jobs.inner.lock().unwrap_or_else(|e| e.into_inner());
    // Une tâche n'est visible que depuis son serveur.
    let job = jobs.get(&id).filter(|j| j.guild_id == s.guild_id).ok_or_else(|| ApiError::not_found("activity__import_job_not_found"))?;
    let progress = job.progress.borrow().clone();
    let outcome = job.outcome.lock().unwrap_or_else(|e| e.into_inner()).clone();
    Ok(Json(serde_json::json!({ "progress": progress, "outcome": outcome })))
}

pub fn routes() -> Router<AppState> {
    Router::new().route("/admin/import/execute", post(execute_route)).route("/admin/import/jobs/{id}", get(job_route))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_universe_name_must_be_typed_exactly() {
        assert!(name_confirmed("Moria", " Moria "));
        assert!(!name_confirmed("Moria", "moria"));
        assert!(!name_confirmed("Moria", "Mori"));
        assert!(!name_confirmed("  ", "  "));
    }

    #[test]
    fn removals_and_missing_servers_block_the_execution() {
        use crate::activity::map::import_plan::PlannedServer;
        use crate::activity::map::partition::{budget, ServerLoad};
        let server = |g: Option<u64>| PlannedServer { guild_id: g, budget: budget(ServerLoad::default(), 0, 0) };
        let mut plan = ImportPlan { places: vec![], roads: vec![], remove_places: vec![], remove_roads: vec![], servers: vec![server(Some(1))], cut_roads: 0, isolated: vec![] };
        assert_eq!(blocking_reason(&plan), None);
        plan.servers.push(server(None));
        assert_eq!(blocking_reason(&plan), Some("activity__import_servers_missing"));
        plan.servers.pop();
        plan.remove_roads.push(9);
        assert_eq!(blocking_reason(&plan), Some("activity__import_removal_not_ready"));
    }

    /// Import réel sur `VERSEENGINE_TEST_GUILD_ID` (qui ne doit être relié à aucun univers), à
    /// travers le routeur : un faux Discord pour les droits, le vrai pour les créations.
    mod discord_integration {
        use super::*;
        use crate::activity::discord_api::*;
        use crate::activity::permissions::{AccessLevel, ADMINISTRATOR};
        use crate::activity::session::{test_session, Session};
        use crate::test_support::{block_on, discord_http, required_env_u64};
        use axum::body::{to_bytes, Body};
        use axum::http::{Request, StatusCode};
        use serenity::all::{ChannelId, GuildId};
        use std::time::Duration;
        use tower::ServiceExt;

        struct AdminEverywhere;
        impl DiscordGateway for AdminEverywhere {
            fn exchange_code<'a>(&'a self, _c: &'a str) -> BoxFuture<'a, Result<TokenGrant, DiscordError>> {
                Box::pin(async { Err(DiscordError::Rejected) })
            }
            fn current_user<'a>(&'a self, _t: &'a str) -> BoxFuture<'a, Result<DiscordUser, DiscordError>> {
                Box::pin(async { Err(DiscordError::Rejected) })
            }
            fn activity_instance<'a>(&'a self, _i: &'a str) -> BoxFuture<'a, Result<ActivityInstance, DiscordError>> {
                Box::pin(async { Err(DiscordError::Rejected) })
            }
            fn member<'a>(&'a self, g: u64, _u: u64) -> BoxFuture<'a, Result<Option<MemberInfo>, DiscordError>> {
                Box::pin(async move { Ok(Some(MemberInfo { roles: vec![], guild_owner_id: 1, guild_roles: vec![(g, ADMINISTRATOR)] })) })
            }
            fn bot_guilds<'a>(&'a self, _l: usize) -> BoxFuture<'a, Result<Vec<GuildSummary>, DiscordError>> {
                Box::pin(async { Ok(vec![]) })
            }
            fn guild_counts<'a>(&'a self, _g: u64) -> BoxFuture<'a, Result<GuildCounts, DiscordError>> {
                Box::pin(async { Ok(GuildCounts { channels: 20, roles: 10 }) })
            }
        }

        async fn call(app: &Router, method: &str, uri: &str, token: &str, body: Body) -> (StatusCode, serde_json::Value) {
            let req = Request::builder().method(method).uri(uri).header("authorization", format!("Bearer {token}")).header("content-type", "application/json").body(body).unwrap();
            let res = app.clone().oneshot(req).await.unwrap();
            let st = res.status();
            let bytes = to_bytes(res.into_body(), 64 << 20).await.unwrap();
            (st, serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null))
        }

        async fn counts(http: &Http, guild: u64) -> (usize, usize) {
            (http.get_channels(GuildId::new(guild)).await.unwrap().len(), http.get_guild_roles(GuildId::new(guild)).await.unwrap().len())
        }

        fn setup() -> (Arc<Http>, u64, Router, AppState, String) {
            let http = discord_http();
            let guild = required_env_u64("VERSEENGINE_TEST_GUILD_ID");
            let state = AppState::new(Arc::new(AdminEverywhere), Duration::from_secs(600)).with_bot_http(http.clone());
            let token = state.sessions.create(Session { user_id: 42, guild_id: guild, universe_id: None, access: AccessLevel::Admin, ..test_session(42) });
            (http, guild, crate::activity::http::router(state.clone()), state, token)
        }

        const PLAN: &str = r#"{"mode":"replace","setup":"PartialSetup"}"#;

        #[test]
        #[ignore = "integration test: needs MongoDB, DISCORD_TOKEN and VERSEENGINE_TEST_GUILD_ID; run with --ignored"]
        fn a_map_is_imported_into_a_new_universe_on_discord() {
            block_on(async {
                let _ = crate::database::db_client::get_db_client().await;
                let (http, guild, app, _state, token) = setup();
                let before = counts(&http, guild).await;
                let map = crate::activity::map::azgaar_file::tests::sample_map_text();
                let (st, _) = call(&app, "POST", "/api/admin/import/map", &token, Body::from(map)).await;
                assert_eq!(st, StatusCode::OK);

                // Mauvais nom : refusé, rien n'est créé.
                tokio::time::sleep(Duration::from_millis(1100)).await;
                let bad = format!(r#"{{"plan":{PLAN},"confirm_name":"Autre","new_universe":{{"name":"Test import"}}}}"#);
                let (st, body) = call(&app, "POST", "/api/admin/import/execute", &token, Body::from(bad)).await;
                assert_eq!((st, body["error"].as_str()), (StatusCode::BAD_REQUEST, Some("activity__import_name_mismatch")));
                assert_eq!(counts(&http, guild).await, before);

                tokio::time::sleep(Duration::from_millis(1100)).await;
                let good = format!(r#"{{"plan":{PLAN},"confirm_name":"Test import","new_universe":{{"name":"Test import","time_modifier":300}}}}"#);
                let (st, body) = call(&app, "POST", "/api/admin/import/execute", &token, Body::from(good)).await;
                assert_eq!(st, StatusCode::OK, "{body}");
                let job = body["job_id"].as_str().unwrap().to_string();

                let mut last = serde_json::Value::Null;
                for _ in 0..240 {
                    tokio::time::sleep(Duration::from_millis(500)).await;
                    let (_, b) = call(&app, "GET", &format!("/api/admin/import/jobs/{job}"), &token, Body::empty()).await;
                    last = b;
                    if last["outcome"]["status"] != "running" {
                        break;
                    }
                }
                assert_eq!(last["outcome"]["status"], "done", "{last}");
                assert_eq!(last["progress"]["phase"], "Done");
                let universe_id = ObjectId::parse_str(last["outcome"]["universe_id"].as_str().unwrap()).unwrap();

                let universe = crate::database::universe::get_universe_by_id(universe_id).await.unwrap().unwrap();
                assert_eq!((universe.name.as_str(), universe.global_time_modifier, universe.creator_id), ("Test import", 300, 42));
                use futures::TryStreamExt;
                let places: Vec<Place> = crate::database::places::get_places_by_universe_id(universe_id).await.unwrap().try_collect().await.unwrap();
                let names: Vec<&str> = places.iter().map(|p| p.name.as_str()).collect();
                assert!(names.contains(&"Harwick") && names.contains(&"Westham"), "{names:?}");
                for pl in &places {
                    let c = http.get_channel(ChannelId::new(pl.central_channel_id.unwrap())).await.unwrap().guild().unwrap();
                    assert_eq!(c.parent_id.map(|x| x.get()), Some(pl.category_id));
                }
                let roads: Vec<crate::database::road::Road> = crate::database::road::get_roads_by_universe(universe_id).await.unwrap().try_collect().await.unwrap();
                assert_eq!(roads.len(), 1);
                assert!(http.get_channel(ChannelId::new(roads[0].channel_id)).await.is_ok());
                let stored = crate::database::universe_maps::get_universe_map(universe_id).await.unwrap().unwrap();
                assert_eq!(stored.source, MapSource::Azgaar);
                assert_eq!(stored.positions.len(), 2);
                assert!(stored.positions.iter().all(|p| p.azgaar_burg_id.is_some()), "burg ids kept for a reimport");
                assert_eq!(stored.road_paths.len(), 1);
                // La session est maintenant reliée à l'univers.
                let (_, me) = call(&app, "GET", "/api/me", &token, Body::empty()).await;
                assert_eq!(me["universe_id"], universe_id.to_hex());

                crate::test_support::delete_universe_everywhere(&http, universe_id).await;
                let job_id = ObjectId::parse_str(&job).unwrap();
                crate::database::db_client::get_db_client().await.database(VERSEENGINE_DB_NAME).collection::<mongodb::bson::Document>("provisioning_jobs").delete_one(doc! {"_id": job_id}).await.unwrap();
                assert_eq!(counts(&http, guild).await, before, "everything is cleaned up");
            });
        }

        /// Un échec au milieu (après le premier lieu) défait tout : univers, serveur, rôles et
        /// salons, documents.
        #[test]
        #[ignore = "integration test: needs MongoDB, DISCORD_TOKEN and VERSEENGINE_TEST_GUILD_ID; run with --ignored"]
        fn a_failure_in_the_middle_of_an_import_rolls_everything_back() {
            block_on(async {
                let _ = crate::database::db_client::get_db_client().await;
                let (http, guild, _app, _state, _token) = setup();
                let before = counts(&http, guild).await;
                let map = crate::activity::map::azgaar_file::parse_map_file(crate::activity::map::azgaar_file::tests::sample_map_text().as_bytes()).unwrap().map;
                let options = crate::activity::map::import_plan::ImportOptions {
                    servers: vec![crate::activity::map::import_plan::KnownServer { guild_id: guild, load: Default::default() }],
                    ..Default::default()
                };
                let plan = crate::activity::map::import_plan::build_plan(&map, &options, &[], &[]).unwrap();
                let mut p = Provisioner::with_job(http.clone(), "test_import", None, plan.discord_objects_to_create()).await.unwrap();
                let job = p.job_id().unwrap();
                let exec = Execution {
                    user_id: 42,
                    guild_id: guild,
                    universe: None,
                    new_universe: Some(("Test rollback".into(), None)),
                    setup: SetupType::PartialSetup,
                    plan,
                    map: Arc::new(map),
                    zone: vec![],
                    fail_after_places: Some(1),
                };
                let err = execute(&mut p, exec).await.unwrap_err();
                assert_eq!(err, "activity__internal_error");
                assert_eq!(p.subscribe().borrow().phase, Phase::RolledBack);
                assert_eq!(counts(&http, guild).await, before, "nothing left on Discord");
                assert!(get_server_by_id(guild).await.unwrap().is_none(), "the server is not linked any more");
                let db = crate::database::db_client::get_db_client().await.database(VERSEENGINE_DB_NAME);
                let universes = db.collection::<mongodb::bson::Document>("universes").count_documents(doc! {"name": "Test rollback", "creator_id": "42"}).await.unwrap();
                assert_eq!(universes, 0);
                db.collection::<mongodb::bson::Document>("provisioning_jobs").delete_one(doc! {"_id": job}).await.unwrap();
            });
        }
    }
}
