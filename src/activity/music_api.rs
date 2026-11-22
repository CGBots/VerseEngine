//! API de la musique (`docs/activity/MUSIC_SPEC.md`, étape S2).
//!
//! - `GET /api/music/here` : playlist de l'endroit où se trouve le joueur (lieu, ou route en voyage).
//! - `GET /api/music/tracks/{id}` : fichier d'un morceau de l'univers de la session.
//! - `/api/admin/music*` : bibliothèque, envoi, titre et assignations, suppression. Droits
//!   d'administrateur **revérifiés auprès de Discord** à chaque requête (jamais la session seule).

use std::collections::HashSet;
use std::sync::{Arc, Mutex};

use axum::body::Bytes;
use axum::extract::{DefaultBodyLimit, Path, Query, State};
use axum::http::{header, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post, put};
use axum::{Json, Router};
use futures::TryStreamExt;
use mongodb::bson::oid::ObjectId;
use serde::{Deserialize, Serialize};

use crate::activity::error::{ApiError, ApiResult};
use crate::activity::events::{Event, EventKind};
use crate::activity::http::{AppState, Authed};
use crate::activity::permissions::AccessLevel;
use crate::activity::session::Session;
use crate::database::music::{self, MusicTrack, Spot, TrackOrigin};
use crate::database::server::get_server_by_id;
use crate::database::travel::{get_group_of_user, SpaceType};
use crate::music::audio::{check_audio, MAX_FILE_BYTES};

/// Univers dont un envoi (fichier ou YouTube) est en cours : un seul à la fois par univers
/// (quota, décodage coûteux) ; et les imports YouTube suivis par l'activité.
#[derive(Default)]
pub struct MusicUploads {
    busy: Mutex<HashSet<ObjectId>>,
    jobs: Mutex<std::collections::HashMap<String, YoutubeJob>>,
}

/// Réserve l'univers le temps d'un envoi ; libéré à la fin, même en cas d'erreur.
pub struct UploadSlot(Arc<MusicUploads>, ObjectId);

impl MusicUploads {
    pub fn reserve(self: &Arc<Self>, universe_id: ObjectId) -> ApiResult<UploadSlot> {
        let mut busy = self.busy.lock().unwrap_or_else(|p| p.into_inner());
        if !busy.insert(universe_id) {
            return Err(ApiError::new(StatusCode::CONFLICT, "activity__music_upload_in_progress"));
        }
        Ok(UploadSlot(self.clone(), universe_id))
    }
}

impl Drop for UploadSlot {
    fn drop(&mut self) {
        self.0.busy.lock().unwrap_or_else(|p| p.into_inner()).remove(&self.1);
    }
}

/// Import YouTube en cours ou terminé (gardé 15 min pour que l'activité lise le résultat).
#[derive(Clone)]
pub struct YoutubeJob {
    universe_id: ObjectId,
    created: std::time::Instant,
    status: YoutubeStatus,
}

#[derive(Clone, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum YoutubeStatus {
    Running,
    Done { track: AdminTrackDto },
    Failed { error: String },
}

const JOB_TTL: std::time::Duration = std::time::Duration::from_secs(15 * 60);

impl MusicUploads {
    fn job_put(&self, id: &str, job: YoutubeJob) {
        let mut jobs = self.jobs.lock().unwrap_or_else(|p| p.into_inner());
        jobs.retain(|_, j| j.created.elapsed() < JOB_TTL);
        jobs.insert(id.to_string(), job);
    }

    fn job_set(&self, id: &str, status: YoutubeStatus) {
        if let Some(j) = self.jobs.lock().unwrap_or_else(|p| p.into_inner()).get_mut(id) {
            j.status = status;
        }
    }

    fn job_get(&self, id: &str, universe_id: ObjectId) -> Option<YoutubeStatus> {
        let jobs = self.jobs.lock().unwrap_or_else(|p| p.into_inner());
        jobs.get(id).filter(|j| j.universe_id == universe_id && j.created.elapsed() < JOB_TTL).map(|j| j.status.clone())
    }
}

