use crate::app::AppState;
use crate::error::AppError;
use crate::model::*;
use axum::Json;
use axum::extract::State;
use sqlx::Row;
use std::collections::HashMap;

pub async fn get_settings(
    State(state): State<AppState>,
) -> Result<Json<GoogleCalSettingsResponse>, AppError> {
    let row = sqlx::query_as::<_, GoogleCalSettingsRow>(
        "SELECT * FROM google_cal_settings WHERE id = 'active'",
    )
    .fetch_optional(&state.db)
    .await?;

    let row = row.unwrap_or_else(|| GoogleCalSettingsRow {
        id: "active".to_string(),
        enabled: false,
        calendar_id: "primary".to_string(),
        client_id: String::new(),
        client_secret: String::new(),
        refresh_token: None,
        created_at: String::new(),
        updated_at: String::new(),
    });

    Ok(Json(GoogleCalSettingsResponse {
        enabled: row.enabled,
        calendar_id: row.calendar_id,
        client_id: row.client_id,
        has_client_secret: !row.client_secret.is_empty(),
        has_refresh_token: row.refresh_token.is_some(),
    }))
}

pub async fn update_settings(
    State(state): State<AppState>,
    Json(body): Json<UpdateGoogleCalSettings>,
) -> Result<Json<GoogleCalSettingsResponse>, AppError> {
    let existing = sqlx::query_as::<_, GoogleCalSettingsRow>(
        "SELECT * FROM google_cal_settings WHERE id = 'active'",
    )
    .fetch_optional(&state.db)
    .await?;

    let (enabled, calendar_id, client_id, client_secret, refresh_token) = match &existing {
        Some(row) => (
            body.enabled.unwrap_or(row.enabled),
            body.calendar_id
                .clone()
                .unwrap_or_else(|| row.calendar_id.clone()),
            body.client_id
                .clone()
                .unwrap_or_else(|| row.client_id.clone()),
            body.client_secret
                .clone()
                .unwrap_or_else(|| row.client_secret.clone()),
            body.refresh_token
                .clone()
                .or_else(|| row.refresh_token.clone()),
        ),
        None => (
            body.enabled.unwrap_or(false),
            body.calendar_id.unwrap_or_else(|| "primary".to_string()),
            body.client_id.unwrap_or_default(),
            body.client_secret.unwrap_or_default(),
            body.refresh_token.clone(),
        ),
    };

    sqlx::query(
        "INSERT INTO google_cal_settings (id, enabled, calendar_id, client_id, client_secret, refresh_token) \
         VALUES ('active', ?, ?, ?, ?, ?) \
         ON CONFLICT(id) DO UPDATE SET enabled=excluded.enabled, calendar_id=excluded.calendar_id, \
         client_id=excluded.client_id, client_secret=excluded.client_secret, \
         refresh_token=excluded.refresh_token, updated_at=datetime('now')",
    )
    .bind(enabled)
    .bind(&calendar_id)
    .bind(&client_id)
    .bind(&client_secret)
    .bind(&refresh_token)
    .execute(&state.db)
    .await?;

    Ok(Json(GoogleCalSettingsResponse {
        enabled,
        calendar_id,
        client_id,
        has_client_secret: !client_secret.is_empty(),
        has_refresh_token: refresh_token.is_some(),
    }))
}

pub async fn oauth_url(
    State(state): State<AppState>,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, AppError> {
    let redirect_uri = body
        .get("redirect_uri")
        .and_then(|v| v.as_str())
        .ok_or_else(|| AppError::BadRequest("redirect_uri is required".into()))?;

    let row = sqlx::query_as::<_, GoogleCalSettingsRow>(
        "SELECT * FROM google_cal_settings WHERE id = 'active'",
    )
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::BadRequest("google calendar settings not configured".into()))?;

    let url = google_cal::oauth_url(&row.client_id, redirect_uri);
    Ok(Json(serde_json::json!({ "url": url })))
}

pub async fn oauth_callback(
    State(state): State<AppState>,
    Json(body): Json<OAuthCallbackRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    let row = sqlx::query_as::<_, GoogleCalSettingsRow>(
        "SELECT * FROM google_cal_settings WHERE id = 'active'",
    )
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::BadRequest("google calendar settings not configured".into()))?;

    let tokens = google_cal::exchange_code(
        &row.client_id,
        &row.client_secret,
        &body.code,
        &body.redirect_uri,
    )
    .await
    .map_err(|e| AppError::Internal(format!("oauth exchange failed: {e}")))?;

    sqlx::query(
        "UPDATE google_cal_settings SET refresh_token = ?, updated_at = datetime('now') WHERE id = 'active'",
    )
    .bind(&tokens.refresh_token)
    .execute(&state.db)
    .await?;

    Ok(Json(serde_json::json!({
        "refresh_token_set": true,
    })))
}

