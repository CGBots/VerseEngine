//! Sauvegarde `.map` des univers : gardée après chaque import Azgaar ou mise à jour du rendu
//! (`database::map_sources`), et envoyée en message privé au créateur (pièce jointe), le moyen le
//! plus simple de la récupérer depuis Discord.
//!
//! Routes (mêmes droits que l'import, revérifiés) :
//! - `GET /api/admin/import/source` : sauvegardes gardées (graine, nom, date, taille).
//! - `POST /api/admin/import/source/send` : renvoie la dernière en message privé.
//!
//! Le fichier n'est jamais servi aux joueurs : il dévoile toute la carte.

use std::io::Write;
use std::sync::Arc;

use axum::extract::State;
use axum::routing::{get, post};
use axum::{Json, Router};
use mongodb::bson::oid::ObjectId;
use serenity::all::{CreateAttachment, CreateMessage, Http, UserId};

use crate::activity::error::{ApiError, ApiResult};
use crate::activity::http::{bot_http, now_secs, AppState, Authed};
use crate::database::map_sources::{self, MapSourceInfo};
use crate::translation::get_by_locale;

/// Taille max d'une pièce jointe envoyée par le bot (limite de Discord : 10 Mo).
const MAX_ATTACHMENT: usize = 9 * 1024 * 1024;

/// Nom de fichier sûr : lettres, chiffres, tirets.
pub fn file_name(universe: &str, seed: Option<&str>, gz: bool) -> String {
    let clean = |s: &str| s.chars().map(|c| if c.is_alphanumeric() || c == '-' { c } else { '-' }).collect::<String>();
    let mut name = clean(universe).trim_matches('-').chars().take(40).collect::<String>();
    if name.is_empty() {
        name = "univers".into();
    }
    if let Some(seed) = seed {
        name = format!("{name}-{}", clean(seed));
    }
    format!("{name}.map{}", if gz { ".gz" } else { "" })
}

/// Pièce jointe : le fichier tel quel, ou compressé s'il dépasse la limite ; `None` s'il est
/// trop gros même compressé.
fn attachment(bytes: &[u8]) -> Option<(Vec<u8>, bool)> {
    if bytes.len() <= MAX_ATTACHMENT {
        return Some((bytes.to_vec(), false));
    }
    if bytes.starts_with(&[0x1f, 0x8b]) {
        return None; // déjà compressé
    }
    let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::best());
    gz.write_all(bytes).ok()?;
    let out = gz.finish().ok()?;
    (out.len() <= MAX_ATTACHMENT).then_some((out, true))
}

/// Envoie la sauvegarde en message privé. Échec (messages privés fermés…) : journalisé.
pub async fn send_dm(http: &Arc<Http>, user_id: u64, universe_name: &str, info: &MapSourceInfo, bytes: &[u8], locale: &str) -> bool {
    let mut args = fluent::FluentArgs::new();
    args.set("universe", universe_name.to_string());
    args.set("seed", info.seed.clone().unwrap_or_else(|| "?".into()));
    let mut msg = CreateMessage::new();
    match attachment(bytes) {
        Some((data, gz)) => {
            msg = msg.content(get_by_locale(locale, "map_source__dm", None, Some(&args))).add_file(CreateAttachment::bytes(data, file_name(universe_name, info.seed.as_deref(), gz)));
        }
        None => msg = msg.content(get_by_locale(locale, "map_source__dm_too_big", None, Some(&args))),
    }
    match UserId::new(user_id).direct_message(http.as_ref(), msg).await {
        Ok(_) => true,
        Err(e) => {
            tracing::warn!(user = user_id, error = %e, "sauvegarde .map : message privé non envoyé");
            false
        }
    }
}

/// Garde la sauvegarde de l'univers et l'envoie au créateur. Appelé après un import réussi ou une
/// mise à jour du rendu ; un échec n'annule rien (journalisé).
pub async fn keep_and_send(http: Option<Arc<Http>>, universe_id: ObjectId, bytes: Vec<u8>, name: Option<String>, user_id: u64, locale: &'static str) {
    let info = match map_sources::save(universe_id, &bytes, name.as_deref(), user_id, now_secs()).await {
        Ok(i) => i,
        Err(e) => {
            tracing::error!(universe = %universe_id, error = %e, "sauvegarde .map non gardée");
            return;
        }
    };
    tracing::info!(universe = %universe_id, seed = ?info.seed, bytes = info.bytes, "sauvegarde .map gardée");
    let universe_name = crate::database::universe::get_universe_by_id(universe_id).await.ok().flatten().map(|u| u.name).unwrap_or_default();
    if let Some(http) = http {
        send_dm(&http, user_id, &universe_name, &info, &bytes, locale).await;
    }
}

async fn list_route(State(state): State<AppState>, Authed(s): Authed) -> ApiResult<Json<serde_json::Value>> {
    let ctx = crate::activity::admin_import::require_importer(&state, &s).await?;
    let Some(universe) = ctx.universe else { return Ok(Json(serde_json::json!({ "sources": [] }))) };
    Ok(Json(serde_json::json!({ "sources": map_sources::list(universe.universe_id).await? })))
}

async fn send_route(State(state): State<AppState>, Authed(s): Authed) -> ApiResult<Json<serde_json::Value>> {
    state.write_limiter.check(&format!("user:{}", s.user_id)).map_err(ApiError::too_many)?;
    let ctx = crate::activity::admin_import::require_importer(&state, &s).await?;
    let universe = ctx.universe.ok_or_else(|| ApiError::not_found("activity__server_not_linked"))?;
    let latest = map_sources::list(universe.universe_id).await?.into_iter().next().ok_or_else(|| ApiError::not_found("activity__map_source_none"))?;
    let bytes = map_sources::read(latest.file_id).await?;
    let http = bot_http(&state)?;
    if !send_dm(&http, s.user_id, &universe.name, &latest, &bytes, s.locale).await {
        return Err(ApiError::new(axum::http::StatusCode::BAD_GATEWAY, "activity__map_source_dm_failed"));
    }
    Ok(Json(serde_json::json!({ "result": "activity__map_source_sent" })))
}

pub fn routes() -> Router<AppState> {
    Router::new().route("/admin/import/source", get(list_route)).route("/admin/import/source/send", post(send_route))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_names_are_safe() {
        assert_eq!(file_name("Les Terres de Moria", Some("962345872"), false), "Les-Terres-de-Moria-962345872.map");
        assert_eq!(file_name("../../etc", None, true), "etc.map.gz");
        assert_eq!(file_name("", None, false), "univers.map");
    }

    #[test]
    fn big_files_are_compressed_or_skipped() {
        assert_eq!(attachment(b"small"), Some((b"small".to_vec(), false)));
        let big = vec![b'a'; MAX_ATTACHMENT + 10];
        let (data, gz) = attachment(&big).unwrap();
        assert!(gz && data.len() < MAX_ATTACHMENT);
        let mut gzipped_big = vec![0x1f, 0x8b];
        gzipped_big.extend(vec![0u8; MAX_ATTACHMENT + 10]);
        assert_eq!(attachment(&gzipped_big), None, "already compressed and too big");
    }
}
