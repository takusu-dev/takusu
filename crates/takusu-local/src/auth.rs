use axum::extract::{Request, State};
use axum::http::header;
use axum::middleware::Next;
use axum::response::Response;

use crate::error::HttpError;
use crate::state::AppState;
use takusu_local_lib::error::AppError;
use takusu_local_lib::{DEFAULT_AUD, DEFAULT_ISS, SCOPE_ROOT, TokenClaims};

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

    // If the request presents the server's configured root token, accept it
    // as root without asking the storage backend. This lets root-only routes
    // like `PUT /api/workers/config` succeed while the worker endpoint or
    // token is being changed, and while the worker itself is unreachable.
    let is_root_token = {
        let root = state.root_token.read().await;
        !root.is_empty() && token == &**root
    };
    if is_root_token {
        let jti = takusu_local_lib::auth::hash_token(token);
        let claims = TokenClaims {
            sub: jti.clone(),
            jti,
            scope: SCOPE_ROOT.to_string(),
            label: None,
            aud: DEFAULT_AUD.to_string(),
            iss: DEFAULT_ISS.to_string(),
            iat: 0,
            exp: None,
        };
        req.extensions_mut().insert(claims);
        return Ok(next.run(req).await);
    }

    let claims = takusu_local_lib::auth::verify_token_with_cache(
        token,
        state.app.storage.as_ref(),
        &state.app.token_cache,
    )
    .await
    .map_err(|e| HttpError(AppError::Internal(e.to_string())))?;

    if let Some(claims) = claims {
        req.extensions_mut().insert(claims);
        Ok(next.run(req).await)
    } else {
        Err(HttpError(AppError::Unauthorized))
    }
}
