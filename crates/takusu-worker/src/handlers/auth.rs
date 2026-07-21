use worker::{D1Database, Env, Request, Response};

use crate::auth;
use crate::error::WorkerError;

pub async fn require_auth(req: &Request, env: &Env) -> Result<(), WorkerError> {
    let claims = auth::verify_token(req, env)?;

    // Root tokens are self-authenticating; regular tokens must not be revoked.
    if claims.is_root() {
        return Ok(());
    }

    let db = db(env)?;
    let stmt = worker::query!(
        &db,
        "SELECT COUNT(*) AS c FROM tokens WHERE jti = ?1 AND revoked_at IS NULL",
        claims.jti
    )?;
    let row: Option<CountRow> = stmt.first(None).await.map_err(WorkerError::Worker)?;
    if row.map(|r| r.c > 0).unwrap_or(false) {
        Ok(())
    } else {
        Err(WorkerError::Unauthorized)
    }
}

pub async fn verify(req: Request, env: Env) -> Result<Response, WorkerError> {
    let claims = auth::verify_token(&req, &env)?;
    let body = serde_json::to_string(&claims)
        .map_err(|e| WorkerError::Internal(format!("json error: {e}")))?;
    let mut resp = Response::ok(body)?;
    resp.headers_mut()
        .set("content-type", "application/json")
        .map_err(WorkerError::Worker)?;
    Ok(resp)
}

pub fn is_root(req: &Request, env: &Env) -> Result<bool, WorkerError> {
    auth::is_root(req, env)
}

pub fn db(env: &Env) -> Result<D1Database, WorkerError> {
    env.d1("DB")
        .map_err(|e| WorkerError::Internal(format!("D1 binding 'DB' missing: {e}")))
}

#[derive(serde::Deserialize)]
struct CountRow {
    #[serde(rename = "c")]
    c: i64,
}
