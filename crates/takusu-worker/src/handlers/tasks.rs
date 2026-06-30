use wasm_bindgen::JsValue;
use worker::Env;
use worker::Request;
use worker::Response;

use crate::error::WorkerError;
use crate::handlers::auth::db;
use crate::handlers::tokens::{json_created, json_ok, parse_json};
use crate::models::{CreateTask, TaskRow, UpdateTask};

const TASK_COLS: &str = "id, display_id, title, description, start_at, end_at, avg_minutes, sigma_minutes, depends, parallelizable, allows_parallel, abandonability, status, habit_id, ical_uid, created_at, updated_at";

fn select_tasks() -> String {
    format!("SELECT {TASK_COLS} FROM tasks")
}

pub async fn list(req: Request, env: Env) -> Result<Response, WorkerError> {
    let database = db(&env)?;
    let url = req.url()?;
    let mut sql = format!("{select} WHERE 1=1", select = select_tasks());
    let mut bindings: Vec<JsValue> = Vec::new();
    for (k, v) in url.query_pairs() {
        let col = match k.as_ref() {
            "status" => "status",
            "from" => "end_at",
            "until" => "start_at",
            "habit_id" => "habit_id",
            _ => continue,
        };
        sql.push_str(&format!(" AND {col} = ?"));
        bindings.push(JsValue::from_str(&v));
    }
    sql.push_str(" ORDER BY created_at DESC");

    let result = if bindings.is_empty() {
        database
            .prepare(&sql)
            .all()
            .await
            .map_err(WorkerError::Worker)?
    } else {
        database
            .prepare(&sql)
            .bind(&bindings)?
            .all()
            .await
            .map_err(WorkerError::Worker)?
    };
    let rows: Vec<TaskRow> = result.results().map_err(WorkerError::Worker)?;
    json_ok(&rows)
}

pub async fn create(mut req: Request, env: Env) -> Result<Response, WorkerError> {
    let body: CreateTask = parse_json(&mut req).await?;
    let database = db(&env)?;
    let id = uuid::Uuid::now_v7().to_string();
    let resolved_depends = resolve_depends(&database, body.depends.as_deref()).await?;
    let depends_json =
        serde_json::to_string(&resolved_depends).unwrap_or_else(|_| "[]".to_string());
    let sigma = body.sigma_minutes.unwrap_or(0);
    let parallelizable = body.parallelizable.unwrap_or(false);
    let allows_parallel = body.allows_parallel.unwrap_or(false);
    let abandonability = body.abandonability.unwrap_or(0.5);

    // Atomic display_id allocation via subquery — avoids TOCTOU race
    // between separate SELECT MAX and INSERT under concurrent creates.
    let stmt = database.prepare(
        "INSERT INTO tasks (id, display_id, title, description, start_at, end_at, avg_minutes, sigma_minutes, depends, parallelizable, allows_parallel, abandonability, status, ical_uid, habit_id) VALUES (?1, (SELECT COALESCE(MAX(display_id), 0) + 1 FROM tasks), ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, 'pending', ?12, ?13)"
    );
    stmt.bind(&[
        JsValue::from_str(&id),
        JsValue::from_str(&body.title),
        body.description
            .as_deref()
            .map(JsValue::from_str)
            .unwrap_or(JsValue::NULL),
        body.start_at
            .as_deref()
            .map(JsValue::from_str)
            .unwrap_or(JsValue::NULL),
        JsValue::from_str(&body.end_at),
        JsValue::from_f64(body.avg_minutes as f64),
        JsValue::from_f64(sigma as f64),
        JsValue::from_str(&depends_json),
        JsValue::from_bool(parallelizable),
        JsValue::from_bool(allows_parallel),
        JsValue::from_f64(abandonability),
        body.ical_uid
            .as_deref()
            .map(JsValue::from_str)
            .unwrap_or(JsValue::NULL),
        body.habit_id
            .as_deref()
            .map(JsValue::from_str)
            .unwrap_or(JsValue::NULL),
    ])?
    .run()
    .await
    .map_err(WorkerError::Worker)?;

    let row = select_one(&database, &id).await?;
    json_created(&row)
}

