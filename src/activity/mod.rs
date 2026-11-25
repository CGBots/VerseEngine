//! # Activité Discord (Embedded App SDK)
//!
//! API HTTP + WebSocket embarquée **dans le processus du bot**, pour réutiliser les
//! ordonnanceurs en mémoire (`travel::logic`, `craft::logic`…). Désactivée par défaut
//! (`ACTIVITY_ENABLED`). Voir `docs/activity/ARCHITECTURE.md`.

pub mod admin_import;
pub mod admin_panel;
pub mod config;
pub mod craft_view;
pub mod discord_api;
pub mod dto;
pub mod error;
pub mod events;
pub mod farm_results;
pub mod fog;
pub mod group_invites;
pub mod http;
pub mod images;
pub mod import_exec;
pub mod import_removal;
#[allow(dead_code)] // azgaar/network : utilisés par l'import admin (phase d)
pub mod map;
pub mod permissions;
pub mod place_admin;
pub mod ratelimit;
pub mod render_refresh;
pub mod session;
pub mod spectator;
pub mod map_view;
pub mod map_source;
pub mod music_api;
pub mod travel_view;

use std::sync::Arc;

use axum::http::{header, HeaderValue};
use serenity::all::Http;
use tower_http::services::{ServeDir, ServeFile};
use tower_http::set_header::SetResponseHeaderLayer;

use config::{ActivityConfig, ConfigError};

/// CSP des fichiers statiques : tout passe par le proxy Discord (même origine).
const STATIC_CSP: &str = "default-src 'self'; img-src 'self' data: blob: https://cdn.discordapp.com https://media.discordapp.net; \
connect-src 'self'; media-src 'self' blob:; frame-src 'self' https://www.youtube-nocookie.com; style-src 'self' 'unsafe-inline'; frame-ancestors https://discord.com https://*.discord.com https://*.discordsays.com";

/// CSP d'Azgaar (`/azgaar/`, étape E9). `sandbox` : le navigateur donne à la page une **origine
/// opaque**, même ouverte directement ou dans un cadre sans attribut `sandbox`. Elle ne peut donc
/// ni lire le jeton de session de l'activité, ni toucher sa page, ni appeler l'API en son nom
/// (requêtes sans cookie ni jeton, depuis l'origine `null`). Pas de fenêtres surgissantes, pas de
/// cadres, aucun réseau hors de notre domaine. Seule sortie : `postMessage` vers l'activité, qui
/// n'accepte qu'une carte (voir `web/activity/src/admin/azgaarBridge.ts`).
/// `unsafe-eval` : Azgaar compile ses formules de répartition des ressources (`new Function`) ; le
/// code ainsi produit reste enfermé dans le même bac à sable.
const AZGAAR_CSP: &str = "sandbox allow-scripts allow-downloads allow-modals; default-src 'self'; script-src 'self' 'unsafe-eval'; \
style-src 'self' 'unsafe-inline'; img-src 'self' data: blob:; font-src 'self' data:; connect-src 'self' data: blob:; \
worker-src 'self' blob:; frame-src 'none'; object-src 'none'; base-uri 'none'; form-action 'none'; \
frame-ancestors 'self' https://discord.com https://*.discord.com https://*.discordsays.com";

