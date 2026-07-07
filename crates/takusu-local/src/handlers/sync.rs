use axum::Json;
use axum::extract::State;
use serde::Deserialize;
use takusu_local_lib::app::GoogleCalSettingsOutput;
use takusu_local_lib::error::AppError;
use takusu_storage::{GoogleCalEventRow, UpdateGoogleCalSettings};

use crate::error::HttpError;
use crate::state::AppState;

#[derive(Debug, Deserialize)]
pub struct OAuthUrlRequest {
    pub redirect_uri: String,
}

#[derive(Debug, Deserialize)]
pub struct OAuthCallbackRequest {
    pub code: String,
    pub redirect_uri: Option<String>,
}

pub async fn get_settings(
    State(state): State<AppState>,
) -> Result<Json<GoogleCalSettingsOutput>, HttpError> {
    let output = state.app.get_gcal_settings().await?;
    Ok(Json(output))
}

pub async fn update_settings(
    State(state): State<AppState>,
    Json(body): Json<UpdateGoogleCalSettings>,
) -> Result<Json<GoogleCalSettingsOutput>, HttpError> {
    let output = state.app.update_gcal_settings(&body).await?;
    Ok(Json(output))
}

pub async fn oauth_url(
    State(state): State<AppState>,
    Json(body): Json<OAuthUrlRequest>,
) -> Result<Json<serde_json::Value>, HttpError> {
    let url = state.app.oauth_url(&body.redirect_uri).await?;
    Ok(Json(serde_json::json!({ "url": url })))
}

pub async fn oauth_callback(
    State(state): State<AppState>,
    Json(body): Json<OAuthCallbackRequest>,
) -> Result<Json<serde_json::Value>, HttpError> {
    state
        .app
        .oauth_callback(&body.code, body.redirect_uri.as_deref())
        .await?;
    Ok(Json(serde_json::json!({ "refresh_token_set": true })))
}

pub async fn trigger_sync(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, HttpError> {
    state.app.do_sync().await.map_err(|e| {
        tracing::error!("google calendar sync failed: {e}");
        HttpError::from(AppError::Internal(format!("sync failed: {e}")))
    })?;
    Ok(Json(serde_json::json!({ "status": "sync_triggered" })))
}

pub async fn list_mappings(
    State(state): State<AppState>,
) -> Result<Json<Vec<GoogleCalEventRow>>, HttpError> {
    let rows = state.app.list_gcal_mappings().await?;
    Ok(Json(rows))
}
