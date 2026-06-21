use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use serde::Deserialize;
use takusu_storage::{TokenCreateResponse, TokenRow};

use crate::error::AppError;
use crate::handlers::task::storage_to_app;
use crate::state::AppState;

#[derive(Debug, Deserialize)]
pub struct CreateTokenRequest {
    pub label: Option<String>,
}

pub async fn create_token(
    State(state): State<AppState>,
    Json(body): Json<CreateTokenRequest>,
) -> Result<(StatusCode, Json<TokenCreateResponse>), AppError> {
    let resp = state
        .storage
        .create_token(body.label.as_deref())
        .await
        .map_err(storage_to_app)?;
    state.token_cache.invalidate();
    Ok((StatusCode::CREATED, Json(resp)))
}

pub async fn list_tokens(State(state): State<AppState>) -> Result<Json<Vec<TokenRow>>, AppError> {
    let tokens = state.storage.list_tokens().await.map_err(storage_to_app)?;
    Ok(Json(tokens))
}

pub async fn revoke_token(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<StatusCode, AppError> {
    state
        .storage
        .revoke_token(id)
        .await
        .map_err(storage_to_app)?;
    state.token_cache.invalidate();
    Ok(StatusCode::NO_CONTENT)
}