/// Fichiers statiques : le front (`dir`) et, sous `/azgaar/`, Azgaar compilé (`web/azgaar/build.sh`),
/// chacun avec sa CSP.
fn static_files(dir: &std::path::Path) -> axum::Router {
    let index = dir.join("index.html");
    let files = ServeDir::new(dir).not_found_service(ServeFile::new(index));
    let front = tower::ServiceBuilder::new()
        .layer(SetResponseHeaderLayer::if_not_present(header::CONTENT_SECURITY_POLICY, HeaderValue::from_static(STATIC_CSP)))
        .service(files);
    // Depuis l'origine opaque, les scripts modules et les polices sont des requêtes CORS : fichiers
    // publics, sans identifiants, donc `*` suffit.
    let azgaar = tower::ServiceBuilder::new()
        .layer(SetResponseHeaderLayer::overriding(header::CONTENT_SECURITY_POLICY, HeaderValue::from_static(AZGAAR_CSP)))
        .layer(SetResponseHeaderLayer::overriding(header::ACCESS_CONTROL_ALLOW_ORIGIN, HeaderValue::from_static("*")))
        .layer(SetResponseHeaderLayer::overriding(header::X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff")))
        .service(ServeDir::new(dir.join("azgaar")));
    axum::Router::new().nest_service("/azgaar", azgaar).fallback_service(front)
}

/// Démarre le serveur de l'activité si `ACTIVITY_ENABLED` est vrai.
/// `bot_http` est le client HTTP du bot (même limiteur de débit Discord).
pub fn spawn(bot_http: Arc<Http>) {
    let cfg = match ActivityConfig::from_env() {
        Ok(c) => c,
        Err(ConfigError::Disabled) => return,
        Err(e) => {
            tracing::error!(?e, "activity: configuration invalide, API non démarrée");
            return;
        }
    };
    tokio::spawn(async move {
        let discord = Arc::new(discord_api::LiveDiscord::new(&cfg, bot_http.clone()));
        let state = http::AppState::new(discord, cfg.session_ttl).with_bot_http(bot_http).with_client_id(cfg.client_id);
        state.hub.start_watchers();
        let mut app = http::router(state);
        if let Some(dir) = cfg.static_dir.as_ref().filter(|d| d.is_dir()) {
            app = app.fallback_service(static_files(dir));
        } else {
            tracing::warn!(dir = ?cfg.static_dir, "activity: dossier du front introuvable, seule l'API est servie");
        }
        let listener = match tokio::net::TcpListener::bind(cfg.bind).await {
            Ok(l) => l,
            Err(e) => {
                tracing::error!(error = %e, bind = %cfg.bind, "activity: impossible d'écouter");
                return;
            }
        };
        tracing::info!(bind = %cfg.bind, "activity: API démarrée");
        if let Err(e) = axum::serve(listener, app).await {
            tracing::error!(error = %e, "activity: arrêt du serveur");
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    async fn get(app: &axum::Router, uri: &str) -> (StatusCode, axum::http::HeaderMap) {
        let res = app.clone().oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap()).await.unwrap();
        (res.status(), res.headers().clone())
    }

    #[test]
    fn azgaar_is_served_in_a_sandbox_and_the_front_is_not() {
        let dir = std::env::temp_dir().join(format!("verseengine-static-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("azgaar")).unwrap();
        std::fs::write(dir.join("index.html"), "front").unwrap();
        std::fs::write(dir.join("azgaar/index.html"), "azgaar").unwrap();
        std::fs::write(dir.join("azgaar/app.js"), "1").unwrap();
        let app = static_files(&dir);
        crate::test_support::block_on(async {
            for uri in ["/azgaar/", "/azgaar/app.js"] {
                let (status, h) = get(&app, uri).await;
                assert_eq!(status, StatusCode::OK, "{uri}");
                let csp = h.get(header::CONTENT_SECURITY_POLICY).unwrap().to_str().unwrap();
                assert!(csp.starts_with("sandbox allow-scripts allow-downloads allow-modals;"), "{uri}: {csp}");
                assert!(!csp.contains("allow-same-origin") && !csp.contains("allow-popups") && !csp.contains("allow-top-navigation"));
                assert!(csp.contains("connect-src 'self'") && csp.contains("frame-src 'none'"));
                let scripts = csp.split(';').map(str::trim).find(|d| d.starts_with("script-src")).unwrap();
                assert_eq!(scripts, "script-src 'self' 'unsafe-eval'", "no inline scripts, no other host");
                assert_eq!(h.get(header::ACCESS_CONTROL_ALLOW_ORIGIN).unwrap(), "*");
            }
            let (status, h) = get(&app, "/").await;
            assert_eq!(status, StatusCode::OK);
            let csp = h.get(header::CONTENT_SECURITY_POLICY).unwrap().to_str().unwrap();
            assert!(!csp.contains("sandbox"), "the activity itself is not sandboxed");
            assert!(csp.contains("media-src 'self' blob:"), "music is played from blob: URLs");
            assert!(csp.contains("frame-src 'self' https://www.youtube-nocookie.com;"), "only YouTube's official player may be framed");
            assert!(h.get(header::ACCESS_CONTROL_ALLOW_ORIGIN).is_none());
            // Une route inconnue du front renvoie la page de l'activité, pas Azgaar.
            let (_, h) = get(&app, "/somewhere").await;
            assert!(!h.get(header::CONTENT_SECURITY_POLICY).unwrap().to_str().unwrap().contains("sandbox"));
        });
        std::fs::remove_dir_all(&dir).ok();
    }
}
