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
use crate::activity::import_removal::{apply_in_session, apply_on_discord, channels_of_places, load_removal, validate_choices, DiscordLeftovers, Removal, RemovalChoices};
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
    /// Brouillard sur la carte des joueurs (I17).
    #[serde(default)]
    pub fog: bool,
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
    /// Lieux existants supprimés (I15, ré-import) : lieu de départ et sort des objets posés.
    #[serde(default)]
    pub removal: RemovalChoices,
}

/// I12 : le nom saisi doit être exactement celui de l'univers (espaces de bord ignorés).
pub fn name_confirmed(expected: &str, typed: &str) -> bool {
    !expected.trim().is_empty() && expected.trim() == typed.trim()
}

/// Ce qui manque pour exécuter le plan : clé d'erreur, ou `None` (Q11).
pub fn blocking_reason(plan: &ImportPlan) -> Option<&'static str> {
    (plan.servers_to_create() > 0).then_some("activity__import_servers_missing")
}

/// Le plan supprime-t-il de l'existant ?
pub fn removes_something(plan: &ImportPlan) -> bool {
    !plan.remove_places.is_empty() || !plan.remove_roads.is_empty()
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
    pub new_universe: Option<(String, Option<u32>, bool)>,
    pub setup: SetupType,
    pub plan: ImportPlan,
    pub map: Arc<AzgaarMap>,
    /// Carte générée par l'activité : enregistrée comme `Manual`, sans relief.
    pub generated: bool,
    pub zone: Vec<[f64; 2]>,
    /// Lieux et routes à supprimer, avec les choix de l'admin (vérifiés).
    pub removal: Option<(Removal, RemovalChoices)>,
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
        (None, Some((name, time_modifier, fog))) => {
            let locale = guild_locale(&http, exec.guild_id).await;
            let new = NewUniverse { name, creator_id: exec.user_id, guild_id: exec.guild_id, setup: exec.setup, time_modifier: *time_modifier, locale: &locale, fog: *fog };
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

    // 6. Mises à jour, carte et suppressions, en une transaction (point de non-retour).
    let mut map = universe_map(universe_id, &exec.map, plan, &place_of_burg, &exec.zone);
    if exec.generated {
        map.source = MapSource::Manual;
    }
    let removal = match &exec.removal {
        Some((removal, choices)) => {
            let start = choices.start_burg.and_then(|b| place_of_burg.get(&b)).cloned();
            if start.is_none() && removal.info.characters > 0 {
                return Err("activity__import_start_place_required");
            }
            let channels = channels_of_places(&http, &removal.places).await;
            Some((removal, choices, start, channels))
        }
        None => None,
    };
    // Les voyages annulés sortent de la file en mémoire avant l'écriture (sinon le minuteur
    // pourrait réécrire un groupe déjà déplacé) ; ils y reviennent si l'écriture échoue.
    let paused = match &removal {
        Some((r, ..)) => pause_moves(&r.groups).await,
        None => vec![],
    };
    let saved = save_in_transaction(universe_id, &updates, &map, removal.as_ref().map(|(r, c, s, ch)| (*r, *c, s.as_ref(), ch.as_slice()))).await;
    if saved.is_err() {
        for g in paused {
            crate::travel::logic::add_move(g).await;
        }
        return Err("activity__internal_error");
    }
    if let Some((r, _, start, channels)) = &removal {
        let left = apply_on_discord(&http, r, start.as_ref(), channels).await;
        if left != DiscordLeftovers::default() {
            eprintln!("[import] universe {universe_id}: left on Discord after the removal: {left:?}");
        }
    }
    Ok(universe_id)
}

/// Retire de la file des voyages les groupes donnés ; retourne ceux qui y étaient.
async fn pause_moves(groups: &[crate::database::travel::TravelGroup]) -> Vec<crate::database::travel::TravelGroup> {
    let ids: std::collections::HashSet<ObjectId> = groups.iter().map(|g| g._id).collect();
    let paused: Vec<_> = crate::travel::logic::MOVES.lock().await.iter().filter(|m| ids.contains(&m._id)).cloned().collect();
    for g in &paused {
        if let Some(&first) = g.members.first() {
            crate::travel::logic::remove_move(first).await;
        }
    }
    paused
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

async fn save_in_transaction(
    universe_id: ObjectId,
    updates: &[(u64, u64, bool)],
    map: &UniverseMap,
    removal: Option<(&Removal, &RemovalChoices, Option<&Place>, &[u64])>,
) -> mongodb::error::Result<()> {
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
        if let Some((removal, choices, start, channels)) = removal {
            apply_in_session(&mut session, universe_id, removal, start, choices, channels).await?;
        }
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
            Some((name, n.time_modifier, n.fog))
        }
        (None, None) => return Err(ApiError::bad_request("activity__import_universe_required")),
    };

    // Plan recalculé ici, jamais repris du client.
    let inputs: PlanInputs = plan_inputs(&state, &s, &ctx, &req.plan).await?;
    let plan = compute_plan(&inputs).await?;
    if let Some(key) = blocking_reason(&plan) {
        return Err(ApiError::new(axum::http::StatusCode::CONFLICT, key));
    }
    // Suppressions : lues en base ici, et choix de l'admin vérifiés (I15, Q2).
    let removal = match (&ctx.universe, removes_something(&plan)) {
        (Some(u), true) => {
            let removal = load_removal(u.universe_id, req.plan.mode, &plan).await?;
            validate_choices(&removal.info, &req.removal, &plan).map_err(ApiError::bad_request)?;
            Some((removal, req.removal.clone()))
        }
        _ => None,
    };

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
        generated: inputs.generated,
        zone: req.plan.zone.clone(),
        removal,
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

    /// Carte Moria telle qu'un joueur la reçoit sous brouillard, après deux routes depuis la
    /// capitale la mieux reliée, écrite dans `IMPORT_FIXTURE_DIR/fog-moria.json` pour
    /// `web/activity/fog-harness.html`.
    #[test]
    #[ignore = "integration test: needs AZGAAR_MAP_FILE and IMPORT_FIXTURE_DIR"]
    fn a_real_map_fog_fixture_is_written() {
        use crate::activity::fog::{apply_fog, Discovery};
        use crate::database::road::Road;
        use crate::activity::map::import_plan::{build_plan, ImportOptions};
        let path = std::env::var("AZGAAR_MAP_FILE").expect("set AZGAAR_MAP_FILE");
        let dir = std::path::PathBuf::from(std::env::var("IMPORT_FIXTURE_DIR").expect("set IMPORT_FIXTURE_DIR"));
        let f = crate::activity::map::azgaar_file::parse_map_file(&std::fs::read(&path).unwrap()).unwrap();
        let plan = build_plan(&f.map, &ImportOptions::default(), &[], &[]).unwrap();
        let universe_id = ObjectId::new();
        // Identifiants Discord fictifs : catégorie = identifiant du bourg, salon = 10⁶ + rang.
        let place_of_burg: HashMap<u64, Place> = plan
            .places
            .iter()
            .map(|p| (p.burg_id, Place { _id: ObjectId::new(), universe_id, server_id: 1, category_id: p.burg_id, role: 0, name: p.name.clone(), modifiers: vec![], central_channel_id: None }))
            .collect();
        let roads: Vec<Road> = plan
            .roads
            .iter()
            .enumerate()
            .map(|(i, r)| Road {
                _id: ObjectId::new(),
                universe_id,
                server_id: 1,
                server_two_id: None,
                road_name: String::new(),
                role_id: None,
                channel_id: 1_000_000 + i as u64,
                place_one_id: r.burg_a,
                place_two_id: r.burg_b,
                distance: r.distance,
                secret: false,
                modifiers: vec![],
            })
            .collect();
        let places: Vec<Place> = place_of_burg.values().cloned().collect();
        let map = universe_map(universe_id, &f.map, &plan, &place_of_burg, &[]);
        let all: std::collections::HashSet<u64> = roads.iter().map(|r| r.channel_id).collect();
        let dto = crate::activity::map_view::build_map_dto(&map, &places, &roads, &all, &[], 0);
        let degree = |b: u64| roads.iter().filter(|r| r.place_one_id == b || r.place_two_id == b).count();
        let other = |r: &Road, b: u64| if r.place_one_id == b { r.place_two_id } else { r.place_one_id };
        let start = plan.places.iter().filter(|p| p.capital).max_by_key(|p| degree(p.burg_id)).unwrap().burg_id;
        let r1 = roads.iter().find(|r| r.place_one_id == start || r.place_two_id == start).unwrap();
        let mid = other(r1, start);
        let r2 = roads.iter().filter(|r| r.channel_id != r1.channel_id).find(|r| (r.place_one_id == mid || r.place_two_id == mid) && other(r, mid) != start).unwrap();
        let here = other(r2, mid);
        let d = Discovery { here: Some(here), known_roads: [r1.channel_id, r2.channel_id].into_iter().collect(), ..Default::default() };
        let out = apply_fog(dto, &d);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("fog-moria.json"), serde_json::to_string(&out).unwrap()).unwrap();
    }

    #[test]
    fn the_universe_name_must_be_typed_exactly() {
        assert!(name_confirmed("Moria", " Moria "));
        assert!(!name_confirmed("Moria", "moria"));
        assert!(!name_confirmed("Moria", "Mori"));
        assert!(!name_confirmed("  ", "  "));
    }

    #[test]
    fn missing_servers_block_the_execution() {
        use crate::activity::map::import_plan::PlannedServer;
        use crate::activity::map::partition::{budget, ServerLoad};
        let server = |g: Option<u64>| PlannedServer { guild_id: g, budget: budget(ServerLoad::default(), 0, 0) };
        let mut plan = ImportPlan { places: vec![], roads: vec![], remove_places: vec![], remove_roads: vec![], servers: vec![server(Some(1))], cut_roads: 0, isolated: vec![] };
        assert_eq!(blocking_reason(&plan), None);
        plan.servers.push(server(None));
        assert_eq!(blocking_reason(&plan), Some("activity__import_servers_missing"));
        plan.servers.pop();
        plan.remove_roads.push(9);
        assert_eq!(blocking_reason(&plan), None, "removals are handled (E6)");
        assert!(removes_something(&plan));
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

        async fn wait_job(app: &Router, token: &str, job: &str) -> serde_json::Value {
            let mut last = serde_json::Value::Null;
            for _ in 0..240 {
                tokio::time::sleep(Duration::from_millis(500)).await;
                let (_, b) = call(app, "GET", &format!("/api/admin/import/jobs/{job}"), token, Body::empty()).await;
                last = b;
                if last["outcome"]["status"] != "running" {
                    break;
                }
            }
            last
        }

        /// E6 : un second import « Supprimer les lieux existants » déplace les joueurs vers le
        /// lieu de départ, rend l'objet posé à son propriétaire avec son contenu, détruit l'objet
        /// sans propriétaire (après confirmation, Q2) et supprime les anciens lieux et routes de
        /// Discord et de la base.
        #[test]
        #[ignore = "integration test: needs MongoDB, DISCORD_TOKEN and VERSEENGINE_TEST_GUILD_ID; run with --ignored"]
        fn replacing_the_existing_places_moves_players_and_settles_placed_items_on_discord() {
            use crate::database::inventory::{HolderType, Inventory};
            use crate::database::travel::{SpaceType, TravelGroup};
            use futures::TryStreamExt;
            block_on(async {
                let _ = crate::database::db_client::get_db_client().await;
                let (http, guild, app, _state, token) = setup();
                let before = counts(&http, guild).await;
                let owner_user = http.get_guild(GuildId::new(guild)).await.unwrap().owner_id.get();
                let map = || Body::from(crate::activity::map::azgaar_file::tests::sample_map_text());

                // 1. Premier import.
                assert_eq!(call(&app, "POST", "/api/admin/import/map", &token, map()).await.0, StatusCode::OK);
                tokio::time::sleep(Duration::from_millis(1100)).await;
                let first = format!(r#"{{"plan":{PLAN},"confirm_name":"Test E6","new_universe":{{"name":"Test E6"}}}}"#);
                let (st, body) = call(&app, "POST", "/api/admin/import/execute", &token, Body::from(first)).await;
                assert_eq!(st, StatusCode::OK, "{body}");
                let done = wait_job(&app, &token, body["job_id"].as_str().unwrap()).await;
                assert_eq!(done["outcome"]["status"], "done", "{done}");
                let universe_id = ObjectId::parse_str(done["outcome"]["universe_id"].as_str().unwrap()).unwrap();
                let old_places: Vec<Place> = crate::database::places::get_places_by_universe_id(universe_id).await.unwrap().try_collect().await.unwrap();
                let old_roads: Vec<crate::database::road::Road> = crate::database::road::get_roads_by_universe(universe_id).await.unwrap().try_collect().await.unwrap();
                let harwick = old_places.iter().find(|p| p.name == "Harwick").unwrap().clone();

                // 2. Un joueur à Harwick, un objet posé à lui (avec du contenu), un sans propriétaire.
                let db = crate::database::db_client::get_db_client().await.database(VERSEENGINE_DB_NAME);
                let character = crate::database::characters::Character { _id: ObjectId::new(), user_id: owner_user, universe_id, name: "Testeur".into(), stats: vec![], description: None, image: None };
                db.collection::<crate::database::characters::Character>("characters").insert_one(&character).await.unwrap();
                TravelGroup { _id: ObjectId::new(), universe_id, actual_space_id: harwick.category_id, actual_space_type: SpaceType::Place, members: vec![owner_user], server_id: guild, ..Default::default() }
                    .insert().await.unwrap();
                let (chest_item, coin) = (ObjectId::new(), ObjectId::new());
                let tool = |owner: Option<ObjectId>| crate::database::tool::Tool {
                    _id: Some(ObjectId::new()), universe_id, server_id: guild, owner_id: owner, category_id: harwick.category_id,
                    channel_id: harwick.central_channel_id.unwrap(), area_id: None, original_item: chest_item, name: "Coffre".into(),
                    chained: None, inventory_id: None, inventory_size: 5,
                };
                let (owned, unowned) = (tool(Some(character._id)), tool(None));
                owned.clone().save().await.unwrap();
                unowned.clone().save().await.unwrap();
                Inventory::add_item_to_inventory(universe_id, owned._id.unwrap(), HolderType::Item, coin, 2).await.unwrap();
                Inventory::add_item_to_inventory(universe_id, unowned._id.unwrap(), HolderType::Item, coin, 7).await.unwrap();

                // 3. Second import, en remplaçant : l'aperçu annonce ce qui sera touché.
                tokio::time::sleep(Duration::from_millis(1100)).await;
                assert_eq!(call(&app, "POST", "/api/admin/import/map", &token, map()).await.0, StatusCode::OK);
                tokio::time::sleep(Duration::from_millis(1100)).await;
                let (st, preview) = call(&app, "POST", "/api/admin/import/plan", &token, Body::from(PLAN)).await;
                assert_eq!(st, StatusCode::OK, "{preview}");
                assert_eq!(preview["removal"]["characters"], 1);
                assert_eq!(preview["removal"]["items"].as_array().unwrap().len(), 2);
                assert!(preview["removal"]["items"].as_array().unwrap().iter().any(|i| i["owner"] == "Testeur" && i["content"] == 2));

                // Sans confirmation de la destruction de l'objet sans propriétaire : refusé (Q2).
                tokio::time::sleep(Duration::from_millis(1100)).await;
                let refused = format!(r#"{{"plan":{PLAN},"confirm_name":"Test E6","removal":{{"start_burg":1}}}}"#);
                let (st, body) = call(&app, "POST", "/api/admin/import/execute", &token, Body::from(refused)).await;
                assert_eq!((st, body["error"].as_str()), (StatusCode::BAD_REQUEST, Some("activity__import_confirm_unowned")));

                tokio::time::sleep(Duration::from_millis(1100)).await;
                let ok = format!(r#"{{"plan":{PLAN},"confirm_name":"Test E6","removal":{{"start_burg":2,"confirm_destroy_unowned":true}}}}"#);
                let (st, body) = call(&app, "POST", "/api/admin/import/execute", &token, Body::from(ok)).await;
                assert_eq!(st, StatusCode::OK, "{body}");
                let done = wait_job(&app, &token, body["job_id"].as_str().unwrap()).await;
                assert_eq!(done["outcome"]["status"], "done", "{done}");

                // 4. Vérifications.
                let places: Vec<Place> = crate::database::places::get_places_by_universe_id(universe_id).await.unwrap().try_collect().await.unwrap();
                assert_eq!(places.len(), 2, "only the new places remain");
                assert!(places.iter().all(|p| !old_places.iter().any(|o| o.category_id == p.category_id)));
                for o in &old_places {
                    assert!(http.get_channel(ChannelId::new(o.category_id)).await.is_err(), "old category deleted");
                    assert!(http.get_channel(ChannelId::new(o.central_channel_id.unwrap())).await.is_err(), "old channel deleted");
                }
                for r in &old_roads {
                    assert!(http.get_channel(ChannelId::new(r.channel_id)).await.is_err(), "old road deleted");
                }
                let roles = http.get_guild_roles(GuildId::new(guild)).await.unwrap();
                assert!(old_places.iter().all(|o| !roles.iter().any(|r| r.id.get() == o.role)), "old roles deleted");
                // Lieu de départ : le bourg 2 (Westham) de la nouvelle carte.
                let westham = places.iter().find(|p| p.name == "Westham").unwrap();
                let group = crate::database::travel::get_group_of_user(universe_id, owner_user).await.unwrap().unwrap();
                assert_eq!((group.actual_space_type, group.actual_space_id), (SpaceType::Place, westham.category_id));
                let member = http.get_member(GuildId::new(guild), serenity::all::UserId::new(owner_user)).await.unwrap();
                assert!(member.roles.iter().any(|r| r.get() == westham.role), "the start place role is given");
                // Objets : rendu avec son contenu, l'autre détruit.
                let inv = Inventory::get_by_holder(universe_id, character._id, HolderType::Character).await.unwrap();
                let qty = |item: ObjectId| inv.iter().filter(|i| i.item_id == item).map(|i| i.quantity).sum::<u64>();
                assert_eq!((qty(chest_item), qty(coin)), (1, 2));
                assert_eq!(db.collection::<mongodb::bson::Document>("placed_items").count_documents(doc! {"universe_id": universe_id}).await.unwrap(), 0);
                assert_eq!(db.collection::<mongodb::bson::Document>("inventory").count_documents(doc! {"universe_id": universe_id, "holder.holder_type": "Item"}).await.unwrap(), 0);

                crate::test_support::delete_universe_everywhere(&http, universe_id).await;
                for name in ["characters", "travels", "inventory", "provisioning_jobs"] {
                    db.collection::<mongodb::bson::Document>(name).delete_many(doc! {"universe_id": universe_id}).await.unwrap();
                }
                db.collection::<mongodb::bson::Document>("provisioning_jobs").delete_many(doc! {"kind": "map_import", "universe_id": null}).await.unwrap();
                assert_eq!(counts(&http, guild).await, before, "everything is cleaned up");
            });
        }

        /// E7 : une carte générée de 3 lieux est créée sur Discord ; elle est enregistrée comme
        /// `Manual`, sans relief (l'activité dessine son terrain 3D).
        #[test]
        #[ignore = "integration test: needs MongoDB, DISCORD_TOKEN and VERSEENGINE_TEST_GUILD_ID; run with --ignored"]
        fn a_generated_map_is_created_on_discord() {
            use futures::TryStreamExt;
            block_on(async {
                let _ = crate::database::db_client::get_db_client().await;
                let (http, guild, app, _state, token) = setup();
                let before = counts(&http, guild).await;
                let (st, map) = call(&app, "POST", "/api/admin/import/generate", &token, Body::from(r#"{"seed":77,"count":3}"#)).await;
                assert_eq!(st, StatusCode::OK, "{map}");
                tokio::time::sleep(Duration::from_millis(1100)).await;
                let (st, _) = call(&app, "POST", "/api/admin/import/generated", &token, Body::from(map.to_string())).await;
                assert_eq!(st, StatusCode::OK);
                tokio::time::sleep(Duration::from_millis(1100)).await;
                let req = format!(r#"{{"plan":{PLAN},"confirm_name":"Test généré","new_universe":{{"name":"Test généré"}}}}"#);
                let (st, body) = call(&app, "POST", "/api/admin/import/execute", &token, Body::from(req)).await;
                assert_eq!(st, StatusCode::OK, "{body}");
                let done = wait_job(&app, &token, body["job_id"].as_str().unwrap()).await;
                assert_eq!(done["outcome"]["status"], "done", "{done}");
                let universe_id = ObjectId::parse_str(done["outcome"]["universe_id"].as_str().unwrap()).unwrap();
                let places: Vec<Place> = crate::database::places::get_places_by_universe_id(universe_id).await.unwrap().try_collect().await.unwrap();
                assert_eq!(places.len(), 3);
                let expected: Vec<String> = map["places"].as_array().unwrap().iter().map(|p| p["name"].as_str().unwrap().to_string()).collect();
                assert!(places.iter().all(|p| expected.contains(&p.name)));
                let roads: Vec<crate::database::road::Road> = crate::database::road::get_roads_by_universe(universe_id).await.unwrap().try_collect().await.unwrap();
                assert_eq!(roads.len(), map["roads"].as_array().unwrap().len());
                assert!(roads.iter().all(|r| r.distance >= 1));
                let stored = crate::database::universe_maps::get_universe_map(universe_id).await.unwrap().unwrap();
                assert_eq!(stored.source, MapSource::Manual);
                assert!(stored.terrain.is_none());
                assert_eq!(stored.road_paths.len(), roads.len());

                crate::test_support::delete_universe_everywhere(&http, universe_id).await;
                crate::database::db_client::get_db_client().await.database(VERSEENGINE_DB_NAME).collection::<mongodb::bson::Document>("provisioning_jobs").delete_many(doc! {"kind": "map_import", "universe_id": null}).await.unwrap();
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
                    new_universe: Some(("Test rollback".into(), None, false)),
                    setup: SetupType::PartialSetup,
                    plan,
                    map: Arc::new(map),
                    generated: false,
                    zone: vec![],
                    removal: None,
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
