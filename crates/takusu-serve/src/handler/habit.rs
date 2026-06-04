use crate::app::AppState;
use crate::error::AppError;
use crate::model::*;
use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use sqlx::SqlitePool;
use uuid::Uuid;

async fn resolve_habit_id(db: &SqlitePool, id: &str) -> Result<String, AppError> {
    if id.contains('-') {
        let exists: bool = sqlx::query_scalar("SELECT COUNT(*) > 0 FROM habits WHERE id = ?")
            .bind(id)
            .fetch_one(db)
            .await?;
        if exists {
            return Ok(id.to_string());
        }
    } else {
        let matches: Vec<String> =
            sqlx::query_scalar("SELECT id FROM habits WHERE id LIKE ? || '%'")
                .bind(id)
                .fetch_all(db)
                .await?;
        match matches.len() {
            0 => {}
            1 => return Ok(matches.into_iter().next().unwrap()),
            _ => {
                return Err(AppError::BadRequest(format!(
                    "ambiguous habit id prefix: {id}"
                )));
            }
        }
    }
    Err(AppError::NotFound(format!("habit {id} not found")))
}
pub async fn create_habit(
    State(state): State<AppState>,
    Json(body): Json<CreateHabit>,
) -> Result<(StatusCode, Json<HabitRow>), AppError> {
    let id = Uuid::now_v7().to_string();
    sqlx::query(
        "INSERT INTO habits (id, title, description, recurrence, start_time, end_time, avg_minutes, sigma_minutes, parallelizable, allows_parallel, abandonability, active) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 1)"
    )
    .bind(&id)
    .bind(&body.title)
    .bind(&body.description)
    .bind(&body.recurrence)
    .bind(&body.start_time)
    .bind(&body.end_time)
    .bind(body.avg_minutes)
    .bind(body.sigma_minutes)
    .bind(body.parallelizable)
    .bind(body.allows_parallel)
    .bind(body.abandonability)
    .execute(&state.db).await?;
    let row = sqlx::query_as::<_, HabitRow>("SELECT * FROM habits WHERE id = ?")
        .bind(&id)
        .fetch_one(&state.db)
        .await?;
    Ok((StatusCode::CREATED, Json(row)))
}
pub async fn list_habits(State(state): State<AppState>) -> Result<Json<Vec<HabitRow>>, AppError> {
    let rows = sqlx::query_as::<_, HabitRow>("SELECT * FROM habits ORDER BY created_at DESC")
        .fetch_all(&state.db)
        .await?;
    Ok(Json(rows))
}
pub async fn get_habit(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<HabitRow>, AppError> {
    let full_id = resolve_habit_id(&state.db, &id).await?;
    let row = sqlx::query_as::<_, HabitRow>("SELECT * FROM habits WHERE id = ?")
        .bind(&full_id)
        .fetch_one(&state.db)
        .await?;
    Ok(Json(row))
}
pub async fn update_habit(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<UpdateHabit>,
) -> Result<Json<HabitRow>, AppError> {
    let full_id = resolve_habit_id(&state.db, &id).await?;
    sqlx::query(
        "UPDATE habits SET title=COALESCE(?,title), description=COALESCE(?,description), recurrence=COALESCE(?,recurrence), start_time=COALESCE(?,start_time), end_time=COALESCE(?,end_time), avg_minutes=COALESCE(?,avg_minutes), sigma_minutes=COALESCE(?,sigma_minutes), parallelizable=COALESCE(?,parallelizable), allows_parallel=COALESCE(?,allows_parallel), abandonability=COALESCE(?,abandonability), active=COALESCE(?,active), updated_at=datetime('now') WHERE id = ?"
    )
    .bind(body.title.as_ref())
    .bind(body.description.as_ref())
    .bind(body.recurrence.as_ref())
    .bind(body.start_time.as_ref())
    .bind(body.end_time.as_ref())
    .bind(body.avg_minutes)
    .bind(body.sigma_minutes)
    .bind(body.parallelizable)
    .bind(body.allows_parallel)
    .bind(body.abandonability)
    .bind(body.active)
    .bind(&full_id)
    .execute(&state.db).await?;
    let row = sqlx::query_as::<_, HabitRow>("SELECT * FROM habits WHERE id = ?")
        .bind(&full_id)
        .fetch_one(&state.db)
        .await?;
    Ok(Json(row))
}
pub async fn replace_habit(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<CreateHabit>,
) -> Result<Json<HabitRow>, AppError> {
    let full_id = resolve_habit_id(&state.db, &id).await?;
    sqlx::query(
        "UPDATE habits SET title=?, description=?, recurrence=?, start_time=?, end_time=?, avg_minutes=?, sigma_minutes=?, parallelizable=?, allows_parallel=?, abandonability=?, updated_at=datetime('now') WHERE id = ?"
    )
    .bind(&body.title)
    .bind(&body.description)
    .bind(&body.recurrence)
    .bind(&body.start_time)
    .bind(&body.end_time)
    .bind(body.avg_minutes)
    .bind(body.sigma_minutes)
    .bind(body.parallelizable)
    .bind(body.allows_parallel)
    .bind(body.abandonability)
    .bind(&full_id)
    .execute(&state.db).await?;
    let row = sqlx::query_as::<_, HabitRow>("SELECT * FROM habits WHERE id = ?")
        .bind(&full_id)
        .fetch_one(&state.db)
        .await?;
    Ok(Json(row))
}
pub async fn delete_habit(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, AppError> {
    let full_id = resolve_habit_id(&state.db, &id).await?;
    sqlx::query("DELETE FROM habits WHERE id = ?")
        .bind(&full_id)
        .execute(&state.db)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}
