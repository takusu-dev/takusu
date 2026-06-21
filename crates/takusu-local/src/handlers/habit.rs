use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use takusu_storage::{CreateHabit, HabitRow, UpdateHabit};

use crate::error::AppError;
use crate::handlers::task::storage_to_app;
use crate::state::AppState;

pub async fn create_habit(
    State(state): State<AppState>,
    Json(body): Json<CreateHabit>,
) -> Result<(StatusCode, Json<HabitRow>), AppError> {
    let habit = state
        .storage
        .create_habit(&body)
        .await
        .map_err(storage_to_app)?;
    Ok((StatusCode::CREATED, Json(habit)))
}

pub async fn list_habits(State(state): State<AppState>) -> Result<Json<Vec<HabitRow>>, AppError> {
    let habits = state.storage.list_habits().await.map_err(storage_to_app)?;
    Ok(Json(habits))
}

pub async fn get_habit(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<HabitRow>, AppError> {
    let habit = state.storage.get_habit(&id).await.map_err(storage_to_app)?;
    Ok(Json(habit))
}

pub async fn update_habit(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<UpdateHabit>,
) -> Result<Json<HabitRow>, AppError> {
    let habit = state
        .storage
        .update_habit(&id, &body)
        .await
        .map_err(storage_to_app)?;
    Ok(Json(habit))
}

pub async fn replace_habit(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<CreateHabit>,
) -> Result<Json<HabitRow>, AppError> {
    let habit = state
        .storage
        .replace_habit(&id, &body)
        .await
        .map_err(storage_to_app)?;
    Ok(Json(habit))
}

pub async fn delete_habit(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, AppError> {
    state
        .storage
        .delete_habit(&id)
        .await
        .map_err(storage_to_app)?;
    Ok(StatusCode::NO_CONTENT)
}
