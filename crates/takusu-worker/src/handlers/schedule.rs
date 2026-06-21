use wasm_bindgen::JsValue;
use worker::D1PreparedStatement;
use worker::Env;
use worker::Response;

use crate::error::WorkerError;
use crate::handlers::auth::db;
use crate::handlers::tokens::{json_created, json_ok, parse_json};
use crate::models::{SaveScheduleRequest, ScheduleRow};

pub async fn get(_req: worker::Request, env: Env) -> Result<Response, WorkerError> {
    let database = db(&env)?;
    let result = database
        .prepare("SELECT id, created_at, updated_at, schedule FROM schedules WHERE id = 'active'")
        .all()
        .await
        .map_err(WorkerError::Worker)?;
    let rows: Vec<ScheduleRow> = result.results().map_err(WorkerError::Worker)?;
    match rows.into_iter().next() {
        Some(row) => json_ok(&row),
        None => Err(WorkerError::NotFound("no active schedule".into())),
    }
}

pub async fn save(mut req: worker::Request, env: Env) -> Result<Response, WorkerError> {
    let body: SaveScheduleRequest = parse_json(&mut req).await?;
    let database = db(&env)?;
    let schedule_json = serde_json::to_string(&body.entries)
        .map_err(|e| WorkerError::Internal(format!("serialize schedule: {e}")))?;

    let mut stmts: Vec<D1PreparedStatement> =
        Vec::with_capacity(1 + body.mark_scheduled_task_ids.len());
    let upsert = database
        .prepare(
            "INSERT INTO schedules (id, created_at, updated_at, schedule) VALUES ('active', datetime('now'), datetime('now'), ?1) ON CONFLICT(id) DO UPDATE SET schedule=excluded.schedule, updated_at=excluded.updated_at"
        )
        .bind(&[JsValue::from_str(&schedule_json)])
        .map_err(WorkerError::Worker)?;
    stmts.push(upsert);

    for id in &body.mark_scheduled_task_ids {
        let stmt = database
            .prepare(
                "UPDATE tasks SET status = 'scheduled', updated_at = datetime('now') WHERE id = ?1",
            )
            .bind(&[JsValue::from_str(id)])
            .map_err(WorkerError::Worker)?;
        stmts.push(stmt);
    }

    database.batch(stmts).await.map_err(WorkerError::Worker)?;

    let result = database
        .prepare("SELECT id, created_at, updated_at, schedule FROM schedules WHERE id = 'active'")
        .all()
        .await
        .map_err(WorkerError::Worker)?;
    let rows: Vec<ScheduleRow> = result.results().map_err(WorkerError::Worker)?;
    let row = rows
        .into_iter()
        .next()
        .ok_or_else(|| WorkerError::Internal("schedule not found after save".into()))?;
    json_created(&row)
}

pub async fn clear(_req: worker::Request, env: Env) -> Result<Response, WorkerError> {
    let database = db(&env)?;
    let stmt = database.prepare("DELETE FROM schedules WHERE id = 'active'");
    stmt.run().await.map_err(WorkerError::Worker)?;
    Ok(Response::empty()?)
}
