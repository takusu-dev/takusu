use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use serde::Deserialize;
use takusu_storage::{TokenCreateResponse, TokenRow};

use crate::error::HttpError;
use crate::state::AppState;

#[derive(Debug, Deserialize)]
pub struct CreateTokenRequest {
    pub label: Option<String>,
}

pub async fn create_token(
    State(state): State<AppState>,
    Json(body): Json<CreateTokenRequest>,
) -> Result<(StatusCode, Json<TokenCreateResponse>), HttpError> {
    let resp = state.app.create_token(body.label.as_deref()).await?;
    Ok((StatusCode::CREATED, Json(resp)))
}

pub async fn list_tokens(State(state): State<AppState>) -> Result<Json<Vec<TokenRow>>, HttpError> {
    let tokens = state.app.list_tokens().await?;
    Ok(Json(tokens))
}

pub async fn revoke_token(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<StatusCode, HttpError> {
    state.app.revoke_token(id).await?;
    Ok(StatusCode::NO_CONTENT)
}
