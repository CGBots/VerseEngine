//! « Mettre à jour le rendu seulement » : l'admin renvoie la sauvegarde `.map` d'une carte déjà
//! importée ; le bot recalcule le rendu façon Azgaar (cellules, contours vectoriels, rivières,
//! relief) sans toucher à Discord ni aux lieux. Sert aux cartes importées avant le rendu
//! vectoriel (le `.map` d'origine n'est pas conservé).
//!
//! `POST /api/admin/import/render` : utilise la carte envoyée par `POST /admin/import/map`.

use axum::extract::State;
use axum::routing::post;
use axum::{Json, Router};

use crate::activity::error::{ApiError, ApiResult};
use crate::activity::http::{AppState, Authed};
use crate::activity::map::azgaar::AzgaarMap;
use crate::database::universe_maps::{MapSource, MapTerrain, UniverseMap};

/// Écart toléré entre la position d'un lieu et celle de son bourg dans la carte renvoyée.
const POSITION_TOLERANCE: f64 = 3.0;
/// Part des lieux qui doivent retrouver leur bourg.
const MATCH_RATIO: f64 = 0.9;

/// La carte renvoyée est-elle celle de l'univers ? Mêmes dimensions, et au moins 90 % des lieux
/// liés à un bourg le retrouvent (même identifiant, à moins de 3 unités).
pub fn same_map(stored: &UniverseMap, map: &AzgaarMap) -> Result<(), &'static str> {
    if stored.source != MapSource::Azgaar {
        return Err("activity__render_no_azgaar_map");
    }
    if (stored.width - map.width).abs() > 1.0 || (stored.height - map.height).abs() > 1.0 {
        return Err("activity__render_not_same_map");
    }
    let linked: Vec<_> = stored.positions.iter().filter_map(|p| p.azgaar_burg_id.map(|b| (b, p.x, p.y))).collect();
    if linked.is_empty() {
        return Err("activity__render_not_same_map");
    }
    let matched = linked
        .iter()
        .filter(|(b, x, y)| map.burgs.iter().any(|m| m.id == *b && (m.x - x).abs() <= POSITION_TOLERANCE && (m.y - y).abs() <= POSITION_TOLERANCE))
        .count();
    if (matched as f64) < linked.len() as f64 * MATCH_RATIO {
        return Err("activity__render_not_same_map");
    }
    Ok(())
}

/// Carte de l'univers avec le rendu recalculé depuis `map` ; lieux, routes et zone inchangés
/// (la zone du lasso est réappliquée au nouveau rendu).
pub fn refreshed_map(stored: &UniverseMap, map: &AzgaarMap, now: u64) -> Result<UniverseMap, &'static str> {
    same_map(stored, map)?;
    let style = map.style.clone().ok_or("activity__render_no_style")?;
    let zone: Vec<[f64; 2]> = stored.zone.as_ref().map(|z| z.polygon.clone()).unwrap_or_default();
    let mut out = stored.clone();
    out.style = Some(crate::activity::import_exec::hide_outside_zone(style, &zone, map.width, map.height));
    if let Some(t) = crate::activity::map::azgaar::resample_terrain(map, crate::activity::map::azgaar::MAX_TERRAIN_SIDE) {
        out.terrain = Some(MapTerrain { cols: t.cols as u32, rows: t.rows as u32, heights: t.heights.iter().map(|&h| ((h.min(100) as u32 * 255) / 100) as u8).collect() });
    }
    out.updated_at = now;
    Ok(out)
}

