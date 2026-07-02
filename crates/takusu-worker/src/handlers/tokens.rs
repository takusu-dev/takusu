use worker::{Env, Request, Response};

use crate::auth;
use crate::error::WorkerError;
use crate::handlers::auth::db;
use crate::handlers::d1::safe_all;
use crate::models::{TokenCreateResponse, TokenRow};

#[derive(serde::Deserialize)]
pub struct CreateTokenBody {
    pub label: Option<String>,
}

pub async fn create(mut req: Request, env: Env) -> Result<Response, WorkerError> {
    let body: CreateTokenBody = parse_json(&mut req).await?;
    let label_str = body.label.clone().unwrap_or_default();
    let label_opt: Option<String> = if label_str.is_empty() {
        None
    } else {
        Some(label_str)
    };

    let new_token = auth::new_token();
    let hash = auth::hash_token(&new_token);
    let database = db(&env)?;

    let stmt = worker::query!(
        &database,
        "INSERT INTO tokens (token_hash, label, created_by) VALUES (?1, ?2, 'authenticated')",
        hash,
        label_opt
    )?;
    stmt.run().await.map_err(WorkerError::Worker)?;

    let lookup = worker::query!(
        &database,
        "SELECT id, token_hash, label, created_by, created_at, revoked_at FROM tokens WHERE token_hash = ?1",
        hash
    )?;
    let row: Option<TokenRow> = lookup.first(None).await.map_err(WorkerError::Worker)?;
    let row = row.ok_or_else(|| WorkerError::Internal("inserted token not found".into()))?;
    let resp = TokenCreateResponse {
        id: row.id,
        token: new_token,
        label: row.label,
        created_at: row.created_at,
    };
    json_created(&resp)
}

pub async fn list(_req: Request, env: Env) -> Result<Response, WorkerError> {
    let database = db(&env)?;
    let stmt = worker::query!(
        &database,
        "SELECT id, token_hash, label, created_by, created_at, revoked_at FROM tokens ORDER BY created_at DESC"
    );
    let rows: Vec<TokenRow> = safe_all(&stmt).await?;
    json_ok(&rows)
}

pub async fn revoke(_req: Request, env: Env, id: &str) -> Result<Response, WorkerError> {
    let id_num: i64 = id
        .parse()
        .map_err(|_| WorkerError::BadRequest(format!("invalid token id: {id}")))?;
    let database = db(&env)?;
    let stmt = worker::query!(
        &database,
        "UPDATE tokens SET revoked_at = datetime('now') WHERE id = ?1 AND revoked_at IS NULL",
        id_num
    )?;
    let result = stmt.run().await.map_err(WorkerError::Worker)?;
    let affected = result
        .meta()
        .map_err(WorkerError::Worker)?
        .and_then(|m| m.rows_written)
        .unwrap_or(0);
    if affected == 0 {
        return Err(WorkerError::NotFound(format!(
            "token {id} not found or already revoked"
        )));
    }
    Ok(Response::empty()?)
}

pub async fn parse_json<T: serde::de::DeserializeOwned>(
    req: &mut Request,
) -> Result<T, WorkerError> {
    let text = req.text().await.map_err(WorkerError::Worker)?;
    serde_json::from_str(&text).map_err(|e| WorkerError::BadRequest(format!("invalid json: {e}")))
}

pub fn json_ok<T: serde::Serialize>(value: &T) -> Result<Response, WorkerError> {
    Response::from_json(value).map_err(WorkerError::Worker)
}

pub fn json_created<T: serde::Serialize>(value: &T) -> Result<Response, WorkerError> {
    let body = serde_json::to_string(value).map_err(|e| WorkerError::Internal(e.to_string()))?;
    let mut resp = Response::ok(body)?;
    resp.headers_mut()
        .set("content-type", "application/json")
        .map_err(WorkerError::Worker)?;
    Ok(resp)
}
