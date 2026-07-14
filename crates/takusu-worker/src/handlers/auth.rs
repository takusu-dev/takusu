use worker::{D1Database, Env, Request, Response};

use crate::auth;
use crate::error::WorkerError;

pub async fn require_auth(req: &Request, env: &Env) -> Result<(), WorkerError> {
    let header = req
        .headers()
        .get("authorization")
        .map_err(|e| WorkerError::Internal(format!("header read: {e}")))?;
    let token = header
        .as_deref()
        .and_then(|v| v.strip_prefix("Bearer "))
        .ok_or(WorkerError::Unauthorized)?;

    if verify_root(token, env) {
        return Ok(());
    }
    let db = db(env)?;
    let hash = auth::hash_token(token);
    let stmt = worker::query!(
        &db,
        "SELECT COUNT(*) AS c FROM tokens WHERE token_hash = ?1 AND revoked_at IS NULL",
        hash
    )?;
    let row: Option<CountRow> = stmt.first(None).await.map_err(WorkerError::Worker)?;
    if row.map(|r| r.c > 0).unwrap_or(false) {
        Ok(())
    } else {
        Err(WorkerError::Unauthorized)
    }
}

pub async fn verify(req: Request, env: Env) -> Result<Response, WorkerError> {
    require_auth(&req, &env).await?;
    ok()
}

pub fn is_root(req: &Request, env: &Env) -> Result<bool, WorkerError> {
    let header = req
        .headers()
        .get("authorization")
        .map_err(|e| WorkerError::Internal(format!("header read: {e}")))?;
    let token = header
        .as_deref()
        .and_then(|v| v.strip_prefix("Bearer "))
        .ok_or(WorkerError::Unauthorized)?;
    Ok(verify_root(token, env))
}

fn verify_root(token: &str, env: &Env) -> bool {
    match auth::root_token(env) {
        Ok(root) => token == root,
        Err(_) => false,
    }
}

fn ok() -> Result<Response, WorkerError> {
    let mut resp = Response::ok("")?;
    resp.headers_mut()
        .set("content-type", "text/plain")
        .map_err(WorkerError::Worker)?;
    Ok(resp)
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
