use crate::app::AppState;
use crate::error::AppError;
use crate::model::*;
use axum::Json;
use axum::extract::State;

pub async fn get_settings(
    State(state): State<AppState>,
) -> Result<Json<SettingsResponse>, AppError> {
    let row = sqlx::query_as::<_, SettingsRow>("SELECT * FROM settings WHERE id = 'active'")
        .fetch_one(&state.db)
        .await
        .map_err(|_| AppError::NotFound("settings not found".into()))?;

    Ok(Json(SettingsResponse {
        tz: row.tz,
        sleep_start: row.sleep_start,
        sleep_end: row.sleep_end,
    }))
}

pub async fn update_settings(
    State(state): State<AppState>,
    Json(body): Json<UpdateSettings>,
) -> Result<Json<SettingsResponse>, AppError> {
    let existing = sqlx::query_as::<_, SettingsRow>("SELECT * FROM settings WHERE id = 'active'")
        .fetch_one(&state.db)
        .await
        .map_err(|_| AppError::NotFound("settings not found".into()))?;

    let tz = body.tz.unwrap_or(existing.tz);
    let sleep_start = body.sleep_start.unwrap_or(existing.sleep_start);
    let sleep_end = body.sleep_end.unwrap_or(existing.sleep_end);

    sqlx::query(
        "UPDATE settings SET tz = ?, sleep_start = ?, sleep_end = ?, updated_at = datetime('now') WHERE id = 'active'",
    )
    .bind(&tz)
    .bind(&sleep_start)
    .bind(&sleep_end)
    .execute(&state.db)
    .await?;

    Ok(Json(SettingsResponse {
        tz,
        sleep_start,
        sleep_end,
    }))
}