pub async fn trigger_sync(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, AppError> {
    if let Err(e) = do_sync(&state.db).await {
        tracing::error!("google calendar sync failed: {e}");
    }
    Ok(Json(serde_json::json!({ "status": "sync_triggered" })))
}

async fn get_settings_row(pool: &sqlx::SqlitePool) -> Option<GoogleCalSettingsRow> {
    sqlx::query_as::<_, GoogleCalSettingsRow>(
        "SELECT * FROM google_cal_settings WHERE id = 'active'",
    )
    .fetch_optional(pool)
    .await
    .ok()
    .flatten()
}

async fn get_schedule_entries(pool: &sqlx::SqlitePool) -> Option<Vec<ScheduleEntry>> {
    let row = sqlx::query_as::<_, ScheduleRow>("SELECT * FROM schedules WHERE id = 'active'")
        .fetch_optional(pool)
        .await
        .ok()??;

    serde_json::from_str(&row.schedule).ok()
}

async fn get_task_infos(
    pool: &sqlx::SqlitePool,
    task_ids: &[String],
) -> Result<HashMap<String, (String, Option<String>)>, String> {
    if task_ids.is_empty() {
        return Ok(HashMap::new());
    }
    let placeholders = task_ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
    let sql = format!("SELECT id, title, description FROM tasks WHERE id IN ({placeholders})");
    let mut query = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()));
    for id in task_ids {
        query = query.bind(id);
    }
    let rows = query.fetch_all(pool).await.map_err(|e| e.to_string())?;
    Ok(rows
        .iter()
        .filter_map(|row| {
            let id: String = row.try_get("id").ok()?;
            let title: String = row.try_get("title").ok()?;
            let description: Option<String> = row.try_get("description").ok()?;
            Some((id, (title, description)))
        })
        .collect())
}

async fn get_existing_mappings(pool: &sqlx::SqlitePool) -> Result<Vec<GoogleCalEventRow>, String> {
    sqlx::query_as::<_, GoogleCalEventRow>("SELECT * FROM google_cal_events")
        .fetch_all(pool)
        .await
        .map_err(|e| e.to_string())
}

async fn upsert_mappings(pool: &sqlx::SqlitePool, mappings: &[(String, String)]) -> Result<(), String> {
    for (task_id, event_id) in mappings {
        sqlx::query(
            "INSERT INTO google_cal_events (task_id, google_event_id) VALUES (?, ?) \
             ON CONFLICT(task_id) DO UPDATE SET google_event_id=excluded.google_event_id, updated_at=datetime('now')",
        )
        .bind(task_id)
        .bind(event_id)
        .execute(pool)
        .await
        .map_err(|e| format!("failed to upsert mapping for {task_id}: {e}"))?;
    }
    Ok(())
}

async fn delete_mappings_by_task_ids(pool: &sqlx::SqlitePool, task_ids: &[String]) -> Result<(), String> {
    for task_id in task_ids {
        sqlx::query("DELETE FROM google_cal_events WHERE task_id = ?")
            .bind(task_id)
            .execute(pool)
            .await
            .map_err(|e| format!("failed to delete mapping for {task_id}: {e}"))?;
    }
    Ok(())
}

async fn delete_all_mappings(pool: &sqlx::SqlitePool) -> Result<(), String> {
    sqlx::query("DELETE FROM google_cal_events")
        .execute(pool)
        .await
        .map_err(|e| format!("failed to clear mappings: {e}"))?;
    Ok(())
}

pub async fn do_sync(pool: &sqlx::SqlitePool) -> Result<(), String> {
    let settings = get_settings_row(pool).await;
    let settings = match settings {
        Some(s) if s.enabled && s.refresh_token.is_some() => s,
        _ => return Ok(()),
    };

    let refresh_token = match &settings.refresh_token {
        Some(t) if !t.is_empty() => t.clone(),
        _ => return Ok(()),
    };

    let entries = get_schedule_entries(pool).await;

    let client = google_cal::Client::new(
        settings.client_id,
        settings.client_secret,
        refresh_token,
        settings.calendar_id,
    );

    match entries {
        Some(entries) => {
            let task_ids: Vec<String> = entries.iter().map(|e| e.task_id.clone()).collect();
            let titles = get_task_infos(pool, &task_ids).await?;
            let db_mappings = get_existing_mappings(pool).await?;
            let existing: HashMap<String, String> = db_mappings
                .iter()
                .map(|m| (m.task_id.clone(), m.google_event_id.clone()))
                .collect();

            let sync_entries: Vec<google_cal::SyncEntry> = entries
                .iter()
                .map(|e| {
                    let (summary, description) = titles
                        .get(&e.task_id)
                        .cloned()
                        .unwrap_or_else(|| (e.task_id.clone(), None));
                    google_cal::SyncEntry {
                        task_id: e.task_id.clone(),
                        summary,
                        description,
                        start: e.start_at.clone(),
                        end: e.end_at.clone(),
                    }
                })
                .collect();

            let result = client
                .sync(&sync_entries, &existing)
                .await
                .map_err(|e| e.to_string())?;

            let deleted_task_ids: Vec<String> = result
                .deleted
                .iter()
                .filter_map(|eid| {
                    db_mappings
                        .iter()
                        .find(|m| m.google_event_id == *eid)
                        .map(|m| m.task_id.clone())
                })
                .collect();

            upsert_mappings(pool, &result.mappings).await?;
            delete_mappings_by_task_ids(pool, &deleted_task_ids).await?;

            tracing::info!(
                "google calendar sync: created/updated {}, deleted {}",
                result.mappings.len(),
                deleted_task_ids.len()
            );
            Ok(())
        }
        None => {
            tracing::info!("no active schedule, clearing google calendar events");
            let mappings = get_existing_mappings(pool).await?;
            if mappings.is_empty() {
                return Ok(());
            }
            let event_ids: Vec<(String, String)> = mappings
                .iter()
                .map(|m| (m.task_id.clone(), m.google_event_id.clone()))
                .collect();
            client
                .delete_all(&event_ids)
                .await
                .map_err(|e| e.to_string())?;
            delete_all_mappings(pool).await?;
            tracing::info!("cleared {} google calendar events", event_ids.len());
            Ok(())
        }
    }
}
