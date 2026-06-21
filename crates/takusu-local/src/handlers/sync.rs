use axum::Json;
use axum::extract::State;
use serde::Deserialize;
use takusu_storage::{GoogleCalEventRow, GoogleCalSettingsRow, UpdateGoogleCalSettings};

use crate::error::AppError;
use crate::handlers::task::storage_to_app;
use crate::state::AppState;

#[derive(Debug, Clone, serde::Serialize)]
pub struct GoogleCalSettingsResponse {
    pub enabled: bool,
    pub calendar_id: String,
    pub client_id: String,
    pub has_client_secret: bool,
    pub has_refresh_token: bool,
}

impl From<GoogleCalSettingsRow> for GoogleCalSettingsResponse {
    fn from(s: GoogleCalSettingsRow) -> Self {
        Self {
            enabled: s.enabled,
            calendar_id: s.calendar_id,
            client_id: s.client_id,
            has_client_secret: !s.client_secret.is_empty(),
            has_refresh_token: s.refresh_token.is_some(),
        }
    }
}

pub async fn get_settings(
    State(state): State<AppState>,
) -> Result<Json<GoogleCalSettingsResponse>, AppError> {
    let row = state
        .storage
        .get_gcal_settings()
        .await
        .map_err(storage_to_app)?;
    Ok(Json(row.into()))
}

pub async fn update_settings(
    State(state): State<AppState>,
    Json(body): Json<UpdateGoogleCalSettings>,
) -> Result<Json<GoogleCalSettingsResponse>, AppError> {
    let row = state
        .storage
        .update_gcal_settings(&body)
        .await
        .map_err(storage_to_app)?;
    Ok(Json(row.into()))
}

#[derive(Debug, Deserialize)]
pub struct OAuthUrlRequest {
    pub redirect_uri: String,
}

pub async fn oauth_url(
    State(state): State<AppState>,
    Json(body): Json<OAuthUrlRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    let row = state
        .storage
        .get_gcal_settings()
        .await
        .map_err(storage_to_app)?;
    if row.client_id.is_empty() {
        return Err(AppError::BadRequest(
            "google calendar settings not configured".into(),
        ));
    }
    let url = google_cal::oauth_url(&row.client_id, &body.redirect_uri);
    Ok(Json(serde_json::json!({ "url": url })))
}

#[derive(Debug, Deserialize)]
pub struct OAuthCallbackRequest {
    pub code: String,
    pub redirect_uri: String,
}

pub async fn oauth_callback(
    State(state): State<AppState>,
    Json(body): Json<OAuthCallbackRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    let row = state
        .storage
        .get_gcal_settings()
        .await
        .map_err(storage_to_app)?;
    if row.client_id.is_empty() || row.client_secret.is_empty() {
        return Err(AppError::BadRequest(
            "google calendar settings not configured".into(),
        ));
    }
    let tokens = google_cal::exchange_code(
        &row.client_id,
        &row.client_secret,
        &body.code,
        &body.redirect_uri,
    )
    .await
    .map_err(|e| AppError::Internal(format!("oauth exchange failed: {e}")))?;
    state
        .storage
        .update_gcal_settings(&UpdateGoogleCalSettings {
            enabled: None,
            calendar_id: None,
            client_id: None,
            client_secret: None,
            refresh_token: Some(tokens.refresh_token),
        })
        .await
        .map_err(storage_to_app)?;
    Ok(Json(serde_json::json!({ "refresh_token_set": true })))
}

pub async fn trigger_sync(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, AppError> {
    if let Err(e) = crate::handlers::schedule::do_sync(&state).await {
        tracing::error!("google calendar sync failed: {e}");
    }
    Ok(Json(serde_json::json!({ "status": "sync_triggered" })))
}

pub async fn list_mappings(
    State(state): State<AppState>,
) -> Result<Json<Vec<GoogleCalEventRow>>, AppError> {
    let rows = state
        .storage
        .list_gcal_mappings()
        .await
        .map_err(storage_to_app)?;
    Ok(Json(rows))
}
