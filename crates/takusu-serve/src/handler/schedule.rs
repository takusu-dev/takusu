use crate::app::AppState;
use crate::error::AppError;
use crate::model::*;
use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use std::str::FromStr;
use takusu_core::{NormalDist, Planner, Point, RescheduleRange, SleepConfig, Task as CoreTask};
fn parse_hhmm(s: &str) -> (u8, u8) {
    let parts: Vec<&str> = s.split(':').collect();
    let h: u8 = parts.first().and_then(|s| s.parse().ok()).unwrap_or(0);
    let m: u8 = parts.get(1).and_then(|s| s.parse().ok()).unwrap_or(0);
    (h, m)
}

fn parse_sleep(s: &str, settings: &SettingsRow) -> SleepConfig {
    match s {
        "recommended" => {
            let (sh, sm) = parse_hhmm(&settings.sleep_start);
            let (eh, em) = parse_hhmm(&settings.sleep_end);
            let tz = jiff::tz::TimeZone::get(&settings.tz).unwrap_or(jiff::tz::TimeZone::UTC);
            SleepConfig::from_local(5, &tz, sh, sm, eh, em)
        }
        "disabled" => SleepConfig::disabled(),
        custom => {
            let parts: Vec<&str> = custom.splitn(2, '-').collect();
            if parts.len() == 2 {
                let (sh, sm) = parse_hhmm(parts[0]);
                let (eh, em) = parse_hhmm(parts[1]);
                let tz = jiff::tz::TimeZone::get(&settings.tz).unwrap_or(jiff::tz::TimeZone::UTC);
                SleepConfig::from_local(5, &tz, sh, sm, eh, em)
            } else {
                SleepConfig::disabled()
            }
        }
    }
}

