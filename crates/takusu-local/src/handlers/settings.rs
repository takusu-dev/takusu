use axum::Json;
use axum::extract::State;
use takusu_storage::{SettingsRow, UpdateSettings};

use crate::error::HttpError;
use crate::state::AppState;

pub async fn get_settings(
    State(state): State<AppState>,
) -> Result<Json<SettingsRow>, HttpError> {
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
