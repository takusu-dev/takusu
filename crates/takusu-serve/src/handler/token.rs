use crate::app::AppState;
use crate::auth::hash_token;
use crate::error::AppError;
use crate::model::*;
use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use uuid::Uuid;
pub async fn create_token(
    State(state): State<AppState>,
    Json(body): Json<CreateToken>,
) -> Result<(StatusCode, Json<serde_json::Value>), AppError> {
    let new_token = format!("tsk_{}", Uuid::now_v7());
    let hash = hash_token(&new_token);
    let label = body.label.clone().unwrap_or_default();
    sqlx::query(
        "INSERT INTO tokens (token_hash, label, created_by) VALUES (?, ?, 'authenticated')",
    )
    .bind(&hash)
    .bind(if label.is_empty() {
        None::<String>
    } else {
        body.label.clone()
    })
    .execute(&state.db)
    .await?;
    let row = sqlx::query_as::<_, TokenRow>("SELECT * FROM tokens WHERE token_hash = ?")
        .bind(&hash)
        .fetch_one(&state.db)
        .await?;
    Ok((
        StatusCode::CREATED,
        Json(serde_json::json!({
            "id": row.id,
            "token": new_token,
            "label": row.label,
            "created_at": row.created_at,
        })),
    ))
}
pub async fn list_tokens(State(state): State<AppState>) -> Result<Json<Vec<TokenRow>>, AppError> {
    let rows = sqlx::query_as::<_, TokenRow>("SELECT * FROM tokens ORDER BY created_at DESC")
        .fetch_all(&state.db)
        .await?;
    Ok(Json(rows))
}
pub async fn revoke_token(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<StatusCode, AppError> {
    let result = sqlx::query(
        "UPDATE tokens SET revoked_at = datetime('now') WHERE id = ? AND revoked_at IS NULL",
    )
    .bind(id)
    .execute(&state.db)
    .await?;
    if result.rows_affected() == 0 {
        return Err(AppError::NotFound(format!(
            "token {id} not found or already revoked"
        )));
    }
    Ok(StatusCode::NO_CONTENT)
}