/// Morceau tel que le voit un joueur.
#[derive(Serialize, Debug, PartialEq)]
pub struct TrackDto {
    pub id: String,
    pub title: String,
    pub duration_ms: u64,
}

impl From<&MusicTrack> for TrackDto {
    fn from(t: &MusicTrack) -> Self {
        Self { id: t.id.to_hex(), title: t.title.clone(), duration_ms: t.duration_ms }
    }
}

#[derive(Serialize, Debug, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SpotDto {
    Place { id: String },
    Road { id: String },
}

#[derive(Serialize)]
pub struct HereDto {
    /// `None` : le joueur n'a pas de position (pas encore de personnage placé).
    pub spot: Option<SpotDto>,
    pub tracks: Vec<TrackDto>,
}

/// Endroit d'un groupe de voyage : le lieu où il est, ou la route qu'il parcourt.
pub fn spot_of(space_type: &SpaceType, space_id: u64) -> Spot {
    match space_type {
        SpaceType::Place => Spot::Place(space_id),
        SpaceType::Road => Spot::Road(space_id),
    }
}

async fn here(Authed(s): Authed) -> ApiResult<Json<HereDto>> {
    let universe_id = s.universe()?;
    let Some(group) = get_group_of_user(universe_id, s.user_id).await? else {
        return Ok(Json(HereDto { spot: None, tracks: vec![] }));
    };
    let spot = spot_of(&group.actual_space_type, group.actual_space_id);
    let tracks = music::playlist(universe_id, spot).await?;
    let spot = match spot {
        Spot::Place(id) => SpotDto::Place { id: id.to_string() },
        Spot::Road(id) => SpotDto::Road { id: id.to_string() },
    };
    Ok(Json(HereDto { spot: Some(spot), tracks: tracks.iter().map(TrackDto::from).collect() }))
}

fn parse_id(id: &str) -> ApiResult<ObjectId> {
    ObjectId::parse_str(id).map_err(|_| ApiError::not_found("activity__music_not_found"))
}

/// Type servi : seulement ceux que le contrôle du fichier peut produire.
fn served_mime(mime: &str) -> Option<&'static str> {
    ["audio/mpeg", "audio/ogg", "audio/flac", "audio/wav", "audio/mp4", "audio/webm"].into_iter().find(|m| *m == mime)
}

/// Fichier d'un morceau. Servi comme l'image de fond de carte : type imposé, `nosniff` (en-tête
/// global), `sandbox` (même ouvert directement, rien ne s'exécute), sans nom de fichier.
async fn track_file(Authed(s): Authed, Path(id): Path<String>) -> ApiResult<Response> {
    let track = music::get_track(s.universe()?, parse_id(&id)?).await?.ok_or_else(|| ApiError::not_found("activity__music_not_found"))?;
    let mime = served_mime(&track.mime).ok_or_else(ApiError::internal)?;
    let bytes = music::read_track_file(track.file_id).await?;
    let mut res = bytes.into_response();
    let h = res.headers_mut();
    h.insert(header::CONTENT_TYPE, HeaderValue::from_static(mime));
    h.insert(header::CONTENT_DISPOSITION, HeaderValue::from_static("inline"));
    h.insert(header::CACHE_CONTROL, HeaderValue::from_static("private, max-age=86400, immutable"));
    h.insert(header::CONTENT_SECURITY_POLICY, HeaderValue::from_static("sandbox; default-src 'none'"));
    Ok(res)
}

// ---------------------------------------------------------------------------
// Admin
// ---------------------------------------------------------------------------

/// Administrateur du serveur de la session, revérifié auprès de Discord ; renvoie l'univers.
pub async fn require_music_admin(state: &AppState, s: &Session) -> ApiResult<ObjectId> {
    let universe_id = s.universe()?;
    let server = get_server_by_id(s.guild_id).await?;
    if server.as_ref().map(|sv| sv.universe_id) != Some(universe_id) {
        return Err(ApiError::forbidden("activity__music_forbidden"));
    }
    match crate::activity::admin_import::access_on(state.discord.as_ref(), s.guild_id, s.user_id, server.as_ref()).await? {
        Some(AccessLevel::Admin) => Ok(universe_id),
        _ => Err(ApiError::forbidden("activity__music_forbidden")),
    }
}

