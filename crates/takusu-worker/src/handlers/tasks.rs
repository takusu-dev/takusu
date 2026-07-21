use wasm_bindgen::JsValue;
use worker::{Env, Request, Response};

use crate::error::WorkerError;
use crate::handlers::auth::db;
use crate::handlers::d1::safe_all;
use crate::handlers::tokens::{json_created, json_ok, parse_json};
use crate::models::{CreateTask, TaskRow, UpdateTask};
use crate::validate::{validate_minutes, validate_quantity};

const TASK_COLS: &str = "id, display_id, title, description, start_at, end_at, avg_minutes, sigma_minutes, depends, parallelizable, allows_parallel, abandonability, status, habit_id, ical_uid, user_edited, fixed, habit_step_id, quantity_total, quantity_done, quantity_unit, completed_at, split_from_task_id, original_quantity_total, created_at, updated_at";
/// SQL predicate for tasks whose deadline has passed but are not finished.
const OVERDUE_SQL: &str =
    "status NOT IN ('completed', 'skipped') AND datetime(end_at) < datetime('now')";
/// SQL predicate that excludes overdue tasks (completed/skipped or end_at is now or later).
const NOT_OVERDUE_SQL: &str =
    "(status IN ('completed', 'skipped') OR datetime(end_at) >= datetime('now'))";

pub(crate) fn select_tasks() -> String {
    format!("SELECT {TASK_COLS} FROM tasks")
}

pub async fn list(req: Request, env: Env) -> Result<Response, WorkerError> {
    let database = db(&env)?;
    let url = req.url()?;
    let mut sql = format!("{select} WHERE 1=1", select = select_tasks());
    let mut bindings: Vec<JsValue> = Vec::new();
    for (k, v) in url.query_pairs() {
        match k.as_ref() {
            "status" => {
                if v == "overdue" {
                    sql.push_str(" AND ");
                    sql.push_str(OVERDUE_SQL);
                } else {
                    sql.push_str(" AND status = ?");
                    bindings.push(JsValue::from_str(&v));
                }
            }
            "from" => {
                // end_at is NOT NULL, so a simple >= is safe.
                sql.push_str(" AND end_at >= ?");
                bindings.push(JsValue::from_str(&v));
            }
            "until" => {
                // start_at is nullable: NULL <= value evaluates to NULL
                // (excluded). Include tasks with no explicit start time so
                // range queries don't silently drop them.
                sql.push_str(" AND (start_at IS NULL OR start_at <= ?)");
                bindings.push(JsValue::from_str(&v));
            }
            "no_overdue" => {
                if v.parse::<bool>().unwrap_or(false) {
                    sql.push_str(" AND ");
                    sql.push_str(NOT_OVERDUE_SQL);
                }
            }
            "habit_id" => {
                sql.push_str(" AND habit_id = ?");
                bindings.push(JsValue::from_str(&v));
            }
            "ical_uid" => {
                sql.push_str(" AND ical_uid = ?");
                bindings.push(JsValue::from_str(&v));
            }
            _ => continue,
        }
    }
    sql.push_str(" ORDER BY created_at DESC");

    let stmt = if bindings.is_empty() {
        database.prepare(&sql)
    } else {
        database.prepare(&sql).bind(&bindings)?
    };
    let rows: Vec<TaskRow> = safe_all(&stmt).await?;
    json_ok(&rows)
}

