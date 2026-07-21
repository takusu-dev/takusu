use std::sync::Arc;

use axum::Json;
use axum::extract::{Extension, State};
use serde::{Deserialize, Serialize};
use takusu_storage::{SettingsRow, UpdateSettings};

use crate::error::HttpError;
use crate::state::AppState;
use takusu_local_lib::error::AppError;

pub async fn get_settings(State(state): State<AppState>) -> Result<Json<SettingsRow>, HttpError> {
    let row = state.app.get_settings().await?;
    Ok(Json(row))
}

pub async fn update_settings(
    State(state): State<AppState>,
    Json(body): Json<UpdateSettings>,
) -> Result<Json<SettingsRow>, HttpError> {
    let row = state.app.update_settings(&body).await?;
    Ok(Json(row))
}

#[derive(Serialize)]
pub struct HealthCheckResponse {
    pub status: String,
}

/// `GET /api/workers/health` — checks the storage backend (Cloudflare Worker
/// or local SQLite) is reachable. Used by the mobile settings page.
pub async fn workers_health(
    State(state): State<AppState>,
) -> Result<Json<HealthCheckResponse>, HttpError> {
    let status = state.app.health_check().await?;
    Ok(Json(HealthCheckResponse { status }))
}

#[derive(Deserialize)]
pub struct UpdateWorkersConfig {
    pub url: String,
    pub token: String,
}

/// `PUT /api/workers/config` — updates the Worker endpoint and root token
/// at runtime without restarting the server. Requires the current root token.
pub async fn update_workers_config(
    State(state): State<AppState>,
    Extension(token): Extension<String>,
    Json(body): Json<UpdateWorkersConfig>,
) -> Result<Json<serde_json::Value>, HttpError> {
    if body.url.trim().is_empty() {
        return Err(HttpError(AppError::BadRequest(
            "workers url is required".into(),
        )));
    }
    if body.token.trim().is_empty() {
        return Err(HttpError(AppError::BadRequest(
            "workers token is required".into(),
        )));
    }
    let app = state.app.clone();
    let mut root_token = state.token.write().await;
    if root_token.is_empty() || token != root_token.as_ref() {
        return Err(HttpError(AppError::Unauthorized));
    }
    app.update_workers_credentials(&body.url, &body.token)
        .await?;
    *root_token = Arc::from(body.token.into_boxed_str());
    Ok(Json(serde_json::json!({ "ok": true })))
}
