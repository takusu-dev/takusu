use wasm_bindgen::JsValue;
use worker::Env;
use worker::Response;

use crate::error::WorkerError;
use crate::handlers::auth::db;
use crate::handlers::d1::safe_all;
use crate::handlers::tokens::{json_created, json_ok, parse_json};
use crate::models::{CreateHabit, CreateHabitPause, HabitPauseRow, HabitRow, UpdateHabit};
use crate::validate::{validate_minutes, validate_pause_dates, validate_recurrence};

const HABIT_COLS: &str = "id, display_id, title, description, recurrence, start_time, end_time, avg_minutes, sigma_minutes, parallelizable, allows_parallel, abandonability, active, fixed, created_at, updated_at";

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
    validate_minutes(body.avg_minutes, body.sigma_minutes)?;
    validate_recurrence(&body.recurrence)?;
    let database = db(&env)?;
    let id = uuid::Uuid::now_v7().to_string();
    let sigma = body.sigma_minutes.unwrap_or((body.avg_minutes / 5).max(1));
    let parallelizable = body.parallelizable.unwrap_or(false);
    let allows_parallel = body.allows_parallel.unwrap_or(false);
    let abandonability = body.abandonability.unwrap_or(0.5);
    let fixed = body.fixed.unwrap_or(false);

    // Atomically reserve a monotonic display_id from the sequence table
    // (mirrors tasks.display_id, issue #186 / #305).
    let seq_stmt = database.prepare(
        "UPDATE habit_display_id_seq SET next_id = next_id + 1 RETURNING next_id - 1 AS display_id",
    );
    let seq_row: Option<DisplayIdRow> = seq_stmt.first(None).await.map_err(WorkerError::Worker)?;
    let display_id = seq_row
        .ok_or_else(|| WorkerError::Internal("habit display_id sequence is empty".into()))?
        .display_id;

    let stmt = database.prepare(
        "INSERT INTO habits (id, display_id, title, description, recurrence, start_time, end_time, avg_minutes, sigma_minutes, parallelizable, allows_parallel, abandonability, active, fixed) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, 1, ?13)"
    );
    stmt.bind(&[
        JsValue::from_str(&id),
        JsValue::from_f64(display_id as f64),
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
    let full = resolve_habit_id(&database, id).await?;
    let row = select_one(&database, &full).await?;
    json_ok(&row)
}

pub async fn update(mut req: worker::Request, env: Env, id: &str) -> Result<Response, WorkerError> {
    let body: UpdateHabit = parse_json(&mut req).await?;
    if let Some(avg) = body.avg_minutes {
        validate_minutes(avg, body.sigma_minutes)?;
    } else if let Some(sigma) = body.sigma_minutes {
        validate_minutes(0, Some(sigma))?;
    }
    if let Some(ref recurrence) = body.recurrence {
        validate_recurrence(recurrence)?;
    }
    let database = db(&env)?;
    let full = resolve_habit_id(&database, id).await?;
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
        JsValue::from_str(&full),
    ])?
    .run()
    .await
    .map_err(WorkerError::Worker)?;

    let row = select_one(&database, &full).await?;
    json_ok(&row)
}

pub async fn replace(
    mut req: worker::Request,
    env: Env,
    id: &str,
) -> Result<Response, WorkerError> {
    let body: CreateHabit = parse_json(&mut req).await?;
    validate_minutes(body.avg_minutes, body.sigma_minutes)?;
    validate_recurrence(&body.recurrence)?;
    let database = db(&env)?;
    let full = resolve_habit_id(&database, id).await?;
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
        JsValue::from_str(&full),
    ])?
    .run()
    .await
    .map_err(WorkerError::Worker)?;

    let row = select_one(&database, &full).await?;
    json_ok(&row)
}

