use wasm_bindgen::JsValue;
use worker::Env;
use worker::Response;

use crate::error::WorkerError;
use crate::handlers::auth::db;
use crate::handlers::d1::safe_all;
use crate::handlers::tokens::{json_created, json_ok, parse_json};
use crate::models::{CreateHabit, HabitRow, UpdateHabit};

const HABIT_COLS: &str = "id, title, description, recurrence, start_time, end_time, avg_minutes, sigma_minutes, parallelizable, allows_parallel, abandonability, active, fixed, created_at, updated_at";

fn select_habits() -> String {
    format!("SELECT {HABIT_COLS} FROM habits")
}

pub async fn list(_req: worker::Request, env: Env) -> Result<Response, WorkerError> {
    let database = db(&env)?;
    let stmt = database.prepare(format!(
        "{select} ORDER BY created_at DESC",
        select = select_habits()
    ));
    let rows: Vec<HabitRow> = safe_all(&stmt).await?;
    json_ok(&rows)
}

pub async fn create(mut req: worker::Request, env: Env) -> Result<Response, WorkerError> {
    let body: CreateHabit = parse_json(&mut req).await?;
    let database = db(&env)?;
    let id = uuid::Uuid::now_v7().to_string();
    let sigma = body.sigma_minutes.unwrap_or((body.avg_minutes / 5).max(1));
    let parallelizable = body.parallelizable.unwrap_or(false);
    let allows_parallel = body.allows_parallel.unwrap_or(false);
    let abandonability = body.abandonability.unwrap_or(0.5);
    let fixed = body.fixed.unwrap_or(false);

    let stmt = database.prepare(
        "INSERT INTO habits (id, title, description, recurrence, start_time, end_time, avg_minutes, sigma_minutes, parallelizable, allows_parallel, abandonability, active, fixed) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, 1, ?12)"
    );
    stmt.bind(&[
        JsValue::from_str(&id),
        JsValue::from_str(&body.title),
        body.description
            .as_deref()
            .map(JsValue::from_str)
            .unwrap_or(JsValue::NULL),
        JsValue::from_str(&body.recurrence),
        JsValue::from_str(&body.start_time),
        JsValue::from_str(&body.end_time),
        JsValue::from_f64(body.avg_minutes as f64),
        JsValue::from_f64(sigma as f64),
        JsValue::from_bool(parallelizable),
        JsValue::from_bool(allows_parallel),
        JsValue::from_f64(abandonability),
        JsValue::from_bool(fixed),
    ])?
    .run()
    .await
    .map_err(WorkerError::Worker)?;

    let row = select_one(&database, &id).await?;
    json_created(&row)
}

pub async fn get(_req: worker::Request, env: Env, id: &str) -> Result<Response, WorkerError> {
    let database = db(&env)?;
    let row = select_one(&database, id).await?;
    json_ok(&row)
}

pub async fn update(mut req: worker::Request, env: Env, id: &str) -> Result<Response, WorkerError> {
    let body: UpdateHabit = parse_json(&mut req).await?;
    let database = db(&env)?;
    let stmt = database.prepare(
        "UPDATE habits SET title=COALESCE(?1,title), description=COALESCE(?2,description), recurrence=COALESCE(?3,recurrence), start_time=COALESCE(?4,start_time), end_time=COALESCE(?5,end_time), avg_minutes=COALESCE(?6,avg_minutes), sigma_minutes=COALESCE(?7,sigma_minutes), parallelizable=COALESCE(?8,parallelizable), allows_parallel=COALESCE(?9,allows_parallel), abandonability=COALESCE(?10,abandonability), active=COALESCE(?11,active), fixed=COALESCE(?12,fixed), updated_at=datetime('now') WHERE id = ?13"
    );
    stmt.bind(&[
        body.title
            .as_deref()
            .map(JsValue::from_str)
            .unwrap_or(JsValue::NULL),
        body.description
            .as_deref()
            .map(JsValue::from_str)
            .unwrap_or(JsValue::NULL),
        body.recurrence
            .as_deref()
            .map(JsValue::from_str)
            .unwrap_or(JsValue::NULL),
        body.start_time
            .as_deref()
            .map(JsValue::from_str)
            .unwrap_or(JsValue::NULL),
        body.end_time
            .as_deref()
            .map(JsValue::from_str)
            .unwrap_or(JsValue::NULL),
        body.avg_minutes
            .map(|n| JsValue::from_f64(n as f64))
            .unwrap_or(JsValue::NULL),
        body.sigma_minutes
            .map(|n| JsValue::from_f64(n as f64))
            .unwrap_or(JsValue::NULL),
        body.parallelizable
            .map(JsValue::from_bool)
            .unwrap_or(JsValue::NULL),
        body.allows_parallel
            .map(JsValue::from_bool)
            .unwrap_or(JsValue::NULL),
        body.abandonability
            .map(JsValue::from_f64)
            .unwrap_or(JsValue::NULL),
        body.active.map(JsValue::from_bool).unwrap_or(JsValue::NULL),
        body.fixed.map(JsValue::from_bool).unwrap_or(JsValue::NULL),
        JsValue::from_str(id),
    ])?
    .run()
    .await
    .map_err(WorkerError::Worker)?;

    let row = select_one(&database, id).await?;
    json_ok(&row)
}

