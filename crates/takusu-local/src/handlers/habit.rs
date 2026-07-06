use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use takusu_storage::{CreateHabit, CreateHabitPause, HabitPauseRow, HabitRow, UpdateHabit};

use crate::error::HttpError;
use crate::state::AppState;

pub async fn create_habit(
    State(state): State<AppState>,
    Json(body): Json<CreateHabit>,
) -> Result<(StatusCode, Json<HabitRow>), HttpError> {
    let habit = state.app.create_habit(&body).await?;
    Ok((StatusCode::CREATED, Json(habit)))
}

pub async fn list_habits(State(state): State<AppState>) -> Result<Json<Vec<HabitRow>>, HttpError> {
    let habits = state.app.list_habits().await?;
    Ok(Json(habits))
}

pub async fn get_habit(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<HabitRow>, HttpError> {
    let habit = state.app.get_habit(&id).await?;
    Ok(Json(habit))
}

pub async fn update_habit(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<UpdateHabit>,
) -> Result<Json<HabitRow>, HttpError> {
    let habit = state.app.update_habit(&id, &body).await?;
    Ok(Json(habit))
}

pub async fn replace_habit(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<CreateHabit>,
) -> Result<Json<HabitRow>, HttpError> {
    let habit = state.app.replace_habit(&id, &body).await?;
    Ok(Json(habit))
}

pub async fn delete_habit(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, HttpError> {
    state.app.delete_habit(&id).await?;
    Ok(StatusCode::NO_CONTENT)
}

// ── Habit pauses (#303) ────────────────────────────────────────────────

pub async fn list_habit_pauses(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Vec<HabitPauseRow>>, HttpError> {
    let pauses = state.app.list_habit_pauses(&id).await?;
    Ok(Json(pauses))
}

pub async fn list_all_habit_pauses(
    State(state): State<AppState>,
) -> Result<Json<Vec<HabitPauseRow>>, HttpError> {
    let pauses = state
        .app
        .list_all_habit_pauses()
        .await
        .map_err(HttpError::from)?;
    Ok(Json(pauses))
}

pub async fn create_habit_pause(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<CreateHabitPause>,
) -> Result<(StatusCode, Json<HabitPauseRow>), HttpError> {
    let pause = state.app.create_habit_pause(&id, &body).await?;
    Ok((StatusCode::CREATED, Json(pause)))
}

pub async fn delete_habit_pause(
    State(state): State<AppState>,
    Path((id, pause_id)): Path<(String, String)>,
) -> Result<StatusCode, HttpError> {
    state.app.delete_habit_pause(&id, &pause_id).await?;
    Ok(StatusCode::NO_CONTENT)
}
