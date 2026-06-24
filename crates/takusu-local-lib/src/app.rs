use std::collections::HashMap;
use std::str::FromStr;
use std::sync::Arc;

use jiff::Timestamp;
use serde::{Deserialize, Serialize};
use takusu_core::{NormalDist, Planner, Point, RescheduleRange, SleepConfig, Task as CoreTask};
use takusu_storage::{
    CreateHabit, CreateTask, GoogleCalEventRow, HabitRow, SaveScheduleRequest, ScheduleEntry,
    ScheduleRow, SettingsRow, Storage, TaskQuery, TaskRow, TokenCreateResponse, TokenRow,
    UpdateGoogleCalSettings, UpdateHabit, UpdateSettings, UpdateTask,
};

use crate::error::AppError;
use crate::error::storage_to_app;
use crate::token_cache::TokenCache;

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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IcalImportResult {
    pub imported: usize,
    pub task_ids: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct GenerateScheduleInput {
    pub task_ids: Option<Vec<String>>,
    pub sleep: String,
}

#[derive(Debug, Clone)]
pub struct RescheduleInput {
    pub mode: String,
    pub from: Option<String>,
    pub until: Option<String>,
    pub task_ids: Option<Vec<String>>,
    pub pinned: Vec<String>,
    pub sleep: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct MoveEntryOutput {
    pub task_id: String,
    pub start_at: String,
    pub end_at: String,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct GoogleCalSettingsOutput {
    pub enabled: bool,
    pub calendar_id: String,
    pub client_id: String,
    pub has_client_secret: bool,
    pub has_refresh_token: bool,
}

fn default_settings_row() -> SettingsRow {
    SettingsRow {
        id: "active".to_string(),
        tz: "UTC".to_string(),
        sleep_start: "22:00".to_string(),
        sleep_end: "06:00".to_string(),
        created_at: String::new(),
        updated_at: String::new(),
    }
}

pub struct TakusuApp {
    pub storage: Arc<dyn Storage>,
    pub root_token: String,
    pub token_cache: Arc<TokenCache>,
}

impl TakusuApp {
    pub fn new(
        storage: Arc<dyn Storage>,
        root_token: String,
        token_cache: Arc<TokenCache>,
    ) -> Self {
        Self {
            storage,
            root_token,
            token_cache,
        }
    }

    // ── Settings ──────────────────────────────────────────

    async fn get_settings_or_default(&self) -> Result<SettingsRow, AppError> {
        self.storage
            .get_settings()
            .await
            .map_err(storage_to_app)
            .or_else(|e| {
                if matches!(e, AppError::NotFound(_)) {
                    Ok(default_settings_row())
                } else {
                    Err(e)
                }
            })
    }

    pub async fn get_settings(&self) -> Result<SettingsRow, AppError> {
        self.storage.get_settings().await.map_err(storage_to_app)
    }

    pub async fn update_settings(&self, body: &UpdateSettings) -> Result<SettingsRow, AppError> {
        self.storage
            .update_settings(body)
            .await
            .map_err(storage_to_app)
    }

    // ── Tasks ─────────────────────────────────────────────

    pub async fn create_task(&self, body: &CreateTask) -> Result<TaskRow, AppError> {
        self.storage.create_task(body).await.map_err(storage_to_app)
    }

    pub async fn list_tasks(&self, query: &TaskQuery) -> Result<Vec<TaskRow>, AppError> {
        self.storage.list_tasks(query).await.map_err(storage_to_app)
    }

    pub async fn get_task(&self, id: &str) -> Result<TaskRow, AppError> {
        self.storage.get_task(id).await.map_err(storage_to_app)
    }

    pub async fn update_task(&self, id: &str, body: &UpdateTask) -> Result<TaskRow, AppError> {
        self.storage
            .update_task(id, body)
            .await
            .map_err(storage_to_app)
    }

    pub async fn replace_task(&self, id: &str, body: &CreateTask) -> Result<TaskRow, AppError> {
        self.storage
            .replace_task(id, body)
            .await
            .map_err(storage_to_app)
    }

    pub async fn delete_task(&self, id: &str) -> Result<(), AppError> {
        self.storage.delete_task(id).await.map_err(storage_to_app)
    }

    pub async fn import_ical(&self, ical_body: &str) -> Result<IcalImportResult, AppError> {
        let events =
            takusu_ical::parse_ical(ical_body).map_err(|e| AppError::BadRequest(e.to_string()))?;
        let mut imported = 0usize;
        let mut task_ids = Vec::new();
        for event in &events {
            if let Some(ref uid) = event.uid
                && self.task_exists_by_ical_uid(uid).await?
            {
                continue;
            }
            let task = self
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
        Ok(IcalImportResult { imported, task_ids })
    }

    async fn task_exists_by_ical_uid(&self, uid: &str) -> Result<bool, AppError> {
        let tasks = self
            .storage
            .list_tasks(&TaskQuery::default())
            .await
            .map_err(storage_to_app)?;
        Ok(tasks.iter().any(|t| t.ical_uid.as_deref() == Some(uid)))
    }

    // ── Habits ────────────────────────────────────────────

    pub async fn create_habit(&self, body: &CreateHabit) -> Result<HabitRow, AppError> {
        self.storage
            .create_habit(body)
            .await
            .map_err(storage_to_app)
    }

    pub async fn list_habits(&self) -> Result<Vec<HabitRow>, AppError> {
        self.storage.list_habits().await.map_err(storage_to_app)
    }

    pub async fn get_habit(&self, id: &str) -> Result<HabitRow, AppError> {
        self.storage.get_habit(id).await.map_err(storage_to_app)
    }

    pub async fn update_habit(&self, id: &str, body: &UpdateHabit) -> Result<HabitRow, AppError> {
        self.storage
            .update_habit(id, body)
            .await
            .map_err(storage_to_app)
    }

    pub async fn replace_habit(&self, id: &str, body: &CreateHabit) -> Result<HabitRow, AppError> {
        self.storage
            .replace_habit(id, body)
            .await
            .map_err(storage_to_app)
    }

    pub async fn delete_habit(&self, id: &str) -> Result<(), AppError> {
        self.storage.delete_habit(id).await.map_err(storage_to_app)
    }

    // ── Schedule ──────────────────────────────────────────

    pub async fn get_schedule(&self) -> Result<ScheduleRow, AppError> {
        let row = self
            .storage
            .get_schedule()
            .await
            .map_err(storage_to_app)?
            .ok_or_else(|| AppError::NotFound("no active schedule".into()))?;
        Ok(row)
    }

    pub async fn generate_schedule(
        &self,
        input: &GenerateScheduleInput,
    ) -> Result<ScheduleRow, AppError> {
        let settings = self.get_settings_or_default().await?;
        let sleep = parse_sleep(&input.sleep, &settings);
        let from_point = Point::from_timestamp(Timestamp::now(), 5);

        let task_rows = self.load_task_rows(input.task_ids.as_ref()).await?;
        let (planner, id_map, _) = self.build_planner(from_point, sleep, &task_rows)?;

        let plan = planner.plan();
        let entries = self.plan_to_entries(&plan, &id_map);
        let mark_ids: Vec<String> = task_rows.iter().map(|t| t.id.clone()).collect();

        let result = self
            .storage
            .save_schedule(&SaveScheduleRequest {
                entries,
                mark_scheduled_task_ids: mark_ids,
            })
            .await
            .map_err(storage_to_app)?;

        if let Err(e) = self.do_sync().await {
            tracing::warn!("google calendar sync failed: {e}");
        }
        Ok(result)
    }

    pub async fn reschedule(&self, input: &RescheduleInput) -> Result<ScheduleRow, AppError> {
        let settings = self.get_settings_or_default().await?;
        let sleep = parse_sleep(&input.sleep, &settings);

        let schedule_row = self
            .storage
            .get_schedule()
            .await
            .map_err(storage_to_app)?
            .ok_or_else(|| AppError::NotFound("no active schedule".into()))?;
        let entries: Vec<ScheduleEntry> = serde_json::from_str(&schedule_row.schedule)
            .map_err(|e| AppError::Internal(e.to_string()))?;

        let task_rows = self
            .storage
            .list_tasks(&TaskQuery::default())
            .await
            .map_err(storage_to_app)?;
        let active: Vec<TaskRow> = task_rows
            .into_iter()
            .filter(|t| t.status == "pending" || t.status == "scheduled")
            .collect();

        let (planner, id_map, id_to_idx) = self.build_planner(Point(0), sleep, &active)?;

        let current_schedule: Vec<(Point, Point, usize)> = entries
            .iter()
            .filter_map(|entry| {
                let idx = *id_to_idx.get(&entry.task_id)?;
                let s = iso_to_point(&entry.start_at).ok()?;
                let e = iso_to_point(&entry.end_at).ok()?;
                Some((s, e, idx))
            })
            .collect();

        let plan = match input.mode.as_str() {
            "range" => {
                let from_str = input.from.as_ref().ok_or_else(|| {
                    AppError::BadRequest("from is required for range mode".into())
                })?;
                let until_str = input.until.as_ref().ok_or_else(|| {
                    AppError::BadRequest("until is required for range mode".into())
                })?;
                let range = RescheduleRange {
                    from: iso_to_point(from_str)?,
                    until: iso_to_point(until_str)?,
                };
                let extra_pinned: Vec<usize> = input
                    .pinned
                    .iter()
                    .filter_map(|pid| id_to_idx.get(pid).copied())
                    .collect();
                planner.plan_in_range(&range, &current_schedule, &extra_pinned)
            }
            "tasks" => {
                let task_ids = input.task_ids.as_ref().ok_or_else(|| {
                    AppError::BadRequest("task_ids is required for tasks mode".into())
                })?;
                let pinned_entries: Vec<(Point, Point, usize)> = current_schedule
                    .iter()
                    .filter(|(_, _, idx)| {
                        let tid = &id_map[*idx];
                        !task_ids.contains(tid) || input.pinned.contains(tid)
                    })
                    .copied()
                    .collect();
                planner.plan_partial(&pinned_entries)
            }
            _ => {
                return Err(AppError::BadRequest(format!(
                    "unknown mode: {}",
                    input.mode
                )));
            }
        };

        let final_entries = self.plan_to_entries(&plan, &id_map);
        let result = self
            .storage
            .save_schedule(&SaveScheduleRequest {
                entries: final_entries,
                mark_scheduled_task_ids: vec![],
            })
            .await
            .map_err(storage_to_app)?;

        if let Err(e) = self.do_sync().await {
            tracing::warn!("google calendar sync failed: {e}");
        }
        Ok(result)
    }

    pub async fn move_entry(
        &self,
        task_id: &str,
        new_start: &str,
        force: bool,
    ) -> Result<MoveEntryOutput, AppError> {
        let full_task_id = self
            .storage
            .get_task(task_id)
            .await
            .map(|t| t.id)
            .map_err(storage_to_app)?;

        let schedule_row = self
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

        let new_start_point = iso_to_point(new_start)?;
        let task_row = self
            .storage
            .get_task(&full_task_id)
            .await
            .map_err(storage_to_app)?;
        let old_start = iso_to_point(&entries[idx].start_at)?;
        let old_end = iso_to_point(&entries[idx].end_at)?;
        let duration = Point::delta(old_end, old_start);
        let new_end = Point(new_start_point.0 + duration);
        let new_entry = ScheduleEntry {
            task_id: full_task_id.clone(),
            start_at: point_to_iso(new_start_point.0),
            end_at: point_to_iso(new_end.0),
        };

        let mut warnings = Vec::new();
        let task_deadline = iso_to_point(&task_row.end_at)?;
        if new_end.0 > task_deadline.0 {
            warnings.push("deadline_violation".to_string());
        }
        if !warnings.is_empty() && !force {
            return Err(AppError::Conflict {
                message: "schedule violations detected".into(),
            });
        }
        entries[idx] = new_entry;
        self.storage
            .save_schedule(&SaveScheduleRequest {
                entries,
                mark_scheduled_task_ids: vec![],
            })
            .await
            .map_err(storage_to_app)?;

        if let Err(e) = self.do_sync().await {
            tracing::warn!("google calendar sync failed: {e}");
        }

        Ok(MoveEntryOutput {
            task_id: task_row.id,
            start_at: point_to_iso(new_start_point.0),
            end_at: point_to_iso(new_end.0),
            warnings,
        })
    }

    pub async fn clear_schedule(&self) -> Result<(), AppError> {
        self.storage
            .clear_schedule()
            .await
            .map_err(storage_to_app)?;
        if let Err(e) = self.do_sync().await {
            tracing::warn!("google calendar sync failed: {e}");
        }
        Ok(())
    }

    // ── Tokens ────────────────────────────────────────────

    pub async fn create_token(&self, label: Option<&str>) -> Result<TokenCreateResponse, AppError> {
        let resp = self
            .storage
            .create_token(label)
            .await
            .map_err(storage_to_app)?;
        self.token_cache.invalidate();
        Ok(resp)
    }

    pub async fn list_tokens(&self) -> Result<Vec<TokenRow>, AppError> {
        self.storage.list_tokens().await.map_err(storage_to_app)
    }

    pub async fn revoke_token(&self, id: i64) -> Result<(), AppError> {
        self.storage
            .revoke_token(id)
            .await
            .map_err(storage_to_app)?;
        self.token_cache.invalidate();
        Ok(())
    }

    // ── Sync / Google Calendar ────────────────────────────

    pub async fn get_gcal_settings(&self) -> Result<GoogleCalSettingsOutput, AppError> {
        let row = self
            .storage
            .get_gcal_settings()
            .await
            .map_err(storage_to_app)?;
        Ok(GoogleCalSettingsOutput {
            enabled: row.enabled,
            calendar_id: row.calendar_id,
            client_id: row.client_id,
            has_client_secret: !row.client_secret.is_empty(),
            has_refresh_token: row.refresh_token.is_some(),
        })
    }

    pub async fn update_gcal_settings(
        &self,
        body: &UpdateGoogleCalSettings,
    ) -> Result<GoogleCalSettingsOutput, AppError> {
        let row = self
            .storage
            .update_gcal_settings(body)
            .await
            .map_err(storage_to_app)?;
        Ok(GoogleCalSettingsOutput {
            enabled: row.enabled,
            calendar_id: row.calendar_id,
            client_id: row.client_id,
            has_client_secret: !row.client_secret.is_empty(),
            has_refresh_token: row.refresh_token.is_some(),
        })
    }

    pub async fn oauth_url(&self, redirect_uri: &str) -> Result<String, AppError> {
        let row = self
            .storage
            .get_gcal_settings()
            .await
            .map_err(storage_to_app)?;
        if row.client_id.is_empty() {
            return Err(AppError::BadRequest(
                "google calendar settings not configured".into(),
            ));
        }
        Ok(google_cal::oauth_url(&row.client_id, redirect_uri))
    }

    pub async fn oauth_callback(&self, code: &str, redirect_uri: &str) -> Result<(), AppError> {
        let row = self
            .storage
            .get_gcal_settings()
            .await
            .map_err(storage_to_app)?;
        if row.client_id.is_empty() || row.client_secret.is_empty() {
            return Err(AppError::BadRequest(
                "google calendar settings not configured".into(),
            ));
        }
        let tokens =
            google_cal::exchange_code(&row.client_id, &row.client_secret, code, redirect_uri)
                .await
                .map_err(|e| AppError::Internal(format!("oauth exchange failed: {e}")))?;
        self.storage
            .update_gcal_settings(&UpdateGoogleCalSettings {
                enabled: None,
                calendar_id: None,
                client_id: None,
                client_secret: None,
                refresh_token: Some(tokens.refresh_token),
            })
            .await
            .map_err(storage_to_app)?;
        Ok(())
    }

    pub async fn list_gcal_mappings(&self) -> Result<Vec<GoogleCalEventRow>, AppError> {
        self.storage
            .list_gcal_mappings()
            .await
            .map_err(storage_to_app)
    }

    pub async fn do_sync(&self) -> Result<(), String> {
        let settings = self
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

        let schedule_row = self
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
                    if let Ok(t) = self.storage.get_task(id).await {
                        titles.insert(t.id.clone(), (t.title, t.description));
                    }
                }
                let db_mappings = self
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
                self.storage
                    .upsert_gcal_mappings(&result.mappings)
                    .await
                    .map_err(|e| e.to_string())?;
                self.storage
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
                let mappings = self
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
                self.storage
                    .clear_gcal_mappings()
                    .await
                    .map_err(|e| e.to_string())?;
                tracing::info!("cleared {} google calendar events", event_ids.len());
                Ok(())
            }
        }
    }

    // ── Helpers ───────────────────────────────────────────

    async fn load_task_rows(
        &self,
        task_ids: Option<&Vec<String>>,
    ) -> Result<Vec<TaskRow>, AppError> {
        if let Some(ids) = task_ids {
            let mut out = Vec::new();
            for id in ids {
                match self.storage.get_task(id).await {
                    Ok(t) => out.push(t),
                    Err(takusu_storage::StorageError::NotFound(_)) => continue,
                    Err(e) => return Err(storage_to_app(e)),
                }
            }
            Ok(out)
        } else {
            let all = self
                .storage
                .list_tasks(&TaskQuery::default())
                .await
                .map_err(storage_to_app)?;
            Ok(all
                .into_iter()
                .filter(|t| t.status == "pending" || t.status == "scheduled")
                .collect())
        }
    }

    #[allow(clippy::type_complexity)]
    fn build_planner(
        &self,
        start: Point,
        sleep: SleepConfig,
        task_rows: &[TaskRow],
    ) -> Result<(Planner, Vec<String>, HashMap<String, usize>), AppError> {
        let mut planner = Planner::new(start, sleep);
        let mut id_map: Vec<String> = Vec::new();
        let mut id_to_idx: HashMap<String, usize> = HashMap::new();
        for row in task_rows {
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
        Ok((planner, id_map, id_to_idx))
    }

    fn plan_to_entries(&self, plan: &takusu_core::Plan, id_map: &[String]) -> Vec<ScheduleEntry> {
        plan.schedules
            .iter()
            .map(|(s, e, idx)| ScheduleEntry {
                task_id: id_map.get(*idx).cloned().unwrap_or_default(),
                start_at: point_to_iso(s.0),
                end_at: point_to_iso(e.0),
            })
            .collect()
    }
}
