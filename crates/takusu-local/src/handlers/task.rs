use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use serde::Deserialize;
use takusu_storage::{CreateTask, TaskQuery, TaskRow, UpdateTask};

use crate::error::HttpError;
use crate::state::AppState;

#[derive(Debug, Deserialize)]
pub struct TaskQueryParams {
    pub status: Option<String>,
    pub from: Option<String>,
    pub until: Option<String>,
    pub habit_id: Option<String>,
    pub ical_uid: Option<String>,
}

pub async fn create_task(
    State(state): State<AppState>,
    Json(body): Json<CreateTask>,
) -> Result<(StatusCode, Json<TaskRow>), HttpError> {
    let task = state.app.create_task(&body).await?;
    Ok((StatusCode::CREATED, Json(task)))
}

pub async fn list_tasks(
    State(state): State<AppState>,
    Query(query): Query<TaskQueryParams>,
) -> Result<Json<Vec<TaskRow>>, HttpError> {
    let q = TaskQuery {
        status: query.status,
        from: query.from,
        until: query.until,
        habit_id: query.habit_id,
        ical_uid: query.ical_uid,
    };
    let tasks = state.app.list_tasks(&q).await?;
    Ok(Json(tasks))
}

pub async fn get_task(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<TaskRow>, HttpError> {
    let task = state.app.get_task(&id).await?;
    Ok(Json(task))
}

pub async fn update_task(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<UpdateTask>,
) -> Result<Json<TaskRow>, HttpError> {
    let task = state.app.update_task(&id, &body).await?;
    Ok(Json(task))
}

pub async fn replace_task(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<CreateTask>,
) -> Result<Json<TaskRow>, HttpError> {
    let task = state.app.replace_task(&id, &body).await?;
    Ok(Json(task))
}

pub async fn delete_task(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, HttpError> {
    state.app.delete_task(&id).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn import_ical(
    State(state): State<AppState>,
    body: String,
) -> Result<Json<serde_json::Value>, HttpError> {
    let result = state.app.import_ical(&body).await?;
    Ok(Json(serde_json::json!({
        "imported": result.imported,
        "task_ids": result.task_ids,
    })))
}

pub async fn dependency_analysis(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, HttpError> {
    let redundant = state.app.analyze_task_dependencies().await?;
    Ok(Json(serde_json::json!({ "redundant": redundant })))
}
