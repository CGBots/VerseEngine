//! # Activité Discord (Embedded App SDK)
//!
//! API HTTP + WebSocket embarquée **dans le processus du bot**, pour réutiliser les
//! ordonnanceurs en mémoire (`travel::logic`, `craft::logic`…). Désactivée par défaut
//! (`ACTIVITY_ENABLED`). Voir `docs/activity/ARCHITECTURE.md`.

pub mod admin_import;
pub mod config;
pub mod discord_api;
pub mod dto;
pub mod error;
pub mod events;
pub mod fog;
pub mod http;
pub mod images;
pub mod import_exec;
pub mod import_removal;
#[allow(dead_code)] // azgaar/network : utilisés par l'import admin (phase d)
pub mod map;
pub mod permissions;
pub mod ratelimit;
pub mod session;
pub mod map_view;
pub mod travel_view;

use std::sync::Arc;

use axum::http::{header, HeaderValue};
use serenity::all::Http;
use tower_http::services::{ServeDir, ServeFile};
use tower_http::set_header::SetResponseHeaderLayer;

use config::{ActivityConfig, ConfigError};

/// CSP des fichiers statiques : tout passe par le proxy Discord (même origine).
const STATIC_CSP: &str = "default-src 'self'; img-src 'self' data: blob: https://cdn.discordapp.com https://media.discordapp.net; \
connect-src 'self'; style-src 'self' 'unsafe-inline'; frame-ancestors https://discord.com https://*.discord.com https://*.discordsays.com";

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
            let index = dir.join("index.html");
            let files = ServeDir::new(dir).not_found_service(ServeFile::new(index));
            app = app.fallback_service(
                tower::ServiceBuilder::new()
                    .layer(SetResponseHeaderLayer::if_not_present(header::CONTENT_SECURITY_POLICY, HeaderValue::from_static(STATIC_CSP)))
                    .service(files),
            );
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