fn check_write(state: &AppState, s: &Session) -> ApiResult<()> {
    state.write_limiter.check(&format!("user:{}", s.user_id)).map_err(ApiError::too_many)
}

fn publish(state: &AppState, universe_id: ObjectId) {
    state.hub.publish(Event { kind: EventKind::MusicUpdated, universe_id: Some(universe_id), users: vec![] });
}

#[derive(Serialize, Clone)]
pub struct AdminTrackDto {
    pub id: String,
    pub title: String,
    pub duration_ms: u64,
    pub bytes: u64,
    /// `file` ou `youtube`.
    pub origin: &'static str,
    pub places: Vec<String>,
    pub roads: Vec<String>,
}

impl From<&MusicTrack> for AdminTrackDto {
    fn from(t: &MusicTrack) -> Self {
        Self {
            id: t.id.to_hex(),
            title: t.title.clone(),
            duration_ms: t.duration_ms,
            bytes: t.bytes,
            origin: match t.origin {
                TrackOrigin::File => "file",
                TrackOrigin::Youtube { .. } => "youtube",
            },
            places: t.places.iter().map(u64::to_string).collect(),
            roads: t.roads.iter().map(u64::to_string).collect(),
        }
    }
}

#[derive(Serialize)]
pub struct NamedDto {
    pub id: String,
    pub name: String,
}

#[derive(Serialize)]
pub struct RoadNameDto {
    pub id: String,
    pub name: String,
    pub from: String,
    pub to: String,
}

#[derive(Serialize)]
pub struct LibraryDto {
    pub tracks: Vec<AdminTrackDto>,
    pub used_bytes: u64,
    pub max_bytes: u64,
    pub max_tracks: u64,
    pub max_file_bytes: usize,
    pub places: Vec<NamedDto>,
    pub roads: Vec<RoadNameDto>,
    /// Import depuis YouTube activé sur ce bot (S5).
    pub youtube_enabled: bool,
}

