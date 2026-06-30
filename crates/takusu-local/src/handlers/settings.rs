use axum::Json;
use axum::extract::State;
use serde::Serialize;
use takusu_storage::{SettingsRow, UpdateSettings};

use crate::error::HttpError;
use crate::state::AppState;

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