async fn get_settings_or_default(db: &sqlx::SqlitePool) -> SettingsRow {
    sqlx::query_as::<_, SettingsRow>("SELECT * FROM settings WHERE id = 'active'")
        .fetch_optional(db)
        .await
        .ok()
        .flatten()
        .unwrap_or_else(|| SettingsRow {
            id: "active".to_string(),
            tz: "UTC".to_string(),
            sleep_start: "22:00".to_string(),
            sleep_end: "06:00".to_string(),
            created_at: String::new(),
            updated_at: String::new(),
        })
}
fn iso_to_point(iso: &str, per: u16) -> Result<Point, AppError> {
    let ts = if iso.eq_ignore_ascii_case("now") {
        jiff::Timestamp::now()
    } else {
        jiff::Timestamp::from_str(iso)
            .map_err(|e| AppError::BadRequest(format!("invalid datetime: {e}")))?
    };
    Ok(Point::from_timestamp(ts, per))
}
fn point_to_iso(slot: i64) -> String {
    let secs = slot * 5 * 60;
    let ts = jiff::Timestamp::from_second(secs).unwrap_or_else(|_| jiff::Timestamp::now());
    ts.to_string()
}
pub async fn get_schedule(State(state): State<AppState>) -> Result<Json<ScheduleRow>, AppError> {
    let row = sqlx::query_as::<_, ScheduleRow>("SELECT * FROM schedules WHERE id = 'active'")
        .fetch_optional(&state.db)
        .await?
        .ok_or_else(|| AppError::NotFound("no active schedule".into()))?;
    Ok(Json(row))
}
pub async fn generate_schedule(
    State(state): State<AppState>,
    Json(body): Json<GenerateSchedule>,
) -> Result<Json<ScheduleRow>, AppError> {
    let settings = get_settings_or_default(&state.db).await;
    let from_point = Point::from_timestamp(jiff::Timestamp::now(), 5);
    let sleep = parse_sleep(&body.sleep, &settings);
    let task_rows = if let Some(ref task_ids) = body.task_ids {
        let placeholders = task_ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
        let sql = format!(
            "SELECT * FROM tasks WHERE id IN ({placeholders}) AND status IN ('pending','scheduled')"
        );
        let mut q = sqlx::query_as::<_, TaskRow>(sqlx::AssertSqlSafe(sql.as_str()));
        for id in task_ids {
            q = q.bind(id);
        }
        q.fetch_all(&state.db).await?
    } else {
        sqlx::query_as::<_, TaskRow>("SELECT * FROM tasks WHERE status IN ('pending', 'scheduled')")
            .fetch_all(&state.db)
            .await?
    };
    let mut planner = Planner::new(from_point, sleep);
    let mut id_map: Vec<String> = Vec::new();
    for row in &task_rows {
        let start = row
            .start_at
            .as_ref()
            .map(|s| iso_to_point(s, 5))
            .transpose()?;
        let end = iso_to_point(&row.end_at, 5)?;
        let core_task = CoreTask {
            id: planner.tasks().len(),
            start,
            end,
            cost_estimate: NormalDist::new(
                (row.avg_minutes / 5) as u64,
                (row.sigma_minutes / 5) as u64,
            ),
            depends: vec![],
            parallelizable: row.parallelizable,
            allows_parallel: row.allows_parallel,
            abandonability: row.abandonability,
        };
        id_map.push(row.id.clone());
        planner
            .add(core_task)
            .map_err(|e| AppError::BadRequest(e.to_string()))?;
    }
    let plan = planner.plan();
    let mut entries = Vec::new();
    for (s, e, idx) in &plan.schedules {
        let task_id = id_map.get(*idx).cloned().unwrap_or_default();
        entries.push(ScheduleEntry {
            task_id,
            start_at: point_to_iso(s.0),
            end_at: point_to_iso(e.0),
        });
    }
    let schedule_json =
        serde_json::to_string(&entries).map_err(|e| AppError::Internal(e.to_string()))?;
    let now = jiff::Timestamp::now().to_string();
    sqlx::query(
        "INSERT INTO schedules (id, created_at, updated_at, schedule) VALUES ('active', ?, ?, ?) ON CONFLICT(id) DO UPDATE SET schedule=excluded.schedule, updated_at=excluded.updated_at"
    )
    .bind(&now).bind(&now).bind(&schedule_json)
    .execute(&state.db).await?;
    for row in &task_rows {
        sqlx::query(
            "UPDATE tasks SET status = 'scheduled', updated_at = datetime('now') WHERE id = ?",
        )
        .bind(&row.id)
        .execute(&state.db)
        .await?;
    }
    let result = sqlx::query_as::<_, ScheduleRow>("SELECT * FROM schedules WHERE id = 'active'")
        .fetch_one(&state.db)
        .await?;
    let db = state.db.clone();
    let lock = state.sync_lock.clone();
    tokio::spawn(async move {
        crate::handler::sync::run_sync_with_retry(&db, &lock).await;
    });
    Ok(Json(result))
}
pub async fn reschedule(
    State(state): State<AppState>,
    Json(body): Json<Reschedule>,
) -> Result<Json<ScheduleRow>, AppError> {
    let settings = get_settings_or_default(&state.db).await;
    let sleep = parse_sleep(&body.sleep, &settings);
    let schedule_row =
        sqlx::query_as::<_, ScheduleRow>("SELECT * FROM schedules WHERE id = 'active'")
            .fetch_optional(&state.db)
            .await?
            .ok_or_else(|| AppError::NotFound("no active schedule".into()))?;
    let entries: Vec<ScheduleEntry> = serde_json::from_str(&schedule_row.schedule)
        .map_err(|e| AppError::Internal(e.to_string()))?;
    let task_rows = sqlx::query_as::<_, TaskRow>(
        "SELECT * FROM tasks WHERE status IN ('pending', 'scheduled')",
    )
    .fetch_all(&state.db)
    .await?;
    let mut planner = Planner::new(Point(0), sleep);
    let mut id_map: Vec<String> = Vec::new();
    let mut core_idx_map: std::collections::HashMap<String, usize> =
        std::collections::HashMap::new();
    for row in &task_rows {
        let start = row
            .start_at
            .as_ref()
            .map(|s| iso_to_point(s, 5))
            .transpose()?;
        let end = iso_to_point(&row.end_at, 5)?;
        let idx = planner.tasks().len();
        let core_task = CoreTask {
            id: idx,
            start,
            end,
            cost_estimate: NormalDist::new(
                (row.avg_minutes / 5) as u64,
                (row.sigma_minutes / 5) as u64,
            ),
            depends: vec![],
            parallelizable: row.parallelizable,
            allows_parallel: row.allows_parallel,
            abandonability: row.abandonability,
        };
        id_map.push(row.id.clone());
        core_idx_map.insert(row.id.clone(), idx);
        planner
            .add(core_task)
            .map_err(|e| AppError::BadRequest(e.to_string()))?;
    }
    let mut current_schedule: Vec<(Point, Point, usize)> = Vec::new();
    for entry in &entries {
        if let Some(&idx) = core_idx_map.get(&entry.task_id) {
            let s = iso_to_point(&entry.start_at, 5)?;
            let e = iso_to_point(&entry.end_at, 5)?;
            current_schedule.push((s, e, idx));
        }
    }
    let plan =
        match body.mode.as_str() {
            "range" => {
                let from_str = body.from.as_ref().ok_or_else(|| {
                    AppError::BadRequest("from is required for range mode".into())
                })?;
                let until_str = body.until.as_ref().ok_or_else(|| {
                    AppError::BadRequest("until is required for range mode".into())
                })?;
                let range = RescheduleRange {
                    from: iso_to_point(from_str, 5)?,
                    until: iso_to_point(until_str, 5)?,
                };
                let extra_pinned: Vec<usize> = body
                    .pinned
                    .iter()
                    .filter_map(|pid| core_idx_map.get(pid).copied())
                    .collect();
                planner.plan_in_range(&range, &current_schedule, &extra_pinned)
            }
            "tasks" => {
                let task_ids = body.task_ids.as_ref().ok_or_else(|| {
                    AppError::BadRequest("task_ids is required for tasks mode".into())
                })?;
                let pinned_entries: Vec<(Point, Point, usize)> = current_schedule
                    .iter()
                    .filter(|(_, _, idx)| {
                        let tid = &id_map[*idx];
                        !task_ids.contains(tid) || body.pinned.contains(tid)
                    })
                    .copied()
                    .collect();
                planner.plan_partial(&pinned_entries)
            }
            _ => return Err(AppError::BadRequest(format!("unknown mode: {}", body.mode))),
        };
    let final_entries: Vec<ScheduleEntry> = plan
        .schedules
        .iter()
        .map(|(s, e, idx)| {
            let task_id = id_map.get(*idx).cloned().unwrap_or_default();
            ScheduleEntry {
                task_id,
                start_at: point_to_iso(s.0),
                end_at: point_to_iso(e.0),
            }
        })
        .collect();
    save_schedule(&state, final_entries).await
}
async fn save_schedule(
    state: &AppState,
    entries: Vec<ScheduleEntry>,
) -> Result<Json<ScheduleRow>, AppError> {
    let schedule_json =
        serde_json::to_string(&entries).map_err(|e| AppError::Internal(e.to_string()))?;
    let now = jiff::Timestamp::now().to_string();
    sqlx::query(
        "INSERT INTO schedules (id, created_at, updated_at, schedule) VALUES ('active', ?, ?, ?) ON CONFLICT(id) DO UPDATE SET schedule=excluded.schedule, updated_at=excluded.updated_at"
    )
    .bind(&now).bind(&now).bind(&schedule_json)
    .execute(&state.db).await?;
    let result = sqlx::query_as::<_, ScheduleRow>("SELECT * FROM schedules WHERE id = 'active'")
        .fetch_one(&state.db)
        .await?;
    let db = state.db.clone();
    let lock = state.sync_lock.clone();
    tokio::spawn(async move {
        crate::handler::sync::run_sync_with_retry(&db, &lock).await;
    });
    Ok(Json(result))
}
pub async fn move_entry(
    State(state): State<AppState>,
    Path(task_id): Path<String>,
    Json(body): Json<MoveEntry>,
) -> Result<Json<serde_json::Value>, AppError> {
    let full_task_id = super::task::resolve_task_id(&state.db, &task_id).await?;
    let schedule_row =
        sqlx::query_as::<_, ScheduleRow>("SELECT * FROM schedules WHERE id = 'active'")
            .fetch_optional(&state.db)
            .await?
            .ok_or_else(|| AppError::NotFound("no active schedule".into()))?;
    let mut entries: Vec<ScheduleEntry> = serde_json::from_str(&schedule_row.schedule)
        .map_err(|e| AppError::Internal(e.to_string()))?;
    let idx = entries
        .iter()
        .position(|e| e.task_id == full_task_id)
        .ok_or_else(|| AppError::NotFound(format!("task {task_id} not in schedule")))?;
    let new_start = iso_to_point(&body.start_at, 5)?;
    let task_row = sqlx::query_as::<_, TaskRow>("SELECT * FROM tasks WHERE id = ?")
        .bind(&full_task_id)
        .fetch_optional(&state.db)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("task {task_id} not found")))?;
    let old_start = iso_to_point(&entries[idx].start_at, 5)?;
    let old_end = iso_to_point(&entries[idx].end_at, 5)?;
    let duration = Point::delta(old_end, old_start);
    let new_end = Point(new_start.0 + duration);
    let new_entry = ScheduleEntry {
        task_id: full_task_id.clone(),
        start_at: point_to_iso(new_start.0),
        end_at: point_to_iso(new_end.0),
    };
    let mut warnings = Vec::new();
    let task_deadline = iso_to_point(&task_row.end_at, 5)?;
    if new_end.0 > task_deadline.0 {
        warnings.push("deadline_violation".to_string());
    }
    if !warnings.is_empty() && !body.force {
        return Err(AppError::Conflict {
            message: "schedule violations detected".into(),
            warnings,
        });
    }
    entries[idx] = new_entry;
    let schedule_json =
        serde_json::to_string(&entries).map_err(|e| AppError::Internal(e.to_string()))?;
    sqlx::query(
        "UPDATE schedules SET schedule = ?, updated_at = datetime('now') WHERE id = 'active'",
    )
    .bind(&schedule_json)
    .execute(&state.db)
    .await?;
    let entry = &entries[idx];
    let db = state.db.clone();
    let lock = state.sync_lock.clone();
    tokio::spawn(async move {
        crate::handler::sync::run_sync_with_retry(&db, &lock).await;
    });
    if warnings.is_empty() {
        Ok(Json(serde_json::json!({
            "task_id": entry.task_id,
            "start_at": entry.start_at,
            "end_at": entry.end_at,
        })))
    } else {
        Ok(Json(serde_json::json!({
            "task_id": entry.task_id,
            "start_at": entry.start_at,
            "end_at": entry.end_at,
            "warnings": warnings,
        })))
    }
}
pub async fn clear_schedule(State(state): State<AppState>) -> Result<StatusCode, AppError> {
    sqlx::query("DELETE FROM schedules WHERE id = 'active'")
        .execute(&state.db)
        .await?;
    let db = state.db.clone();
    let lock = state.sync_lock.clone();
    tokio::spawn(async move {
        crate::handler::sync::run_sync_with_retry(&db, &lock).await;
    });
    Ok(StatusCode::NO_CONTENT)
}