pub async fn replace(
    mut req: worker::Request,
    env: Env,
    id: &str,
) -> Result<Response, WorkerError> {
    let body: CreateHabit = parse_json(&mut req).await?;
    let database = db(&env)?;
    let sigma = body.sigma_minutes.unwrap_or((body.avg_minutes / 5).max(1));
    let parallelizable = body.parallelizable.unwrap_or(false);
    let allows_parallel = body.allows_parallel.unwrap_or(false);
    let abandonability = body.abandonability.unwrap_or(0.5);
    let fixed = body.fixed.unwrap_or(false);

    let stmt = database.prepare(
        "UPDATE habits SET title=?1, description=?2, recurrence=?3, start_time=?4, end_time=?5, avg_minutes=?6, sigma_minutes=?7, parallelizable=?8, allows_parallel=?9, abandonability=?10, fixed=?11, updated_at=datetime('now') WHERE id = ?12"
    );
    stmt.bind(&[
        JsValue::from_str(&body.title),
        body.description
            .as_deref()
            .map(JsValue::from_str)
            .unwrap_or(JsValue::NULL),
        JsValue::from_str(&body.recurrence),
        JsValue::from_str(&body.start_time),
        JsValue::from_str(&body.end_time),
        JsValue::from_f64(body.avg_minutes as f64),
        JsValue::from_f64(sigma as f64),
        JsValue::from_bool(parallelizable),
        JsValue::from_bool(allows_parallel),
        JsValue::from_f64(abandonability),
        JsValue::from_bool(fixed),
        JsValue::from_str(id),
    ])?
    .run()
    .await
    .map_err(WorkerError::Worker)?;

    let row = select_one(&database, id).await?;
    json_ok(&row)
}

pub async fn delete(_req: worker::Request, env: Env, id: &str) -> Result<Response, WorkerError> {
    let database = db(&env)?;
    // Delete tasks referencing this habit before deleting the habit,
    // so D1's foreign-key constraint does not block deletion of habits
    // that have already generated tasks (#240). The client confirms
    // with the user before issuing the delete when there are
    // associated tasks. All statements run in a single batch() call
    // so D1 executes them atomically (matching the sqlite transaction
    // in storage_sqlite.rs) — a partial failure cannot leave the
    // database with tasks deleted but the habit still present.
    // google_cal_events mappings are cleaned up explicitly; D1
    // enforces FKs so the ON DELETE CASCADE would handle it, but the
    // explicit delete is harmless and keeps parity with the sqlite
    // path (which does not enable PRAGMA foreign_keys).
    let stmts = vec![
        database.prepare("DELETE FROM google_cal_events WHERE task_id IN (SELECT id FROM tasks WHERE habit_id = ?1)").bind(&[JsValue::from_str(id)])?,
        database
            .prepare("DELETE FROM tasks WHERE habit_id = ?1")
            .bind(&[JsValue::from_str(id)])?,
        database
            .prepare("DELETE FROM habits WHERE id = ?1")
            .bind(&[JsValue::from_str(id)])?,
    ];
    database.batch(stmts).await.map_err(WorkerError::Worker)?;
    Ok(Response::empty()?)
}

pub async fn select_one(database: &worker::D1Database, id: &str) -> Result<HabitRow, WorkerError> {
    let stmt = database.prepare(format!("{select} WHERE id = ?1", select = select_habits()));
    let row: Option<HabitRow> = stmt
        .bind(&[JsValue::from_str(id)])?
        .first(None)
        .await
        .map_err(WorkerError::Worker)?;
    row.ok_or_else(|| WorkerError::NotFound(format!("habit {id} not found")))
}
