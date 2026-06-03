use crate::app::AppState;
use crate::error::AppError;
use crate::model::*;
use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use serde::Deserialize;
use uuid::Uuid;
#[derive(Debug, Deserialize)]
pub struct TaskQuery {
    pub status: Option<String>,
    pub from: Option<String>,
    pub until: Option<String>,
    pub habit_id: Option<String>,
}
pub async fn create_task(
    State(state): State<AppState>,
    Json(body): Json<CreateTask>,
) -> Result<(StatusCode, Json<TaskRow>), AppError> {
    let id = Uuid::now_v7().to_string();
    let depends_json = serde_json::to_string(&body.depends).unwrap_or_else(|_| "[]".to_string());
    sqlx::query(
        "INSERT INTO tasks (id, title, description, start_at, end_at, avg_minutes, sigma_minutes, depends, parallelizable, allows_parallel, abandonability, status) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 'pending')"
    )
    .bind(&id)
    .bind(&body.title)
    .bind(&body.description)
    .bind(&body.start_at)
    .bind(&body.end_at)
    .bind(body.avg_minutes)
    .bind(body.sigma_minutes)
    .bind(&depends_json)
    .bind(body.parallelizable)
    .bind(body.allows_parallel)
    .bind(body.abandonability)
    .execute(&state.db).await?;
    let row = sqlx::query_as::<_, TaskRow>("SELECT * FROM tasks WHERE id = ?")
        .bind(&id)
        .fetch_one(&state.db)
        .await?;
    Ok((StatusCode::CREATED, Json(row)))
}
pub async fn list_tasks(
    State(state): State<AppState>,
    Query(query): Query<TaskQuery>,
) -> Result<Json<Vec<TaskRow>>, AppError> {
    let mut sql = String::from("SELECT * FROM tasks WHERE 1=1");
    let mut bindings: Vec<String> = Vec::new();
    if let Some(ref v) = query.status {
        sql.push_str(" AND status = ?");
        bindings.push(v.clone());
    }
    if let Some(ref v) = query.from {
        sql.push_str(" AND end_at >= ?");
        bindings.push(v.clone());
    }
    if let Some(ref v) = query.until {
        sql.push_str(" AND start_at <= ?");
        bindings.push(v.clone());
    }
    if let Some(ref v) = query.habit_id {
        sql.push_str(" AND habit_id = ?");
        bindings.push(v.clone());
    }
    sql.push_str(" ORDER BY created_at DESC");
    let mut q = sqlx::query_as::<_, TaskRow>(sqlx::AssertSqlSafe(sql.as_str()));
    for b in &bindings {
        q = q.bind(b);
    }
    let rows = q.fetch_all(&state.db).await?;
    Ok(Json(rows))
}
pub async fn get_task(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<TaskRow>, AppError> {
    let row = sqlx::query_as::<_, TaskRow>("SELECT * FROM tasks WHERE id = ?")
        .bind(&id)
        .fetch_optional(&state.db)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("task {id} not found")))?;
    Ok(Json(row))
}
pub async fn update_task(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<UpdateTask>,
) -> Result<Json<TaskRow>, AppError> {
    let existing = sqlx::query_as::<_, TaskRow>("SELECT * FROM tasks WHERE id = ?")
        .bind(&id)
        .fetch_optional(&state.db)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("task {id} not found")))?;
    let depends_json = body
        .depends
        .as_ref()
        .map(|d| serde_json::to_string(d).unwrap_or_else(|_| "[]".into()));
    let status = body.status.as_ref().unwrap_or(&existing.status);
    let validated = [
        "pending",
        "scheduled",
        "in_progress",
        "completed",
        "skipped",
    ];
    if !validated.contains(&status.as_str()) {
        return Err(AppError::BadRequest(format!("invalid status: {status}")));
    }
    sqlx::query(
        "UPDATE tasks SET title=COALESCE(?,title), description=COALESCE(?,description), start_at=COALESCE(?,start_at), end_at=COALESCE(?,end_at), avg_minutes=COALESCE(?,avg_minutes), sigma_minutes=COALESCE(?,sigma_minutes), depends=COALESCE(?,depends), parallelizable=COALESCE(?,parallelizable), allows_parallel=COALESCE(?,allows_parallel), abandonability=COALESCE(?,abandonability), status=?, updated_at=datetime('now') WHERE id = ?"
    )
    .bind(body.title.as_ref())
    .bind(body.description.as_ref())
    .bind(body.start_at.as_ref())
    .bind(body.end_at.as_ref())
    .bind(body.avg_minutes)
    .bind(body.sigma_minutes)
    .bind(depends_json.as_ref())
    .bind(body.parallelizable)
    .bind(body.allows_parallel)
    .bind(body.abandonability)
    .bind(status)
    .bind(&id)
    .execute(&state.db).await?;
    let row = sqlx::query_as::<_, TaskRow>("SELECT * FROM tasks WHERE id = ?")
        .bind(&id)
        .fetch_one(&state.db)
        .await?;
    Ok(Json(row))
}
pub async fn replace_task(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<CreateTask>,
) -> Result<Json<TaskRow>, AppError> {
    let depends_json = serde_json::to_string(&body.depends).unwrap_or_else(|_| "[]".into());
    let result = sqlx::query(
        "UPDATE tasks SET title=?, description=?, start_at=?, end_at=?, avg_minutes=?, sigma_minutes=?, depends=?, parallelizable=?, allows_parallel=?, abandonability=?, updated_at=datetime('now') WHERE id = ?"
    )
    .bind(&body.title)
    .bind(&body.description)
    .bind(&body.start_at)
    .bind(&body.end_at)
    .bind(body.avg_minutes)
    .bind(body.sigma_minutes)
    .bind(&depends_json)
    .bind(body.parallelizable)
    .bind(body.allows_parallel)
    .bind(body.abandonability)
    .bind(&id)
    .execute(&state.db).await?;
    if result.rows_affected() == 0 {
        return Err(AppError::NotFound(format!("task {id} not found")));
    }
    let row = sqlx::query_as::<_, TaskRow>("SELECT * FROM tasks WHERE id = ?")
        .bind(&id)
        .fetch_one(&state.db)
        .await?;
    Ok(Json(row))
}
pub async fn delete_task(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, AppError> {
    let result = sqlx::query("DELETE FROM tasks WHERE id = ?")
        .bind(&id)
        .execute(&state.db)
        .await?;
    if result.rows_affected() == 0 {
        return Err(AppError::NotFound(format!("task {id} not found")));
    }
    Ok(StatusCode::NO_CONTENT)
}
pub async fn import_ical(
    State(state): State<AppState>,
    body: String,
) -> Result<Json<serde_json::Value>, AppError> {
    let events = takusu_ical::parse_ical(&body).map_err(|e| AppError::BadRequest(e.to_string()))?;
    let mut imported = 0usize;
    let mut task_ids = Vec::new();
    for event in &events {
        if let Some(ref uid) = event.uid {
            let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM tasks WHERE ical_uid = ?")
                .bind(uid)
                .fetch_one(&state.db)
                .await?;
            if count > 0 {
                continue;
            }
        }
        let id = Uuid::now_v7().to_string();
        sqlx::query(
            "INSERT INTO tasks (id, title, description, start_at, end_at, avg_minutes, sigma_minutes, depends, parallelizable, allows_parallel, abandonability, status, ical_uid) VALUES (?, ?, ?, ?, ?, ?, 0, '[]', 0, 0, 0.5, 'pending', ?)"
        )
        .bind(&id)
        .bind(&event.title)
        .bind(&event.description)
        .bind(&event.start_at)
        .bind(&event.end_at)
        .bind(0i64)
        .bind(&event.uid)
        .execute(&state.db).await?;
        task_ids.push(id);
        imported += 1;
    }
    Ok(Json(serde_json::json!({
        "imported": imported,
        "task_ids": task_ids
    })))
}
