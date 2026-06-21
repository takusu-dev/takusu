use axum::extract::{Request, State};
use axum::http::header;
use axum::middleware::Next;
use axum::response::Response;
use sha2::{Digest, Sha256};

use crate::error::AppError;
use crate::state::AppState;
use crate::token_cache::TokenState;

pub async fn auth_middleware(
    State(state): State<AppState>,
    req: Request,
    next: Next,
) -> Result<Response, AppError> {
    let token = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .ok_or(AppError::Unauthorized)?;

    if token == state.root_token {
        return Ok(next.run(req).await);
    }

    match state.token_cache.get(token) {
        Some(TokenState::Valid) => return Ok(next.run(req).await),
        Some(TokenState::Invalid) => return Err(AppError::Unauthorized),
        None => {}
    }

    let valid = state
        .storage
        .verify_token(token)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;

    if valid {
        state.token_cache.put(token, TokenState::Valid);
        Ok(next.run(req).await)
    } else {
        state.token_cache.put(token, TokenState::Invalid);
        Err(AppError::Unauthorized)
    }
}

pub fn hash_token(token: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(token.as_bytes());
    let result = hasher.finalize();
    result.iter().map(|b| format!("{b:02x}")).collect()
}
