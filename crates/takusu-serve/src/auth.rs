use axum::extract::State;
use axum::http::Request;
use axum::middleware::Next;
use axum::response::Response;
use sha2::{Digest, Sha256};

use crate::app::AppState;
use crate::error::AppError;

pub async fn auth_middleware(
    state: State<AppState>,
    req: Request<axum::body::Body>,
    next: Next,
) -> Result<Response, AppError> {
    let auth_header = req
        .headers()
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .ok_or(AppError::Unauthorized)?;

    let token = auth_header
        .strip_prefix("Bearer ")
        .ok_or(AppError::Unauthorized)?;

    if token == state.root_token {
        return Ok(next.run(req).await);
    }

    let hash = hash_token(token);
    let count: i64 = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM tokens WHERE token_hash = ? AND revoked_at IS NULL",
    )
    .bind(&hash)
    .fetch_one(&state.db)
    .await
    .map_err(|e: sqlx::Error| AppError::Internal(e.to_string()))?;

    if count > 0 {
        Ok(next.run(req).await)
    } else {
        Err(AppError::Unauthorized)
    }
}

pub fn hash_token(token: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(token.as_bytes());
    let result = hasher.finalize();
    result.iter().map(|b| format!("{:02x}", b)).collect()
}
