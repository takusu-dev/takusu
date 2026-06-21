use std::collections::HashMap;
use std::str::FromStr;
use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use jiff::Timestamp;
use serde::Deserialize;
use takusu_core::{NormalDist, Planner, Point, RescheduleRange, SleepConfig, Task as CoreTask};
use takusu_storage::{SaveScheduleRequest, ScheduleEntry, ScheduleRow, TaskRow};

use crate::error::AppError;
use crate::handlers::task::storage_to_app;
use crate::state::AppState;

fn parse_hhmm(s: &str) -> (u8, u8) {
    let parts: Vec<&str> = s.split(':').collect();
    let h: u8 = parts.first().and_then(|s| s.parse().ok()).unwrap_or(0);
    let m: u8 = parts.get(1).and_then(|s| s.parse().ok()).unwrap_or(0);
    (h, m)
}

fn parse_sleep(s: &str, settings: &takusu_storage::SettingsRow) -> SleepConfig {
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

fn iso_to_point(iso: &str) -> Result<Point, AppError> {
    let ts = if iso.eq_ignore_ascii_case("now") {
        Timestamp::now()
    } else {
        Timestamp::from_str(iso)
            .map_err(|e| AppError::BadRequest(format!("invalid datetime: {e}")))?
    };
    Ok(Point::from_timestamp(ts, 5))
}

fn point_to_iso(slot: i64) -> String {
    let secs = slot * 5 * 60;
    let ts = Timestamp::from_second(secs).unwrap_or_else(|_| Timestamp::now());
    ts.to_string()
}

#[derive(Debug, Deserialize)]
pub struct GenerateSchedule {
    pub task_ids: Option<Vec<String>>,
    #[allow(dead_code)]
    pub until: String,
    #[serde(default = "default_sleep")]
    pub sleep: String,
}

fn default_sleep() -> String {
    "recommended".to_string()
}

#[derive(Debug, Deserialize)]
pub struct Reschedule {
    pub mode: String,
    pub from: Option<String>,
    pub until: Option<String>,
    pub task_ids: Option<Vec<String>>,
    #[serde(default)]
    pub pinned: Vec<String>,
    #[serde(default = "default_sleep")]
    pub sleep: String,
}

#[derive(Debug, Deserialize)]
pub struct MoveEntry {
    pub start_at: String,
    #[serde(default)]
    pub force: bool,
}

pub async fn get_schedule(State(state): State<AppState>) -> Result<Json<ScheduleRow>, AppError> {
    let row = state
        .storage
        .get_schedule()
        .await
        .map_err(storage_to_app)?
        .ok_or_else(|| AppError::NotFound("no active schedule".into()))?;
    Ok(Json(row))
}

pub async fn generate_schedule(
    State(state): State<AppState>,
    Json(body): Json<GenerateSchedule>,
) -> Result<Json<ScheduleRow>, AppError> {
    let settings = state
        .storage
        .get_settings()
        .await
        .map_err(storage_to_app)
        .or_else(|e| {
            // Fall back to defaults if not present
            if matches!(e, AppError::NotFound(_)) {
                Ok(takusu_storage::SettingsRow {
                    id: "active".to_string(),
                    tz: "UTC".to_string(),
                    sleep_start: "22:00".to_string(),
                    sleep_end: "06:00".to_string(),
                    created_at: String::new(),
                    updated_at: String::new(),
                })
            } else {
                Err(e)
            }
        })?;
    let sleep = parse_sleep(&body.sleep, &settings);
    let from_point = Point::from_timestamp(Timestamp::now(), 5);
    let task_rows: Vec<TaskRow> = if let Some(ref task_ids) = body.task_ids {
        let mut out = Vec::new();
        for id in task_ids {
            match state.storage.get_task(id).await {
                Ok(t) => out.push(t),
                Err(takusu_storage::StorageError::NotFound(_)) => continue,
                Err(e) => return Err(storage_to_app(e)),
            }
        }
        out
    } else {
        let all = state
            .storage
            .list_tasks(&takusu_storage::TaskQuery::default())
            .await
            .map_err(storage_to_app)?;
        all.into_iter()
            .filter(|t| t.status == "pending" || t.status == "scheduled")
            .collect()
    };

    let mut planner = Planner::new(from_point, sleep);
    let mut id_map: Vec<String> = Vec::new();
    let mut id_to_idx: HashMap<String, usize> = HashMap::new();
    for row in &task_rows {
        let start = row.start_at.as_ref().map(|s| iso_to_point(s)).transpose()?;
        let end = iso_to_point(&row.end_at)?;
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
        let idx = planner
            .add(core_task)
            .map_err(|e| AppError::BadRequest(e.to_string()))?;
        id_map.push(row.id.clone());
        id_to_idx.insert(row.id.clone(), idx);
    }
    let plan = planner.plan();
    let entries: Vec<ScheduleEntry> = plan
        .schedules
        .iter()
        .map(|(s, e, idx)| ScheduleEntry {
            task_id: id_map.get(*idx).cloned().unwrap_or_default(),
            start_at: point_to_iso(s.0),
            end_at: point_to_iso(e.0),
        })
        .collect();
    let mark_ids: Vec<String> = task_rows.iter().map(|t| t.id.clone()).collect();
    let result = state
        .storage
        .save_schedule(&SaveScheduleRequest {
            entries,
            mark_scheduled_task_ids: mark_ids,
        })
        .await
        .map_err(storage_to_app)?;
    spawn_sync(state.clone());
    Ok(Json(result))
}

pub async fn reschedule(
    State(state): State<AppState>,
    Json(body): Json<Reschedule>,
) -> Result<Json<ScheduleRow>, AppError> {
    let settings = state
        .storage
        .get_settings()
        .await
        .map_err(storage_to_app)
        .or_else(|e| {
            if matches!(e, AppError::NotFound(_)) {
                Ok(takusu_storage::SettingsRow {
                    id: "active".to_string(),
                    tz: "UTC".to_string(),
                    sleep_start: "22:00".to_string(),
                    sleep_end: "06:00".to_string(),
                    created_at: String::new(),
                    updated_at: String::new(),
                })
            } else {
                Err(e)
            }
        })?;
    let sleep = parse_sleep(&body.sleep, &settings);
    let schedule_row = state
        .storage
        .get_schedule()
        .await
        .map_err(storage_to_app)?
        .ok_or_else(|| AppError::NotFound("no active schedule".into()))?;
    let entries: Vec<ScheduleEntry> = serde_json::from_str(&schedule_row.schedule)
        .map_err(|e| AppError::Internal(e.to_string()))?;
    let task_rows = state
        .storage
        .list_tasks(&takusu_storage::TaskQuery::default())
        .await
        .map_err(storage_to_app)?;
    let active: Vec<TaskRow> = task_rows
        .into_iter()
        .filter(|t| t.status == "pending" || t.status == "scheduled")
        .collect();

    let mut planner = Planner::new(Point(0), sleep);
    let mut id_map: Vec<String> = Vec::new();
    let mut id_to_idx: HashMap<String, usize> = HashMap::new();
    for row in &active {
        let start = row.start_at.as_ref().map(|s| iso_to_point(s)).transpose()?;
        let end = iso_to_point(&row.end_at)?;
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
        let idx = planner
            .add(core_task)
            .map_err(|e| AppError::BadRequest(e.to_string()))?;
        id_map.push(row.id.clone());
        id_to_idx.insert(row.id.clone(), idx);
    }

    let mut current_schedule: Vec<(Point, Point, usize)> = Vec::new();
    for entry in &entries {
        if let Some(&idx) = id_to_idx.get(&entry.task_id) {
            let s = iso_to_point(&entry.start_at)?;
            let e = iso_to_point(&entry.end_at)?;
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
                    from: iso_to_point(from_str)?,
                    until: iso_to_point(until_str)?,
                };
                let extra_pinned: Vec<usize> = body
                    .pinned
                    .iter()
                    .filter_map(|pid| id_to_idx.get(pid).copied())
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
        .map(|(s, e, idx)| ScheduleEntry {
            task_id: id_map.get(*idx).cloned().unwrap_or_default(),
            start_at: point_to_iso(s.0),
            end_at: point_to_iso(e.0),
        })
        .collect();
    let result = state
        .storage
        .save_schedule(&SaveScheduleRequest {
            entries: final_entries,
            mark_scheduled_task_ids: vec![],
        })
        .await
        .map_err(storage_to_app)?;
    spawn_sync(state.clone());
    Ok(Json(result))
}

pub async fn move_entry(
    State(state): State<AppState>,
    Path(task_id): Path<String>,
    Json(body): Json<MoveEntry>,
) -> Result<Json<serde_json::Value>, AppError> {
    let full_task_id = state
        .storage
        .get_task(&task_id)
        .await
        .map(|t| t.id)
        .map_err(storage_to_app)?;
    let schedule_row = state
        .storage
        .get_schedule()
        .await
        .map_err(storage_to_app)?
        .ok_or_else(|| AppError::NotFound("no active schedule".into()))?;
    let mut entries: Vec<ScheduleEntry> = serde_json::from_str(&schedule_row.schedule)
        .map_err(|e| AppError::Internal(e.to_string()))?;
    let idx = entries
        .iter()
        .position(|e| e.task_id == full_task_id)
        .ok_or_else(|| AppError::NotFound(format!("task {task_id} not in schedule")))?;
    let new_start = iso_to_point(&body.start_at)?;
    let task_row = state
        .storage
        .get_task(&full_task_id)
        .await
        .map_err(storage_to_app)?;
    let old_start = iso_to_point(&entries[idx].start_at)?;
    let old_end = iso_to_point(&entries[idx].end_at)?;
    let duration = Point::delta(old_end, old_start);
    let new_end = Point(new_start.0 + duration);
    let new_entry = ScheduleEntry {
        task_id: full_task_id.clone(),
        start_at: point_to_iso(new_start.0),
        end_at: point_to_iso(new_end.0),
    };
    let mut warnings = Vec::new();
    let task_deadline = iso_to_point(&task_row.end_at)?;
    if new_end.0 > task_deadline.0 {
        warnings.push("deadline_violation".to_string());
    }
    if !warnings.is_empty() && !body.force {
        return Err(AppError::Conflict {
            message: "schedule violations detected".into(),
        });
    }
    entries[idx] = new_entry;
    state
        .storage
        .save_schedule(&SaveScheduleRequest {
            entries,
            mark_scheduled_task_ids: vec![],
        })
        .await
        .map_err(storage_to_app)?;
    spawn_sync(state.clone());
    let entry = &task_row;
    if warnings.is_empty() {
        Ok(Json(serde_json::json!({
            "task_id": entry.id,
            "start_at": point_to_iso(new_start.0),
            "end_at": point_to_iso(new_end.0),
        })))
    } else {
        Ok(Json(serde_json::json!({
            "task_id": entry.id,
            "start_at": point_to_iso(new_start.0),
            "end_at": point_to_iso(new_end.0),
            "warnings": warnings,
        })))
    }
}

pub async fn clear_schedule(State(state): State<AppState>) -> Result<StatusCode, AppError> {
    state
        .storage
        .clear_schedule()
        .await
        .map_err(storage_to_app)?;
    spawn_sync(state.clone());
    Ok(StatusCode::NO_CONTENT)
}

pub(crate) fn spawn_sync(state: AppState) {
    let lock = state.sync_lock.clone();
    tokio::spawn(async move {
        run_sync_with_retry(&state, &lock).await;
    });
}

async fn run_sync_with_retry(state: &AppState, lock: &Arc<tokio::sync::Mutex<()>>) {
    const MAX_RETRIES: u32 = 3;
    let mut delay = std::time::Duration::from_secs(5);
    for attempt in 0..=MAX_RETRIES {
        let _guard = lock.lock().await;
        match do_sync(state).await {
            Ok(()) => return,
            Err(e) => {
                tracing::warn!("sync attempt {}/{MAX_RETRIES} failed: {e}", attempt + 1);
                drop(_guard);
                if attempt < MAX_RETRIES {
                    tokio::time::sleep(delay).await;
                    delay *= 2;
                } else {
                    tracing::error!("google calendar sync failed after {MAX_RETRIES} retries");
                }
            }
        }
    }
}

async fn do_sync(state: &AppState) -> Result<(), String> {
    let settings = state
        .storage
        .get_gcal_settings()
        .await
        .map_err(|e| e.to_string())?;
    let (refresh_token, client_id, client_secret, calendar_id) = match &settings {
        s if s.enabled && s.refresh_token.is_some() => (
            s.refresh_token.clone().unwrap(),
            s.client_id.clone(),
            s.client_secret.clone(),
            s.calendar_id.clone(),
        ),
        _ => return Ok(()),
    };
    let refresh_token = if refresh_token.is_empty() {
        return Ok(());
    } else {
        refresh_token
    };

    let schedule_row = state
        .storage
        .get_schedule()
        .await
        .map_err(|e| e.to_string())?;
    let entries: Option<Vec<ScheduleEntry>> = match schedule_row {
        Some(s) => serde_json::from_str(&s.schedule).ok(),
        None => None,
    };

    let client = google_cal::Client::new(client_id, client_secret, refresh_token, calendar_id);

    match entries {
        Some(entries) => {
            let task_ids: Vec<String> = entries.iter().map(|e| e.task_id.clone()).collect();
            let mut titles: HashMap<String, (String, Option<String>)> = HashMap::new();
            for id in &task_ids {
                if let Ok(t) = state.storage.get_task(id).await {
                    titles.insert(t.id.clone(), (t.title, t.description));
                }
            }
            let db_mappings = state
                .storage
                .list_gcal_mappings()
                .await
                .map_err(|e| e.to_string())?;
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
                        .find(|m| &m.google_event_id == eid)
                        .map(|m| m.task_id.clone())
                })
                .collect();
            state
                .storage
                .upsert_gcal_mappings(&result.mappings)
                .await
                .map_err(|e| e.to_string())?;
            state
                .storage
                .delete_gcal_mappings(&deleted_task_ids)
                .await
                .map_err(|e| e.to_string())?;
            tracing::info!(
                "google calendar sync: created/updated {}, deleted {}",
                result.mappings.len(),
                deleted_task_ids.len()
            );
            Ok(())
        }
        None => {
            tracing::info!("no active schedule, clearing google calendar events");
            let mappings = state
                .storage
                .list_gcal_mappings()
                .await
                .map_err(|e| e.to_string())?;
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
            state
                .storage
                .clear_gcal_mappings()
                .await
                .map_err(|e| e.to_string())?;
            tracing::info!("cleared {} google calendar events", event_ids.len());
            Ok(())
        }
    }
}
