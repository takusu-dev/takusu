use axum::Json;
use axum::extract::{Extension, Path, State};
use axum::http::StatusCode;
use serde::Deserialize;
use takusu_local_lib::TokenClaims;
use takusu_storage::{TokenCreateResponse, TokenRow};

use crate::error::HttpError;
use crate::state::AppState;
use takusu_local_lib::error::AppError;

#[derive(Debug, Deserialize)]
pub struct CreateTokenRequest {
    pub label: Option<String>,
}

fn require_root(claims: &TokenClaims) -> Result<(), HttpError> {
    if !claims.is_root() {
        return Err(HttpError(AppError::Unauthorized));
    }
    Ok(())
}

pub async fn create_token(
    State(state): State<AppState>,
    Extension(claims): Extension<TokenClaims>,
    Json(body): Json<CreateTokenRequest>,
) -> Result<(StatusCode, Json<TokenCreateResponse>), HttpError> {
    require_root(&claims)?;
    let resp = state.app.create_token(body.label.as_deref()).await?;
    Ok((StatusCode::CREATED, Json(resp)))
}

pub async fn list_tokens(
    State(state): State<AppState>,
    Extension(claims): Extension<TokenClaims>,
) -> Result<Json<Vec<TokenRow>>, HttpError> {
    require_root(&claims)?;
    let tokens = state.app.list_tokens().await?;
    Ok(Json(tokens))
}

pub async fn revoke_token(
    State(state): State<AppState>,
    Extension(claims): Extension<TokenClaims>,
    Path(id): Path<i64>,
) -> Result<StatusCode, HttpError> {
    require_root(&claims)?;
    state.app.revoke_token(id).await?;
    Ok(StatusCode::NO_CONTENT)
}