pub async fn create(mut req: Request, env: Env) -> Result<Response, WorkerError> {
    let body: CreateTask = parse_json(&mut req).await?;
    validate_minutes(body.avg_minutes, body.sigma_minutes)?;
    validate_quantity(
        body.quantity_total,
        body.quantity_done,
        body.original_quantity_total,
    )?;
    let database = db(&env)?;
    let id = uuid::Uuid::now_v7().to_string();
    let resolved_depends = resolve_depends(&database, body.depends.as_deref()).await?;
    let depends_json =
        serde_json::to_string(&resolved_depends).unwrap_or_else(|_| "[]".to_string());
    let sigma = body.sigma_minutes.unwrap_or((body.avg_minutes / 5).max(1));
    let parallelizable = body.parallelizable.unwrap_or(false);
    let allows_parallel = body.allows_parallel.unwrap_or(false);
    let abandonability = body.abandonability.unwrap_or(0.5);

    // Atomically reserve a monotonic display_id from the sequence table.
    // This prevents display_id reuse after task deletion (#186).
    // For habit tasks, use a habit-specific sequence (#380).
    let display_id = if let Some(ref habit_id) = body.habit_id {
        // Use habit-specific sequence. Ensure the sequence entry exists first.
        let insert_stmt = database.prepare(
            "INSERT OR IGNORE INTO habit_task_display_id_seq (habit_id, next_id) VALUES (?1, 1)",
        );
        insert_stmt
            .bind(&[JsValue::from_str(habit_id)])?
            .run()
            .await
            .map_err(WorkerError::Worker)?;
        let seq_stmt = database.prepare(
            "UPDATE habit_task_display_id_seq SET next_id = next_id + 1 WHERE habit_id = ?1 RETURNING next_id - 1 AS display_id",
        );
        let bindings = vec![JsValue::from_str(habit_id)];
        let seq_row: Option<DisplayIdRow> = seq_stmt
            .bind(&bindings)?
            .first(None)
            .await
            .map_err(WorkerError::Worker)?;
        seq_row
            .ok_or_else(|| WorkerError::Internal("habit display_id sequence is empty".into()))?
            .display_id
    } else {
        // Use global task sequence
        let seq_stmt = database.prepare(
            "UPDATE task_display_id_seq SET next_id = next_id + 1 RETURNING next_id - 1 AS display_id",
        );
        let seq_row: Option<DisplayIdRow> =
            seq_stmt.first(None).await.map_err(WorkerError::Worker)?;
        seq_row
            .ok_or_else(|| WorkerError::Internal("display_id sequence is empty".into()))?
            .display_id
    };

    let quantity_done = body.quantity_done.unwrap_or(0);
    let stmt = database.prepare(
        "INSERT INTO tasks (id, display_id, title, description, start_at, end_at, avg_minutes, sigma_minutes, depends, parallelizable, allows_parallel, abandonability, status, ical_uid, habit_id, fixed, habit_step_id, quantity_total, quantity_done, quantity_unit, completed_at, split_from_task_id, original_quantity_total) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, 'pending', ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22)"
    );
    stmt.bind(&[
        JsValue::from_str(&id),
        JsValue::from_f64(display_id as f64),
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
        JsValue::from_bool(body.fixed.unwrap_or(false)),
        body.habit_step_id
            .as_deref()
            .map(JsValue::from_str)
            .unwrap_or(JsValue::NULL),
        body.quantity_total
            .map(|n| JsValue::from_f64(n as f64))
            .unwrap_or(JsValue::NULL),
        JsValue::from_f64(quantity_done as f64),
        body.quantity_unit
            .as_deref()
            .map(JsValue::from_str)
            .unwrap_or(JsValue::NULL),
        JsValue::NULL,
        JsValue::NULL,
        body.original_quantity_total
            .map(|n| JsValue::from_f64(n as f64))
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
    if let Some(avg) = body.avg_minutes {
        validate_minutes(avg, body.sigma_minutes)?;
    } else if let Some(sigma) = body.sigma_minutes {
        validate_minutes(0, Some(sigma))?;
    }
    let database = db(&env)?;
    let full = resolve_task_id(&database, id).await?;
    let existing = select_one(&database, &full).await?;
    validate_quantity(
        body.quantity_total.or(existing.quantity_total),
        body.quantity_done.or(Some(existing.quantity_done)),
        body.original_quantity_total,
    )?;

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

    let status = body.status.clone().unwrap_or(existing.status);

    let depends_json = if let Some(ref deps) = body.depends {
        let resolved = resolve_depends(&database, Some(deps)).await?;
        Some(serde_json::to_string(&resolved).unwrap_or_else(|_| "[]".into()))
    } else {
        None
    };

    let stmt = database.prepare(
        "UPDATE tasks SET title=COALESCE(?1,title), description=COALESCE(?2,description), start_at=COALESCE(?3,start_at), end_at=COALESCE(?4,end_at), avg_minutes=COALESCE(?5,avg_minutes), sigma_minutes=COALESCE(?6,sigma_minutes), depends=COALESCE(?7,depends), parallelizable=COALESCE(?8,parallelizable), allows_parallel=COALESCE(?9,allows_parallel), abandonability=COALESCE(?10,abandonability), status=?11, habit_id=COALESCE(?13,habit_id), user_edited=COALESCE(?14,user_edited), fixed=COALESCE(?15,fixed), habit_step_id=COALESCE(?16,habit_step_id), quantity_total=COALESCE(?17,quantity_total), quantity_done=COALESCE(?18,quantity_done), quantity_unit=COALESCE(?19,quantity_unit), original_quantity_total=COALESCE(?20,original_quantity_total), updated_at=datetime('now') WHERE id = ?12"
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
        body.user_edited
            .map(JsValue::from_bool)
            .unwrap_or(JsValue::NULL),
        body.fixed.map(JsValue::from_bool).unwrap_or(JsValue::NULL),
        body.habit_step_id
            .as_deref()
            .map(JsValue::from_str)
            .unwrap_or(JsValue::NULL),
        body.quantity_total
            .map(|n| JsValue::from_f64(n as f64))
            .unwrap_or(JsValue::NULL),
        body.quantity_done
            .map(|n| JsValue::from_f64(n as f64))
            .unwrap_or(JsValue::NULL),
        body.quantity_unit
            .as_deref()
            .map(JsValue::from_str)
            .unwrap_or(JsValue::NULL),
        body.original_quantity_total
            .map(|n| JsValue::from_f64(n as f64))
            .unwrap_or(JsValue::NULL),
    ])?
    .run()
    .await
    .map_err(WorkerError::Worker)?;

    // completed_at must follow explicit status transitions: set on
    // completion, clear when leaving completed.
    if body.status.is_some() {
        let completed_stmt = database.prepare(
            "UPDATE tasks SET completed_at = CASE WHEN ?1 = 'completed' AND completed_at IS NULL THEN strftime('%Y-%m-%dT%H:%M:%SZ','now') WHEN ?1 != 'completed' AND completed_at IS NOT NULL THEN NULL ELSE completed_at END WHERE id = ?2",
        );
        completed_stmt
            .bind(&[JsValue::from_str(&status), JsValue::from_str(&full)])?
            .run()
            .await
            .map_err(WorkerError::Worker)?;
    }

    let row = select_one(&database, &full).await?;
    json_ok(&row)
}

pub async fn replace(mut req: Request, env: Env, id: &str) -> Result<Response, WorkerError> {
    let body: CreateTask = parse_json(&mut req).await?;
    validate_minutes(body.avg_minutes, body.sigma_minutes)?;
    validate_quantity(
        body.quantity_total,
        body.quantity_done,
        body.original_quantity_total,
    )?;
    let database = db(&env)?;
    let full = resolve_task_id(&database, id).await?;
    let resolved_depends = resolve_depends(&database, body.depends.as_deref()).await?;
    let depends_json = serde_json::to_string(&resolved_depends).unwrap_or_else(|_| "[]".into());
    let sigma = body.sigma_minutes.unwrap_or((body.avg_minutes / 5).max(1));
    let parallelizable = body.parallelizable.unwrap_or(false);
    let allows_parallel = body.allows_parallel.unwrap_or(false);
    let abandonability = body.abandonability.unwrap_or(0.5);

    let stmt = database.prepare(
        "UPDATE tasks SET title=?1, description=?2, start_at=?3, end_at=?4, avg_minutes=?5, sigma_minutes=?6, depends=?7, parallelizable=?8, allows_parallel=?9, abandonability=?10, habit_id=COALESCE(?12,habit_id), fixed=?13, habit_step_id=?14, quantity_total=COALESCE(?15, quantity_total), quantity_done=COALESCE(?16, quantity_done), quantity_unit=COALESCE(?17, quantity_unit), completed_at=COALESCE(?18, completed_at), split_from_task_id=COALESCE(?19, split_from_task_id), original_quantity_total=COALESCE(?20, original_quantity_total), updated_at=datetime('now') WHERE id = ?11"
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
        JsValue::from_bool(body.fixed.unwrap_or(false)),
        body.habit_step_id
            .as_deref()
            .map(JsValue::from_str)
            .unwrap_or(JsValue::NULL),
        body.quantity_total
            .map(|n| JsValue::from_f64(n as f64))
            .unwrap_or(JsValue::NULL),
        body.quantity_done
            .map(|n| JsValue::from_f64(n as f64))
            .unwrap_or(JsValue::NULL),
        body.quantity_unit
            .as_deref()
            .map(JsValue::from_str)
            .unwrap_or(JsValue::NULL),
        JsValue::NULL,
        JsValue::NULL,
        body.original_quantity_total
            .map(|n| JsValue::from_f64(n as f64))
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
pub(crate) async fn resolve_task_id(
    database: &worker::D1Database,
    id: &str,
) -> Result<String, WorkerError> {
    // Allow display ids with a leading `#` (e.g. `#42`) written by the LLM.
    let id = id.strip_prefix('#').unwrap_or(id);

    // `h{habit_display_id}#{task_display_id}` → habit task lookup (#380).
    if let Some(rest) = id.strip_prefix(['h', 'H'])
        && let Some((hdisp, tdisp)) = rest.split_once('#')
        && let (Ok(hnum), Ok(tnum)) = (hdisp.parse::<i64>(), tdisp.parse::<i64>())
    {
        let stmt = database.prepare(
            "SELECT t.id AS id FROM tasks t JOIN habits h ON t.habit_id = h.id \
             WHERE h.display_id = ?1 AND t.display_id = ?2",
        );
        let row: Option<IdRow> = stmt
            .bind(&[
                JsValue::from_f64(hnum as f64),
                JsValue::from_f64(tnum as f64),
            ])?
            .first(None)
            .await
            .map_err(WorkerError::Worker)?;
        return row
            .map(|r| r.id)
            .ok_or_else(|| WorkerError::NotFound(format!("task {id} not found")));
    }
    // Numeric → display_id lookup for non-habit tasks only (#380).
    if let Ok(num) = id.parse::<i64>() {
        let stmt = database.prepare(format!(
            "{select} WHERE display_id = ?1 AND habit_id IS NULL",
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
    // Full UUID — verify it exists before accepting it.
    if id.contains('-') {
        let stmt = database.prepare("SELECT id FROM tasks WHERE id = ?1");
        let row: Option<IdRow> = stmt
            .bind(&[JsValue::from_str(id)])?
            .first(None)
            .await
            .map_err(WorkerError::Worker)?;
        return row
            .map(|r| r.id)
            .ok_or_else(|| WorkerError::NotFound(format!("task {id} not found")));
    }
    // UUID prefix — fetch all and filter
    let stmt = database.prepare(select_tasks());
    let all: Vec<TaskRow> = safe_all(&stmt).await?;
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
pub(crate) async fn resolve_depends(
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

/// Allocate the next monotonic display_id from the sequence table.
pub(crate) async fn allocate_display_id(
    database: &worker::D1Database,
    habit_id: Option<&str>,
) -> Result<i64, WorkerError> {
    if let Some(habit_id) = habit_id {
        let insert_stmt = database.prepare(
            "INSERT OR IGNORE INTO habit_task_display_id_seq (habit_id, next_id) VALUES (?1, 1)",
        );
        insert_stmt
            .bind(&[JsValue::from_str(habit_id)])?
            .run()
            .await
            .map_err(WorkerError::Worker)?;
        let seq_stmt = database.prepare(
            "UPDATE habit_task_display_id_seq SET next_id = next_id + 1 WHERE habit_id = ?1 RETURNING next_id - 1 AS display_id",
        );
        let row: Option<DisplayIdRow> = seq_stmt
            .bind(&[JsValue::from_str(habit_id)])?
            .first(None)
            .await
            .map_err(WorkerError::Worker)?;
        row.ok_or_else(|| WorkerError::Internal("habit display_id sequence is empty".into()))
            .map(|r| r.display_id)
    } else {
        let seq_stmt = database.prepare(
            "UPDATE task_display_id_seq SET next_id = next_id + 1 RETURNING next_id - 1 AS display_id",
        );
        let row: Option<DisplayIdRow> = seq_stmt.first(None).await.map_err(WorkerError::Worker)?;
        row.ok_or_else(|| WorkerError::Internal("display_id sequence is empty".into()))
            .map(|r| r.display_id)
    }
}

#[derive(serde::Deserialize)]
pub(crate) struct DisplayIdRow {
    pub(crate) display_id: i64,
}

#[derive(serde::Deserialize)]
pub(crate) struct IdRow {
    pub(crate) id: String,
}