pub async fn get(_req: Request, env: Env, id: &str) -> Result<Response, WorkerError> {
    let database = db(&env)?;
    let full = resolve_task_id(&database, id).await?;
    let row = select_one(&database, &full).await?;
    json_ok(&row)
}

pub async fn update(mut req: Request, env: Env, id: &str) -> Result<Response, WorkerError> {
    let body: UpdateTask = parse_json(&mut req).await?;
    let database = db(&env)?;
    let full = resolve_task_id(&database, id).await?;

    let validated = [
        "pending",
        "scheduled",
        "in_progress",
        "completed",
        "skipped",
    ];
    if let Some(ref s) = body.status
        && !validated.contains(&s.as_str())
    {
        return Err(WorkerError::BadRequest(format!("invalid status: {s}")));
    }

    let existing = select_one(&database, &full).await?;
    let status = body.status.clone().unwrap_or(existing.status);

    let depends_json = if let Some(ref deps) = body.depends {
        let resolved = resolve_depends(&database, Some(deps)).await?;
        Some(serde_json::to_string(&resolved).unwrap_or_else(|_| "[]".into()))
    } else {
        None
    };

    let stmt = database.prepare(
        "UPDATE tasks SET title=COALESCE(?1,title), description=COALESCE(?2,description), start_at=COALESCE(?3,start_at), end_at=COALESCE(?4,end_at), avg_minutes=COALESCE(?5,avg_minutes), sigma_minutes=COALESCE(?6,sigma_minutes), depends=COALESCE(?7,depends), parallelizable=COALESCE(?8,parallelizable), allows_parallel=COALESCE(?9,allows_parallel), abandonability=COALESCE(?10,abandonability), status=?11, habit_id=COALESCE(?13,habit_id), updated_at=datetime('now') WHERE id = ?12"
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
        body.start_at
            .as_deref()
            .map(JsValue::from_str)
            .unwrap_or(JsValue::NULL),
        body.end_at
            .as_deref()
            .map(JsValue::from_str)
            .unwrap_or(JsValue::NULL),
        body.avg_minutes
            .map(|n| JsValue::from_f64(n as f64))
            .unwrap_or(JsValue::NULL),
        body.sigma_minutes
            .map(|n| JsValue::from_f64(n as f64))
            .unwrap_or(JsValue::NULL),
        depends_json
            .as_deref()
            .map(JsValue::from_str)
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
        JsValue::from_str(&status),
        JsValue::from_str(&full),
        body.habit_id
            .as_deref()
            .map(JsValue::from_str)
            .unwrap_or(JsValue::NULL),
    ])?
    .run()
    .await
    .map_err(WorkerError::Worker)?;

    let row = select_one(&database, &full).await?;
    json_ok(&row)
}

pub async fn replace(mut req: Request, env: Env, id: &str) -> Result<Response, WorkerError> {
    let body: CreateTask = parse_json(&mut req).await?;
    let database = db(&env)?;
    let full = resolve_task_id(&database, id).await?;
    let resolved_depends = resolve_depends(&database, body.depends.as_deref()).await?;
    let depends_json = serde_json::to_string(&resolved_depends).unwrap_or_else(|_| "[]".into());
    let sigma = body.sigma_minutes.unwrap_or(0);
    let parallelizable = body.parallelizable.unwrap_or(false);
    let allows_parallel = body.allows_parallel.unwrap_or(false);
    let abandonability = body.abandonability.unwrap_or(0.5);

    let stmt = database.prepare(
        "UPDATE tasks SET title=?1, description=?2, start_at=?3, end_at=?4, avg_minutes=?5, sigma_minutes=?6, depends=?7, parallelizable=?8, allows_parallel=?9, abandonability=?10, habit_id=COALESCE(?12,habit_id), updated_at=datetime('now') WHERE id = ?11"
    );
    stmt.bind(&[
        JsValue::from_str(&body.title),
        body.description
            .as_deref()
            .map(JsValue::from_str)
            .unwrap_or(JsValue::NULL),
        body.start_at
            .as_deref()
            .map(JsValue::from_str)
            .unwrap_or(JsValue::NULL),
        JsValue::from_str(&body.end_at),
        JsValue::from_f64(body.avg_minutes as f64),
        JsValue::from_f64(sigma as f64),
        JsValue::from_str(&depends_json),
        JsValue::from_bool(parallelizable),
        JsValue::from_bool(allows_parallel),
        JsValue::from_f64(abandonability),
        JsValue::from_str(&full),
        body.habit_id
            .as_deref()
            .map(JsValue::from_str)
            .unwrap_or(JsValue::NULL),
    ])?
    .run()
    .await
    .map_err(WorkerError::Worker)?;

    let row = select_one(&database, &full).await?;
    json_ok(&row)
}