pub async fn delete(_req: worker::Request, env: Env, id: &str) -> Result<Response, WorkerError> {
    let database = db(&env)?;
    let full = resolve_habit_id(&database, id).await?;
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
    // habit_pauses follows the same rationale (#303).
    let stmts = vec![
        database.prepare("DELETE FROM google_cal_events WHERE task_id IN (SELECT id FROM tasks WHERE habit_id = ?1)").bind(&[JsValue::from_str(&full)])?,
        database
            .prepare("DELETE FROM tasks WHERE habit_id = ?1")
            .bind(&[JsValue::from_str(&full)])?,
        database
            .prepare("DELETE FROM habit_pauses WHERE habit_id = ?1")
            .bind(&[JsValue::from_str(&full)])?,
        database
            .prepare("DELETE FROM habits WHERE id = ?1")
            .bind(&[JsValue::from_str(&full)])?,
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

/// Resolve a habit reference (`h<N>`, full UUID, or UUID prefix) to a full UUID.
async fn resolve_habit_id(database: &worker::D1Database, id: &str) -> Result<String, WorkerError> {
    // `h<N>` → habit display_id lookup (#305).
    if let Some(rest) = id.strip_prefix(['h', 'H'])
        && let Ok(num) = rest.parse::<i64>()
    {
        let stmt = database.prepare(format!(
            "{select} WHERE display_id = ?1",
            select = select_habits()
        ));
        let row: Option<HabitRow> = stmt
            .bind(&[JsValue::from_f64(num as f64)])?
            .first(None)
            .await
            .map_err(WorkerError::Worker)?;
        return row
            .map(|h| h.id)
            .ok_or_else(|| WorkerError::NotFound(format!("habit {id} not found")));
    }
    // Full UUID
    if id.contains('-') {
        return Ok(id.to_string());
    }
    // UUID prefix — fetch all and filter
    let stmt = database.prepare(select_habits());
    let all: Vec<HabitRow> = safe_all(&stmt).await?;
    let matches: Vec<String> = all
        .iter()
        .filter(|h| h.id.starts_with(id))
        .map(|h| h.id.clone())
        .collect();
    match matches.len() {
        0 => Err(WorkerError::NotFound(format!("habit {id} not found"))),
        1 => Ok(matches.into_iter().next().unwrap()),
        _ => Err(WorkerError::BadRequest(format!(
            "ambiguous habit id prefix: {id}"
        ))),
    }
}

#[derive(serde::Deserialize)]
struct DisplayIdRow {
    display_id: i64,
}

// ── Habit pauses (#303) ────────────────────────────────────────────────

const PAUSE_COLS: &str = "id, habit_id, start_date, end_date, reason, created_at";

pub async fn list_pauses(
    _req: worker::Request,
    env: Env,
    id: &str,
) -> Result<Response, WorkerError> {
    let database = db(&env)?;
    let full = resolve_habit_id(&database, id).await?;
    let stmt = database.prepare(format!(
        "SELECT {PAUSE_COLS} FROM habit_pauses WHERE habit_id = ?1 ORDER BY start_date ASC, created_at ASC"
    ));
    let rows: Vec<HabitPauseRow> = safe_all(&stmt.bind(&[JsValue::from_str(&full)])?).await?;
    json_ok(&rows)
}

pub async fn list_all_pauses(_req: worker::Request, env: Env) -> Result<Response, WorkerError> {
    let database = db(&env)?;
    let stmt = database.prepare(format!(
        "SELECT {PAUSE_COLS} FROM habit_pauses ORDER BY habit_id, start_date ASC, created_at ASC"
    ));
    let rows: Vec<HabitPauseRow> = safe_all(&stmt).await?;
    json_ok(&rows)
}

pub async fn create_pause(
    mut req: worker::Request,
    env: Env,
    id: &str,
) -> Result<Response, WorkerError> {
    let body: CreateHabitPause = parse_json(&mut req).await?;
    validate_pause_dates(&body.start_date, &body.end_date)?;
    let database = db(&env)?;
    let full = resolve_habit_id(&database, id).await?;
    let pause_id = uuid::Uuid::now_v7().to_string();
    let stmt = database.prepare(
        "INSERT INTO habit_pauses (id, habit_id, start_date, end_date, reason, created_at) VALUES (?1, ?2, ?3, ?4, ?5, datetime('now'))",
    );
    stmt.bind(&[
        JsValue::from_str(&pause_id),
        JsValue::from_str(&full),
        JsValue::from_str(&body.start_date),
        JsValue::from_str(&body.end_date),
        body.reason
            .as_deref()
            .map(JsValue::from_str)
            .unwrap_or(JsValue::NULL),
    ])?
    .run()
    .await
    .map_err(WorkerError::Worker)?;
    let row = select_one_pause(&database, &pause_id).await?;
    json_created(&row)
}

pub async fn delete_pause(
    _req: worker::Request,
    env: Env,
    id: &str,
    pause_id: &str,
) -> Result<Response, WorkerError> {
    let database = db(&env)?;
    let full = resolve_habit_id(&database, id).await?;
    let stmt = database.prepare("DELETE FROM habit_pauses WHERE id = ?1 AND habit_id = ?2");
    let result = stmt
        .bind(&[JsValue::from_str(pause_id), JsValue::from_str(&full)])?
        .run()
        .await
        .map_err(WorkerError::Worker)?;
    let affected = result
        .meta()
        .map_err(WorkerError::Worker)?
        .and_then(|m| m.rows_written)
        .unwrap_or(0);
    if affected == 0 {
        return Err(WorkerError::NotFound(format!(
            "pause {pause_id} not found for habit {id}"
        )));
    }
    Ok(Response::empty()?)
}

async fn select_one_pause(
    database: &worker::D1Database,
    pause_id: &str,
) -> Result<HabitPauseRow, WorkerError> {
    let stmt = database.prepare(format!(
        "SELECT {PAUSE_COLS} FROM habit_pauses WHERE id = ?1"
    ));
    let row: Option<HabitPauseRow> = stmt
        .bind(&[JsValue::from_str(pause_id)])?
        .first(None)
        .await
        .map_err(WorkerError::Worker)?;
    row.ok_or_else(|| WorkerError::Internal("inserted pause not found".into()))
}
