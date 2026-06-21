use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use serde::Deserialize;
use takusu_storage::{CreateTask, TaskQuery, TaskRow, UpdateTask};

use crate::error::AppError;
use crate::state::AppState;

#[derive(Debug, Deserialize)]
pub struct TaskQueryParams {
    pub status: Option<String>,
    pub from: Option<String>,
    pub until: Option<String>,
    pub habit_id: Option<String>,
}

pub async fn create_task(
    State(state): State<AppState>,
    Json(body): Json<CreateTask>,
) -> Result<(StatusCode, Json<TaskRow>), AppError> {
    let task = state
        .storage
        .create_task(&body)
        .await
        .map_err(|e| storage_to_app(e))?;
    Ok((StatusCode::CREATED, Json(task)))
}

pub async fn list_tasks(
    State(state): State<AppState>,
    Query(query): Query<TaskQueryParams>,
) -> Result<Json<Vec<TaskRow>>, AppError> {
    let q = TaskQuery {
        status: query.status,
        from: query.from,
        until: query.until,
        habit_id: query.habit_id,
    };
    let tasks = state.storage.list_tasks(&q).await.map_err(storage_to_app)?;
    Ok(Json(tasks))
}

pub async fn get_task(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<TaskRow>, AppError> {
    let task = state.storage.get_task(&id).await.map_err(storage_to_app)?;
    Ok(Json(task))
}

pub async fn update_task(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<UpdateTask>,
) -> Result<Json<TaskRow>, AppError> {
    let task = state
        .storage
        .update_task(&id, &body)
        .await
        .map_err(storage_to_app)?;
    Ok(Json(task))
}

pub async fn replace_task(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<CreateTask>,
) -> Result<Json<TaskRow>, AppError> {
    let task = state
        .storage
        .replace_task(&id, &body)
        .await
        .map_err(storage_to_app)?;
    Ok(Json(task))
}

pub async fn delete_task(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, AppError> {
    state
        .storage
        .delete_task(&id)
        .await
        .map_err(storage_to_app)?;
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
            let exists = task_exists_by_ical_uid(&state, uid).await?;
            if exists {
                continue;
            }
        }
        let task = state
            .storage
            .create_task(&CreateTask {
                title: event.title.clone(),
                description: event.description.clone(),
                start_at: Some(event.start_at.to_string()),
                end_at: event.end_at.to_string(),
                avg_minutes: 0,
                sigma_minutes: Some(0),
                depends: Some(vec![]),
                parallelizable: Some(false),
                allows_parallel: Some(false),
                abandonability: Some(0.5),
                ical_uid: event.uid.clone(),
            })
            .await
            .map_err(storage_to_app)?;
        imported += 1;
        task_ids.push(task.id);
    }
    Ok(Json(serde_json::json!({
        "imported": imported,
        "task_ids": task_ids,
    })))
}

async fn task_exists_by_ical_uid(state: &AppState, uid: &str) -> Result<bool, AppError> {
    let tasks = state
        .storage
        .list_tasks(&TaskQuery::default())
        .await
        .map_err(storage_to_app)?;
    Ok(tasks.iter().any(|t| t.ical_uid.as_deref() == Some(uid)))
}

pub(crate) fn storage_to_app(e: takusu_storage::StorageError) -> AppError {
    use takusu_storage::StorageError;
    match e {
        StorageError::NotFound(m) => AppError::NotFound(m),
        StorageError::BadRequest(m) => AppError::BadRequest(m),
        StorageError::Unauthorized => AppError::Unauthorized,
        StorageError::Conflict(m) => AppError::Conflict { message: m },
        StorageError::Internal(m) => AppError::Internal(m),
    }
}