async fn render_route(State(state): State<AppState>, Authed(s): Authed) -> ApiResult<Json<serde_json::Value>> {
    state.write_limiter.check(&format!("user:{}", s.user_id)).map_err(ApiError::too_many)?;
    // Mêmes droits que l'import (créateur de l'univers ou administrateur, revérifiés).
    let ctx = crate::activity::admin_import::require_importer(&state, &s).await?;
    let universe = ctx.universe.ok_or_else(|| ApiError::not_found("activity__server_not_linked"))?;
    let draft = state.import_drafts.get(s.guild_id, s.user_id).ok_or_else(|| ApiError::not_found("activity__import_no_map"))?;
    if draft.generated() {
        return Err(ApiError::bad_request("activity__render_no_azgaar_map"));
    }
    let stored = crate::database::universe_maps::get_universe_map(universe.universe_id).await?.ok_or_else(|| ApiError::not_found("activity__render_no_azgaar_map"))?;
    let refreshed = refreshed_map(&stored, &draft.map, crate::activity::http::now_secs()).map_err(ApiError::bad_request)?;
    let vector = refreshed.style.as_ref().is_some_and(|st| st.pack.is_some());
    crate::database::universe_maps::save_universe_map(&refreshed).await?;
    state.import_drafts.remove(s.guild_id, s.user_id);
    // La sauvegarde renvoyée est gardée (c'est la bonne : vérifiée ci-dessus) et envoyée au créateur.
    if let Some(bytes) = draft.source.clone() {
        tokio::spawn(crate::activity::map_source::keep_and_send(state.bot_http.clone(), universe.universe_id, bytes.to_vec(), draft.map.name.clone(), s.user_id, s.locale));
    }
    tracing::info!(universe = %universe.universe_id, vector, "carte : rendu mis à jour depuis le .map");
    Ok(Json(serde_json::json!({ "result": "activity__render_updated", "vector": vector })))
}

pub fn routes() -> Router<AppState> {
    Router::new().route("/admin/import/render", post(render_route))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::activity::map::azgaar::Burg;
    use crate::database::universe_maps::{MapPosition, MapZone, OutsideZone};
    use mongodb::bson::oid::ObjectId;

    fn stored() -> UniverseMap {
        let pos = |place: u64, burg: u64, x: f64, y: f64| MapPosition { place_id: place, x, y, z: 0.0, azgaar_burg_id: Some(burg) };
        UniverseMap {
            id: ObjectId::new(), universe_id: ObjectId::new(), source: MapSource::Azgaar, width: 1000.0, height: 500.0,
            positions: vec![pos(1, 3, 100.0, 100.0), pos(2, 7, 400.0, 250.0)], terrain: None,
            zone: Some(MapZone { polygon: vec![[0.0, 0.0], [500.0, 0.0], [500.0, 500.0], [0.0, 500.0]], outside: OutsideZone::Grey }),
            graph_hash: None, road_paths: vec![], background: None, style: None, updated_at: 1,
        }
    }

    fn azgaar(burgs: &[(u64, f64, f64)], width: f64) -> AzgaarMap {
        AzgaarMap {
            name: None, width, height: 500.0, distance_scale: None, distance_unit: None,
            burgs: burgs.iter().map(|&(id, x, y)| Burg { id, name: format!("B{id}"), x, y, cell: None, capital: false }).collect(),
            cells: vec![], routes: vec![], style: None,
        }
    }

    #[test]
    fn only_the_same_map_is_accepted() {
        let s = stored();
        assert_eq!(same_map(&s, &azgaar(&[(3, 101.0, 99.5), (7, 400.0, 251.0)], 1000.0)), Ok(()));
        assert_eq!(same_map(&s, &azgaar(&[(3, 101.0, 99.5), (7, 400.0, 251.0)], 1200.0)), Err("activity__render_not_same_map"), "other size");
        assert_eq!(same_map(&s, &azgaar(&[(3, 600.0, 100.0), (7, 400.0, 251.0)], 1000.0)), Err("activity__render_not_same_map"), "burg moved");
        let mut generated = stored();
        generated.source = MapSource::Manual;
        assert_eq!(same_map(&generated, &azgaar(&[(3, 100.0, 100.0), (7, 400.0, 250.0)], 1000.0)), Err("activity__render_no_azgaar_map"));
    }

    #[test]
    fn the_render_is_replaced_but_places_and_zone_are_kept() {
        let s = stored();
        let mut m = azgaar(&[(3, 100.0, 100.0), (7, 400.0, 250.0)], 1000.0);
        assert_eq!(refreshed_map(&s, &m, 9).unwrap_err(), "activity__render_no_style", "a .map without render data");
        m.style = Some(crate::database::universe_maps::MapStyle {
            cols: 2, rows: 1, surface: vec![4, 4], states: vec![], biome_colors: vec![], state_colors: vec![], rivers: vec![], burgs: vec![], exact: true, pack: None,
        });
        let out = refreshed_map(&s, &m, 9).unwrap();
        assert_eq!((out.id, out.positions.clone(), out.zone.clone(), out.updated_at), (s.id, s.positions.clone(), s.zone.clone(), 9));
        // La zone (moitié gauche) est réappliquée : la case de droite est cachée.
        assert_eq!(out.style.unwrap().surface, vec![4, crate::database::universe_maps::SURFACE_HIDDEN]);
    }
}
