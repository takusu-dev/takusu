use axum::extract::{Request, State};
use axum::http::header;
use axum::middleware::Next;
use axum::response::Response;

use crate::error::HttpError;
use crate::state::AppState;
use takusu_local_lib::error::AppError;

pub async fn auth_middleware(
    State(state): State<AppState>,
    mut req: Request,
    next: Next,
) -> Result<Response, HttpError> {
    let token = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .ok_or(HttpError(AppError::Unauthorized))?;

    if !state.root_token.is_empty() && token == state.root_token {
        let token = token.to_string();
        req.extensions_mut().insert(token);
        return Ok(next.run(req).await);
    }

    let valid = takusu_local_lib::auth::verify_token_with_cache(
        token,
        state.app.storage.as_ref(),
        &state.app.token_cache,
    )
    .await
    .map_err(|e| HttpError(AppError::Internal(e.to_string())))?;

    if valid {
        let token = token.to_string();
        req.extensions_mut().insert(token);
        Ok(next.run(req).await)
    } else {
        Err(HttpError(AppError::Unauthorized))
    }
}
