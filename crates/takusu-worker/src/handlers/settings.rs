use wasm_bindgen::JsValue;
use worker::Env;
use worker::Response;

use crate::error::WorkerError;
use crate::handlers::auth::db;
use crate::handlers::d1::safe_all;
use crate::handlers::tokens::{json_ok, parse_json};
use crate::models::{SettingsRow, UpdateSettings};

pub async fn get(_req: worker::Request, env: Env) -> Result<Response, WorkerError> {
    let database = db(&env)?;
    let row = get_inner(&database).await?;
    json_ok(&row)
}

pub async fn update(mut req: worker::Request, env: Env) -> Result<Response, WorkerError> {
    let body: UpdateSettings = parse_json(&mut req).await?;
    let database = db(&env)?;
    let existing = get_inner(&database).await?;
    let tz = body.tz.clone().unwrap_or(existing.tz);
    let sleep_start = body.sleep_start.clone().unwrap_or(existing.sleep_start);
    let sleep_end = body.sleep_end.clone().unwrap_or(existing.sleep_end);
    let stmt = database.prepare(
        "UPDATE settings SET tz = ?1, sleep_start = ?2, sleep_end = ?3, updated_at = datetime('now') WHERE id = 'active'",
    );
    stmt.bind(&[
        JsValue::from_str(&tz),
        JsValue::from_str(&sleep_start),
        JsValue::from_str(&sleep_end),
    ])?
    .run()
    .await
    .map_err(WorkerError::Worker)?;
    let row = get_inner(&database).await?;
    json_ok(&row)
}

async fn get_inner(database: &worker::D1Database) -> Result<SettingsRow, WorkerError> {
    let stmt = database
        .prepare("SELECT id, tz, sleep_start, sleep_end, created_at, updated_at FROM settings WHERE id = 'active'");
    let rows: Vec<SettingsRow> = safe_all(&stmt).await?;
    rows.into_iter()
        .next()
        .ok_or_else(|| WorkerError::NotFound("settings not found".into()))
}