pub async fn delete(_req: Request, env: Env, id: &str) -> Result<Response, WorkerError> {
    let database = db(&env)?;
    let full = resolve_task_id(&database, id).await?;
    let stmt = database.prepare("DELETE FROM tasks WHERE id = ?1");
    stmt.bind(&[JsValue::from_str(&full)])?
        .run()
        .await
        .map_err(WorkerError::Worker)?;
    Ok(Response::empty()?)
}

pub async fn select_one(database: &worker::D1Database, id: &str) -> Result<TaskRow, WorkerError> {
    let stmt = database.prepare(format!("{select} WHERE id = ?1", select = select_tasks()));
    let row: Option<TaskRow> = stmt
        .bind(&[JsValue::from_str(id)])?
        .first(None)
        .await
        .map_err(WorkerError::Worker)?;
    row.ok_or_else(|| WorkerError::NotFound(format!("task {id} not found")))
}

/// Resolve a single task reference (display_id number, full UUID, or UUID prefix)
/// to a full UUID string.
async fn resolve_task_id(database: &worker::D1Database, id: &str) -> Result<String, WorkerError> {
    // Numeric → display_id lookup only (no UUID prefix fallthrough).
    if let Ok(num) = id.parse::<i64>() {
        let stmt = database.prepare(format!(
            "{select} WHERE display_id = ?1",
            select = select_tasks()
        ));
        let row: Option<TaskRow> = stmt
            .bind(&[JsValue::from_f64(num as f64)])?
            .first(None)
            .await
            .map_err(WorkerError::Worker)?;
        return row
            .map(|t| t.id)
            .ok_or_else(|| WorkerError::NotFound(format!("task {id} not found")));
    }
    // Full UUID
    if id.contains('-') {
        return Ok(id.to_string());
    }
    // UUID prefix — fetch all and filter
    let stmt = database.prepare(select_tasks());
    let result = stmt.all().await.map_err(WorkerError::Worker)?;
    let all: Vec<TaskRow> = result.results().map_err(WorkerError::Worker)?;
    let matches: Vec<String> = all
        .iter()
        .filter(|t| t.id.starts_with(id))
        .map(|t| t.id.clone())
        .collect();
    match matches.len() {
        0 => Err(WorkerError::NotFound(format!("task {id} not found"))),
        1 => Ok(matches.into_iter().next().unwrap()),
        _ => Err(WorkerError::BadRequest(format!(
            "ambiguous task id prefix: {id}"
        ))),
    }
}

/// Resolve a list of dependency references to full UUID strings.
async fn resolve_depends(
    database: &worker::D1Database,
    deps: Option<&[String]>,
) -> Result<Vec<String>, WorkerError> {
    let Some(deps) = deps else {
        return Ok(Vec::new());
    };
    let mut resolved = Vec::with_capacity(deps.len());
    for d in deps {
        resolved.push(resolve_task_id(database, d).await?);
    }
    Ok(resolved)
}
