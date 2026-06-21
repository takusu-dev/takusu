use axum::Json;
use axum::extract::State;
use takusu_storage::{SettingsRow, UpdateSettings};

use crate::error::AppError;
use crate::handlers::task::storage_to_app;
use crate::state::AppState;

pub async fn get_settings(State(state): State<AppState>) -> Result<Json<SettingsRow>, AppError> {
    let row = state.storage.get_settings().await.map_err(storage_to_app)?;
    Ok(Json(row))
}

pub async fn update_settings(
    State(state): State<AppState>,
    Json(body): Json<UpdateSettings>,
) -> Result<Json<SettingsRow>, AppError> {
    let row = state
        .storage
        .update_settings(&body)
        .await
        .map_err(storage_to_app)?;
    Ok(Json(row))
}