async fn library(State(state): State<AppState>, Authed(s): Authed) -> ApiResult<Json<LibraryDto>> {
    let universe_id = require_music_admin(&state, &s).await?;
    let tracks = music::list_tracks(universe_id).await?;
    let places: Vec<crate::database::places::Place> = crate::database::places::get_places_by_universe_id(universe_id).await?.try_collect().await?;
    let roads: Vec<crate::database::road::Road> = crate::database::road::get_roads_by_universe(universe_id).await?.try_collect().await?;
    let name_of = |id: u64| places.iter().find(|p| p.category_id == id).map(|p| p.name.clone()).unwrap_or_default();
    let mut place_dtos: Vec<NamedDto> = places.iter().map(|p| NamedDto { id: p.category_id.to_string(), name: p.name.clone() }).collect();
    place_dtos.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    let mut road_dtos: Vec<RoadNameDto> = roads
        .iter()
        .map(|r| RoadNameDto { id: r.channel_id.to_string(), name: r.road_name.clone(), from: name_of(r.place_one_id), to: name_of(r.place_two_id) })
        .collect();
    road_dtos.sort_by(|a, b| (a.from.to_lowercase(), a.to.to_lowercase()).cmp(&(b.from.to_lowercase(), b.to.to_lowercase())));
    Ok(Json(LibraryDto {
        used_bytes: tracks.iter().map(|t| t.bytes).sum(),
        tracks: tracks.iter().map(AdminTrackDto::from).collect(),
        max_bytes: music::MAX_UNIVERSE_BYTES,
        max_tracks: music::MAX_TRACKS_PER_UNIVERSE,
        max_file_bytes: MAX_FILE_BYTES,
        places: place_dtos,
        roads: road_dtos,
        youtube_enabled: crate::music::ytdlp::enabled(),
    }))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UploadQuery {
    title: String,
    /// « J'ai les droits sur cette musique » : obligatoire.
    rights: bool,
}

/// Contrôle un fichier (hors de l'exécuteur async : le décodage est coûteux) et l'enregistre.
pub async fn store_checked(universe_id: ObjectId, title: String, origin: TrackOrigin, bytes: Vec<u8>, created_by: u64) -> ApiResult<MusicTrack> {
    let (info, bytes) = tokio::task::spawn_blocking(move || (check_audio(&bytes), bytes)).await.map_err(|_| ApiError::internal())?;
    let info = info.map_err(|e| ApiError::new(StatusCode::UNPROCESSABLE_ENTITY, e.key()))?;
    music::insert_track(music::NewTrack { universe_id, title, origin, bytes: &bytes, info, created_by, rights_confirmed: true })
        .await
        .map_err(|e| crate::activity::http::service_error(&e))
}

async fn upload(State(state): State<AppState>, Authed(s): Authed, Query(q): Query<UploadQuery>, body: Bytes) -> ApiResult<Json<AdminTrackDto>> {
    check_write(&state, &s)?;
    let universe_id = require_music_admin(&state, &s).await?;
    if !q.rights {
        return Err(ApiError::bad_request("activity__music_rights_required"));
    }
    let title = music::clean_title(&q.title).ok_or_else(|| ApiError::bad_request("activity__music_title_required"))?;
    if body.len() > MAX_FILE_BYTES {
        return Err(ApiError::new(StatusCode::PAYLOAD_TOO_LARGE, "activity__payload_too_large"));
    }
    let _slot = state.music_uploads.reserve(universe_id)?;
    let track = store_checked(universe_id, title, TrackOrigin::File, body.to_vec(), s.user_id).await?;
    publish(&state, universe_id);
    Ok(Json(AdminTrackDto::from(&track)))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UpdateRequest {
    title: String,
    places: Vec<String>,
    roads: Vec<String>,
}

/// Identifiants demandés, tous connus de l'univers ; sans doublon.
pub fn known_ids(requested: &[String], known: &HashSet<u64>) -> Option<Vec<u64>> {
    let mut out = Vec::with_capacity(requested.len());
    for r in requested {
        let id: u64 = r.parse().ok()?;
        if !known.contains(&id) {
            return None;
        }
        if !out.contains(&id) {
            out.push(id);
        }
    }
    Some(out)
}

async fn update(State(state): State<AppState>, Authed(s): Authed, Path(id): Path<String>, Json(req): Json<UpdateRequest>) -> ApiResult<Json<AdminTrackDto>> {
    check_write(&state, &s)?;
    let universe_id = require_music_admin(&state, &s).await?;
    let id = parse_id(&id)?;
    let title = music::clean_title(&req.title).ok_or_else(|| ApiError::bad_request("activity__music_title_required"))?;
    let places: HashSet<u64> = crate::database::places::get_places_by_universe_id(universe_id).await?.try_collect::<Vec<_>>().await?.iter().map(|p| p.category_id).collect();
    let roads: HashSet<u64> = crate::database::road::get_roads_by_universe(universe_id).await?.try_collect::<Vec<_>>().await?.iter().map(|r| r.channel_id).collect();
    let place_ids = known_ids(&req.places, &places).ok_or_else(|| ApiError::bad_request("activity__music_unknown_spot"))?;
    let road_ids = known_ids(&req.roads, &roads).ok_or_else(|| ApiError::bad_request("activity__music_unknown_spot"))?;
    if !music::update_track(universe_id, id, &title, &place_ids, &road_ids).await? {
        return Err(ApiError::not_found("activity__music_not_found"));
    }
    publish(&state, universe_id);
    let track = music::get_track(universe_id, id).await?.ok_or_else(|| ApiError::not_found("activity__music_not_found"))?;
    Ok(Json(AdminTrackDto::from(&track)))
}

async fn remove(State(state): State<AppState>, Authed(s): Authed, Path(id): Path<String>) -> ApiResult<StatusCode> {
    check_write(&state, &s)?;
    let universe_id = require_music_admin(&state, &s).await?;
    if !music::delete_track(universe_id, parse_id(&id)?).await? {
        return Err(ApiError::not_found("activity__music_not_found"));
    }
    publish(&state, universe_id);
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct YoutubeRequest {
    url: String,
    /// Titre choisi ; sinon celui de la vidéo.
    #[serde(default)]
    title: Option<String>,
    rights: bool,
}

/// Lance l'import du son d'une vidéo YouTube (décision M1, § 3.3) ; l'activité suit la tâche.
async fn youtube_start(State(state): State<AppState>, Authed(s): Authed, Json(req): Json<YoutubeRequest>) -> ApiResult<(StatusCode, Json<serde_json::Value>)> {
    check_write(&state, &s)?;
    let universe_id = require_music_admin(&state, &s).await?;
    if !crate::music::ytdlp::enabled() {
        return Err(ApiError::forbidden("activity__music_youtube_disabled"));
    }
    if !req.rights {
        return Err(ApiError::bad_request("activity__music_rights_required"));
    }
    let video_id = crate::music::ytdlp::video_id(&req.url).ok_or_else(|| ApiError::bad_request("activity__music_youtube_bad_url"))?;
    let chosen_title = req.title.as_deref().and_then(music::clean_title);
    let slot = state.music_uploads.reserve(universe_id)?;
    let job_id = ObjectId::new().to_hex();
    state.music_uploads.job_put(&job_id, YoutubeJob { universe_id, created: std::time::Instant::now(), status: YoutubeStatus::Running });
    let (uploads, hub, user_id, id) = (state.music_uploads.clone(), state.hub.clone(), s.user_id, job_id.clone());
    tokio::spawn(async move {
        let _slot = slot;
        let status = match crate::music::ytdlp::fetch(&video_id).await {
            Err(e) => YoutubeStatus::Failed { error: e.key().to_string() },
            Ok(fetched) => {
                let title = chosen_title
                    .or_else(|| fetched.title.as_deref().and_then(music::clean_title))
                    .unwrap_or_else(|| format!("YouTube {video_id}"));
                match store_checked(universe_id, title, TrackOrigin::Youtube { video_id }, fetched.bytes, user_id).await {
                    Ok(track) => {
                        hub.publish(Event { kind: EventKind::MusicUpdated, universe_id: Some(universe_id), users: vec![] });
                        YoutubeStatus::Done { track: AdminTrackDto::from(&track) }
                    }
                    Err(e) => YoutubeStatus::Failed { error: e.key },
                }
            }
        };
        uploads.job_set(&id, status);
    });
    Ok((StatusCode::ACCEPTED, Json(serde_json::json!({ "job_id": job_id }))))
}

async fn youtube_job(State(state): State<AppState>, Authed(s): Authed, Path(id): Path<String>) -> ApiResult<Json<YoutubeStatus>> {
    let universe_id = require_music_admin(&state, &s).await?;
    state.music_uploads.job_get(&id, universe_id).map(Json).ok_or_else(|| ApiError::not_found("activity__music_not_found"))
}

/// Routes de la musique. L'envoi d'un fichier a sa propre limite de taille.
pub fn routes() -> (Router<AppState>, Router<AppState>) {
    let standard = Router::new()
        .route("/music/here", get(here))
        .route("/music/tracks/{id}", get(track_file))
        .route("/admin/music", get(library))
        .route("/admin/music/{id}", put(update).delete(remove))
        .route("/admin/music/youtube", post(youtube_start))
        .route("/admin/music/youtube/{job}", get(youtube_job));
    let upload = Router::new().route("/admin/music/upload", post(upload)).layer(DefaultBodyLimit::max(MAX_UPLOAD_BODY));
    (standard, upload)
}

/// Taille max d'une requête d'envoi (fichier + marge).
pub const MAX_UPLOAD_BODY: usize = MAX_FILE_BYTES + 1024;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn assignments_must_name_known_places_or_roads() {
        let known: HashSet<u64> = [10, 11].into_iter().collect();
        assert_eq!(known_ids(&["10".into(), "11".into(), "10".into()], &known), Some(vec![10, 11]));
        assert_eq!(known_ids(&[], &known), Some(vec![]));
        assert_eq!(known_ids(&["12".into()], &known), None, "another universe's place");
        assert_eq!(known_ids(&["x".into()], &known), None);
    }

    #[test]
    fn only_checked_types_are_served() {
        assert_eq!(served_mime("audio/ogg"), Some("audio/ogg"));
        assert_eq!(served_mime("text/html"), None);
    }

    #[test]
    fn one_upload_at_a_time_per_universe() {
        let uploads = Arc::new(MusicUploads::default());
        let (a, b) = (ObjectId::new(), ObjectId::new());
        let slot = uploads.reserve(a).unwrap();
        assert!(uploads.reserve(a).is_err());
        assert!(uploads.reserve(b).is_ok(), "other universes are not blocked");
        drop(slot);
        assert!(uploads.reserve(a).is_ok(), "released at the end");
    }

    #[test]
    fn youtube_jobs_are_only_visible_to_their_universe_and_expire() {
        let uploads = MusicUploads::default();
        let (u, other) = (ObjectId::new(), ObjectId::new());
        uploads.job_put("j", YoutubeJob { universe_id: u, created: std::time::Instant::now(), status: YoutubeStatus::Running });
        assert!(matches!(uploads.job_get("j", u), Some(YoutubeStatus::Running)));
        assert!(uploads.job_get("j", other).is_none());
        uploads.job_set("j", YoutubeStatus::Failed { error: "activity__music_youtube_unavailable".into() });
        assert!(matches!(uploads.job_get("j", u), Some(YoutubeStatus::Failed { .. })));
        let old = std::time::Instant::now().checked_sub(JOB_TTL + std::time::Duration::from_secs(1));
        if let Some(created) = old {
            uploads.job_put("old", YoutubeJob { universe_id: u, created, status: YoutubeStatus::Running });
            assert!(uploads.job_get("old", u).is_none(), "expired");
        }
    }

    #[test]
    fn the_player_hears_the_place_or_the_road() {
        assert_eq!(spot_of(&SpaceType::Place, 3), Spot::Place(3));
        assert_eq!(spot_of(&SpaceType::Road, 4), Spot::Road(4));
    }

    /// Bout en bout à travers le routeur, contre MongoDB, avec un faux Discord : un admin envoie,
    /// assigne et supprime ; un joueur entend la playlist de son lieu ; tout le reste est refusé.
    mod routes_integration {
        use super::*;
        use crate::activity::discord_api::*;
        use crate::activity::session::test_session;
        use crate::database::places::Place;
        use crate::database::travel::TravelGroup;
        use crate::test_support::{block_on, random_id};
        use axum::body::{to_bytes, Body};
        use axum::http::Request;
        use std::sync::Arc;
        use tower::ServiceExt;

        /// Faux Discord : 1 est administrateur du serveur, tous les autres sont joueurs.
        struct Fake;

        impl DiscordGateway for Fake {
            fn exchange_code<'a>(&'a self, _c: &'a str) -> BoxFuture<'a, Result<TokenGrant, DiscordError>> {
                Box::pin(async { Err(DiscordError::Rejected) })
            }
            fn current_user<'a>(&'a self, _t: &'a str) -> BoxFuture<'a, Result<DiscordUser, DiscordError>> {
                Box::pin(async { Err(DiscordError::Rejected) })
            }
            fn activity_instance<'a>(&'a self, _id: &'a str) -> BoxFuture<'a, Result<ActivityInstance, DiscordError>> {
                Box::pin(async { Err(DiscordError::Rejected) })
            }
            fn member<'a>(&'a self, g: u64, u: u64) -> BoxFuture<'a, Result<Option<MemberInfo>, DiscordError>> {
                Box::pin(async move {
                    let perms = if u == 1 { crate::activity::permissions::ADMINISTRATOR } else { 0 };
                    Ok(Some(MemberInfo { roles: vec![77], guild_owner_id: 999, guild_roles: vec![(g, 0), (77, perms)] }))
                })
            }
            fn bot_guilds<'a>(&'a self, _l: usize) -> BoxFuture<'a, Result<Vec<GuildSummary>, DiscordError>> {
                Box::pin(async { Ok(vec![]) })
            }
            fn guild_counts<'a>(&'a self, _g: u64) -> BoxFuture<'a, Result<GuildCounts, DiscordError>> {
                Box::pin(async { Ok(GuildCounts { channels: 0, roles: 0 }) })
            }
        }

        fn req(method: &str, uri: &str, token: &str, body: Body, json: bool) -> Request<Body> {
            let mut b = Request::builder().method(method).uri(uri).header("authorization", format!("Bearer {token}"));
            if json {
                b = b.header("content-type", "application/json");
            }
            b.body(body).unwrap()
        }

        async fn call(app: &Router, r: Request<Body>) -> (StatusCode, axum::http::HeaderMap, Vec<u8>) {
            let res = app.clone().oneshot(r).await.unwrap();
            let (status, headers) = (res.status(), res.headers().clone());
            (status, headers, to_bytes(res.into_body(), 64 << 20).await.unwrap().to_vec())
        }

        fn json(b: &[u8]) -> serde_json::Value {
            serde_json::from_slice(b).unwrap_or(serde_json::Value::Null)
        }

        #[test]
        #[ignore = "integration test: needs MongoDB"]
        fn admins_build_the_library_and_players_hear_their_place() {
            block_on(async {
                let _ = crate::database::db_client::get_db_client().await;
                let (universe_id, guild) = (ObjectId::new(), random_id());
                let mut server = crate::database::server::Server::default();
                server.universe_id = universe_id;
                server.server_id = guild;
                server.insert_server().await.unwrap();
                let (tavern, forest) = (random_id(), random_id());
                for (id, name) in [(tavern, "Taverne"), (forest, "Forêt")] {
                    Place { _id: ObjectId::new(), universe_id, server_id: guild, category_id: id, role: 1, name: name.into(), modifiers: vec![], central_channel_id: None }.insert().await.unwrap();
                }
                TravelGroup { _id: ObjectId::new(), universe_id, actual_space_id: tavern, members: vec![2], server_id: guild, ..Default::default() }.insert().await.unwrap();

                let state = AppState::new(Arc::new(Fake), std::time::Duration::from_secs(600));
                let session = |user: u64, access: AccessLevel| Session { user_id: user, guild_id: guild, universe_id: Some(universe_id), access, ..test_session(user) };
                let admin = state.sessions.create(session(1, AccessLevel::Admin));
                // Session falsifiée : se dit admin, Discord dit joueur.
                let liar = state.sessions.create(session(4, AccessLevel::Admin));
                let player = state.sessions.create(session(2, AccessLevel::Player));
                let stranger = state.sessions.create(Session { universe_id: Some(ObjectId::new()), ..session(3, AccessLevel::Player) });
                let app = crate::activity::http::router(state.clone());
                let fixture = |n: &str| std::fs::read(format!("{}/tests/fixtures/audio/{n}", env!("CARGO_MANIFEST_DIR"))).unwrap();
                let up = |token: &str, title: &str, rights: bool, bytes: Vec<u8>| {
                    req("POST", &format!("/api/admin/music/upload?title={title}&rights={rights}"), token, Body::from(bytes), false)
                };

                // Refus : pas admin (même si la session le prétend), sans droits, fichier déguisé ou vidéo.
                let (st, _, _) = call(&app, up(&liar, "Taverne", true, fixture("tone.ogg"))).await;
                assert_eq!(st, StatusCode::FORBIDDEN, "access is checked with Discord, not the session");
                let (st, _, b) = call(&app, up(&admin, "Taverne", false, fixture("tone.ogg"))).await;
                assert_eq!((st, json(&b)["error"].as_str()), (StatusCode::BAD_REQUEST, Some("activity__music_rights_required")));
                let (st, _, b) = call(&app, up(&admin, "Page", true, fixture("page.mp3"))).await;
                assert_eq!((st, json(&b)["error"].as_str()), (StatusCode::UNPROCESSABLE_ENTITY, Some("activity__music_unknown_format")));
                let (st, _, b) = call(&app, up(&admin, "Clip", true, fixture("video.mp4"))).await;
                assert_eq!((st, json(&b)["error"].as_str()), (StatusCode::UNPROCESSABLE_ENTITY, Some("activity__music_not_audio")));

                // Import YouTube : coupé par défaut (après une pause : limite d'écriture).
                tokio::time::sleep(std::time::Duration::from_millis(1100)).await;
                let yt = Body::from(serde_json::json!({"url": "https://youtu.be/dQw4w9WgXcQ", "rights": true}).to_string());
                let (st, _, b) = call(&app, req("POST", "/api/admin/music/youtube", &admin, yt, true)).await;
                assert_eq!((st, json(&b)["error"].as_str()), (StatusCode::FORBIDDEN, Some("activity__music_youtube_disabled")));

                // Envoi accepté (après une pause : 3 écritures d'affilée au plus, puis 1 par seconde).
                let pause = || tokio::time::sleep(std::time::Duration::from_millis(1100));
                pause().await;
                let (st, _, b) = call(&app, up(&admin, "Taverne%20du%20port", true, fixture("tone.ogg"))).await;
                assert_eq!(st, StatusCode::OK, "{}", String::from_utf8_lossy(&b));
                let id = json(&b)["id"].as_str().unwrap().to_string();
                assert_eq!(json(&b)["title"], "Taverne du port");

                // Assignation : un lieu d'un autre univers est refusé.
                let body = |places: Vec<String>| Body::from(serde_json::json!({"title": "Taverne du port", "places": places, "roads": []}).to_string());
                pause().await;
                let (st, _, _) = call(&app, req("PUT", &format!("/api/admin/music/{id}"), &admin, body(vec![random_id().to_string()]), true)).await;
                assert_eq!(st, StatusCode::BAD_REQUEST);
                let (st, _, _) = call(&app, req("PUT", &format!("/api/admin/music/{id}"), &player, body(vec![tavern.to_string()]), true)).await;
                assert_eq!(st, StatusCode::FORBIDDEN);
                pause().await;
                let (st, _, b) = call(&app, req("PUT", &format!("/api/admin/music/{id}"), &admin, body(vec![tavern.to_string()]), true)).await;
                assert_eq!(st, StatusCode::OK, "{}", String::from_utf8_lossy(&b));

                // Le joueur, à la taverne, entend le morceau ; il peut le télécharger, pas un étranger.
                let (st, _, b) = call(&app, req("GET", "/api/music/here", &player, Body::empty(), false)).await;
                assert_eq!(st, StatusCode::OK);
                let here = json(&b);
                assert_eq!(here["spot"], serde_json::json!({"kind": "place", "id": tavern.to_string()}));
                assert_eq!(here["tracks"][0]["id"], id.as_str());
                let (st, h, b) = call(&app, req("GET", &format!("/api/music/tracks/{id}"), &player, Body::empty(), false)).await;
                assert_eq!(st, StatusCode::OK);
                assert_eq!(b, fixture("tone.ogg"));
                assert_eq!(h["content-type"], "audio/ogg");
                assert_eq!(h["x-content-type-options"], "nosniff");
                assert!(h["content-security-policy"].to_str().unwrap().starts_with("sandbox"));
                let (st, _, _) = call(&app, req("GET", &format!("/api/music/tracks/{id}"), &stranger, Body::empty(), false)).await;
                assert_eq!(st, StatusCode::NOT_FOUND, "another universe gets nothing");

                // Bibliothèque admin, puis suppression.
                let (st, _, b) = call(&app, req("GET", "/api/admin/music", &admin, Body::empty(), false)).await;
                assert_eq!(st, StatusCode::OK);
                let lib = json(&b);
                assert_eq!(lib["tracks"][0]["places"], serde_json::json!([tavern.to_string()]));
                assert_eq!(lib["places"].as_array().unwrap().len(), 2);
                pause().await;
                let (st, _, _) = call(&app, req("DELETE", &format!("/api/admin/music/{id}"), &admin, Body::empty(), false)).await;
                assert_eq!(st, StatusCode::NO_CONTENT);
                let (_, _, b) = call(&app, req("GET", "/api/music/here", &player, Body::empty(), false)).await;
                assert_eq!(json(&b)["tracks"], serde_json::json!([]));

                crate::database::music::delete_universe_music(universe_id).await.unwrap();
            });
        }
    }
}
