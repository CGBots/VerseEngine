//! Erreurs de l'API : statut HTTP + clé Fluent (la même que celle du bot quand elle existe).

use axum::http::{header, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use std::time::Duration;

#[derive(Debug)]
pub struct ApiError {
    pub status: StatusCode,
    pub key: String,
    pub retry_after: Option<Duration>,
}

impl ApiError {
    pub fn new(status: StatusCode, key: impl Into<String>) -> Self { Self { status, key: key.into(), retry_after: None } }
    pub fn bad_request(key: &str) -> Self { Self::new(StatusCode::BAD_REQUEST, key) }
    pub fn unauthorized() -> Self { Self::new(StatusCode::UNAUTHORIZED, "activity__unauthorized") }
    pub fn forbidden(key: &str) -> Self { Self::new(StatusCode::FORBIDDEN, key) }
    pub fn not_found(key: &str) -> Self { Self::new(StatusCode::NOT_FOUND, key) }
    pub fn internal() -> Self { Self::new(StatusCode::INTERNAL_SERVER_ERROR, "activity__internal_error") }
    pub fn upstream() -> Self { Self::new(StatusCode::BAD_GATEWAY, "activity__discord_unavailable") }
    pub fn too_many(retry: Duration) -> Self {
        Self { status: StatusCode::TOO_MANY_REQUESTS, key: "activity__rate_limited".into(), retry_after: Some(retry) }
    }
}

impl From<mongodb::error::Error> for ApiError {
    fn from(e: mongodb::error::Error) -> Self {
        tracing::error!(error = %e, "activity: erreur base de données");
        ApiError::internal()
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let mut res = (self.status, Json(serde_json::json!({ "error": self.key }))).into_response();
        if let Some(r) = self.retry_after {
            let secs = r.as_secs_f64().ceil().max(1.0) as u64;
            if let Ok(v) = HeaderValue::from_str(&secs.to_string()) {
                res.headers_mut().insert(header::RETRY_AFTER, v);
            }
        }
        res
    }
}

pub type ApiResult<T> = Result<T, ApiError>;
