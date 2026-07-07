use std::collections::HashMap;
use std::str::FromStr;
use std::sync::Arc;

use jiff::Timestamp;
use serde::{Deserialize, Serialize};
use takusu_core::{NormalDist, Planner, Point, RescheduleRange, SleepConfig, Task as CoreTask};
use takusu_storage::{
    CreateHabit, CreateHabitPause, CreateTask, GoogleCalEventRow, HabitDetail, HabitPauseRow,
    HabitRow, HabitStepInput, HabitStepRow, SaveScheduleRequest, ScheduleEntry, ScheduleRow,
    SettingsRow, Storage, TaskQuery, TaskRow, TokenCreateResponse, TokenRow,
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

/// Reject negative `avg_minutes` / `sigma_minutes`, which would wrap to a
/// huge `u64` slot count in the planner and break the schedule (#269).
fn validate_minutes(avg: i64, sigma: Option<i64>) -> Result<(), AppError> {
    if avg < 0 {
        return Err(AppError::BadRequest(format!(
            "avg_minutes must be >= 0 (got {avg})"
        )));
    }
    if let Some(s) = sigma
        && s < 0
    {
        return Err(AppError::BadRequest(format!(
            "sigma_minutes must be >= 0 (got {s})"
        )));
    }
    Ok(())
}

/// Verify the recurrence string parses as a `RecurrenceRule` so that bad JSON
/// is rejected at the API boundary instead of crashing later (#285).
fn validate_recurrence(recurrence: &str) -> Result<(), AppError> {
    serde_json::from_str::<takusu_habit::RecurrenceRule>(recurrence)
        .map_err(|e| AppError::BadRequest(format!("invalid recurrence: {e}")))?;
    Ok(())
}

/// Validate a `HH:MM` time string (#95).
fn validate_hhmm(s: &str) -> Result<(), AppError> {
    let parts: Vec<&str> = s.split(':').collect();
    if parts.len() != 2 {
        return Err(AppError::BadRequest(format!("invalid time: {s}")));
    }
    let h: u32 = parts[0]
        .parse()
        .map_err(|_| AppError::BadRequest(format!("invalid time: {s}")))?;
    let m: u32 = parts[1]
        .parse()
        .map_err(|_| AppError::BadRequest(format!("invalid time: {s}")))?;
    if h > 23 || m > 59 {
        return Err(AppError::BadRequest(format!("invalid time: {s}")));
    }
    Ok(())
}

/// Validate a bulk-replace step array (#95): per-field sanity + DAG integrity
/// (intra-habit references, cycle detection). Mirrors the worker-side
/// `validate_steps`.
fn validate_steps(steps: &[HabitStepInput]) -> Result<(), AppError> {
    use std::collections::HashMap;

    for s in steps {
        validate_minutes(s.avg_minutes, s.sigma_minutes)?;
        validate_hhmm(&s.start_time)?;
        validate_hhmm(&s.end_time)?;
    }

    // Build id → index map for steps that carry an id. A depends_on reference
    // must point at a sibling step with a known id.
    let mut id_to_idx: HashMap<String, usize> = HashMap::new();
    for (i, s) in steps.iter().enumerate() {
        if let Some(ref id) = s.id {
            id_to_idx.insert(id.clone(), i);
        }
    }

    let mut adj = vec![Vec::new(); steps.len()];
    for (i, s) in steps.iter().enumerate() {
        for dep in &s.depends_on {
            let Some(&dep_idx) = id_to_idx.get(dep) else {
                return Err(AppError::BadRequest(format!(
                    "step depends_on references unknown step id: {dep}"
                )));
            };
            adj[i].push(dep_idx);
        }
    }

    detect_cycle(&adj)?;
    Ok(())
}

/// Topologically sort habit steps by their `depends_on` DAG (#95). Steps with
/// no dependencies come first. Returns indices into `steps`. Cycles are
/// rejected (defensive — validation already caught them at replace time).
fn topo_sort_steps(steps: &[HabitStepRow]) -> Result<Vec<usize>, AppError> {
    let mut id_to_idx: HashMap<String, usize> = HashMap::new();
    for (i, s) in steps.iter().enumerate() {
        id_to_idx.insert(s.id.clone(), i);
    }
    let mut adj: Vec<Vec<usize>> = vec![Vec::new(); steps.len()];
    let mut indeg = vec![0usize; steps.len()];
    for (i, s) in steps.iter().enumerate() {
        let deps: Vec<String> = serde_json::from_str(&s.depends_on).unwrap_or_default();
        for dep in &deps {
            if let Some(&dep_idx) = id_to_idx.get(dep) {
                // edge dep_idx → i (dep must come before i)
                adj[dep_idx].push(i);
                indeg[i] += 1;
            }
        }
    }
    let mut queue: std::collections::VecDeque<usize> =
        (0..steps.len()).filter(|&i| indeg[i] == 0).collect();
    let mut order = Vec::with_capacity(steps.len());
    while let Some(v) = queue.pop_front() {
        order.push(v);
        for &u in &adj[v] {
            indeg[u] -= 1;
            if indeg[u] == 0 {
                queue.push_back(u);
            }
        }
    }
    if order.len() != steps.len() {
        return Err(AppError::BadRequest(
            "circular dependency detected in habit steps".into(),
        ));
    }
    Ok(order)
}

/// Verify the timezone string resolves to a real `jiff::tz::TimeZone` so that
/// typos don't silently fall back to UTC (#277).
fn validate_timezone(tz: &str) -> Result<(), AppError> {
    jiff::tz::TimeZone::get(tz)
        .map(|_| ())
        .map_err(|_| AppError::BadRequest(format!("invalid timezone: {tz}")))
}

/// Build a `CoreTask` for a single step occurrence (#95). The step's window is
/// derived from the occurrence date (taken from `occ_start`) combined with the
/// step's `start_time`/`end_time`. For fixed steps the deadline is the window
/// length (end_time - start_time); otherwise it is `avg_minutes`.
fn step_to_core_task(
    step: &HabitStepRow,
    occ_start: Point,
    tz: &jiff::tz::TimeZone,
) -> Result<CoreTask, AppError> {
    let date = takusu_habit::point_to_date(occ_start, tz)
        .ok_or_else(|| AppError::Internal("occurrence date out of range".into()))?;
    let (sh, sm) = parse_hhmm(&step.start_time);
    let start_time = takusu_habit::TimeOfDay::new(sh, sm).ok_or_else(|| {
        AppError::BadRequest(format!("invalid step start_time: {}", step.start_time))
    })?;
    let start_pt = takusu_habit::date_time_to_point(date, &start_time, tz)
        .ok_or_else(|| AppError::Internal("step start point out of range".into()))?;
    let (eh, em) = parse_hhmm(&step.end_time);
    let start_minutes = sh as i64 * 60 + sm as i64;
    let end_minutes = eh as i64 * 60 + em as i64;
    let avg_slots = (step.avg_minutes / 5) as u64;
    let sigma_slots = (step.sigma_minutes / 5) as u64;
    let end_pt = if step.fixed {
        let diff = end_minutes - start_minutes;
        if diff > 0 {
            start_pt + diff / 5
        } else {
            // overnight fixed step — fall back to avg-based deadline
            start_pt + avg_slots as i64
        }
    } else {
        start_pt + avg_slots as i64
    };
    Ok(CoreTask {
        id: 0,
        start: Some(start_pt),
        end: end_pt,
        cost_estimate: NormalDist::new(avg_slots, sigma_slots),
        depends: vec![],
        parallelizable: step.parallelizable,
        allows_parallel: step.allows_parallel,
        abandonability: step.abandonability,
        fixed: step.fixed,
        habit_group: None,
    })
}

/// Validate that `start` and `end` are real `YYYY-MM-DD` calendar dates and
/// that `start <= end` (#303).
fn validate_pause_dates(start: &str, end: &str) -> Result<(), AppError> {
    let s = parse_calendar_date(start)
        .ok_or_else(|| AppError::BadRequest(format!("invalid start_date: {start}")))?;
    let e = parse_calendar_date(end)
        .ok_or_else(|| AppError::BadRequest(format!("invalid end_date: {end}")))?;
    if s > e {
        return Err(AppError::BadRequest(format!(
            "start_date ({start}) must be <= end_date ({end})"
        )));
    }
    Ok(())
}

/// Parse a `YYYY-MM-DD` string into a `(year, month, day)` tuple if it is a
/// real calendar date, else `None`.
///
/// Enforces zero-padded fields (4-digit year, 2-digit month/day) so that
/// lexicographic comparison against `jiff`'s zero-padded `Date::to_string()`
/// works correctly during pause matching (#303).
fn parse_calendar_date(s: &str) -> Option<(i64, u32, u32)> {
    let parts: Vec<&str> = s.split('-').collect();
    if parts.len() != 3 {
        return None;
    }
    if parts[0].len() != 4 || parts[1].len() != 2 || parts[2].len() != 2 {
        return None;
    }
    let y: i64 = parts[0].parse().ok()?;
    let m: u32 = parts[1].parse().ok()?;
    let d: u32 = parts[2].parse().ok()?;
    if !(1..=12).contains(&m) {
        return None;
    }
    let leap = (y % 4 == 0 && y % 100 != 0) || y % 400 == 0;
    let max_day = match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap => 29,
        2 => 28,
        _ => return None,
    };
    if !(1..=max_day).contains(&d) {
        return None;
    }
    Some((y, m, d))
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

/// ISO文字列 → Point スロット値。`now` は現在時刻。
/// ハードコードされた 5 (分/スロット) は Planner の per と揃っている必要がある。
/// AGENTS.md の「point_to_iso hardcoded 5-minute slots」参照。
/// 変更時は takusu-core, takusu-local-lib, google-cal など全 crate の
/// 5分前提コードを同時に更新すること。
///
/// `tz` はオフセット無しの naive な日時文字列を解釈する際のフォールバック
/// タイムゾーン。過去にモバイルアプリがオフセットを削除した文字列を保存して
/// しまった場合などに救済する。
fn iso_to_point(iso: &str, tz: &jiff::tz::TimeZone) -> Result<Point, AppError> {
    let ts = if iso.eq_ignore_ascii_case("now") {
        Timestamp::now()
    } else {
        match Timestamp::from_str(iso) {
            Ok(ts) => ts,
            Err(_) => {
                // オフセット無しの naive 日時 → tz で解釈するフォールバック
                let dt = jiff::civil::DateTime::from_str(iso)
                    .map_err(|e| AppError::BadRequest(format!("invalid datetime: {e}")))?;
                dt.to_zoned(tz.clone())
                    .map_err(|e| AppError::BadRequest(format!("invalid datetime: {e}")))?
                    .timestamp()
            }
        }
    };
    Ok(Point::from_timestamp(ts, 5))
}

fn point_to_iso(slot: i64) -> String {
    let secs = slot * 5 * 60;
    let ts = Timestamp::from_second(secs).unwrap_or_else(|_| Timestamp::now());
    ts.to_string()
}

/// Point スロット値 → ローカルタイムゾーンの日付文字列 (YYYY-MM-DD)。
/// `point_to_iso` は UTC タイムスタンプを返すため、JST など UTC より東の
/// タイムゾーンで午前 0 時〜 9 时のタスクが前日として扱われてしまう。
/// `sync_habit_tasks` の日付キーはローカル日付で一貫させる必要がある。
fn point_to_local_date(slot: i64, tz: &jiff::tz::TimeZone) -> String {
    let secs = slot * 5 * 60;
    let ts = Timestamp::from_second(secs).unwrap_or_else(|_| Timestamp::now());
    ts.to_zoned(tz.clone()).date().to_string()
}

/// ISO 文字列 → ローカルタイムゾーンの日付文字列 (YYYY-MM-DD)。
/// `task.start_at` (UTC ISO 文字列) からローカル日付を得るために使う。
fn iso_to_local_date(iso: &str, tz: &jiff::tz::TimeZone) -> String {
    if let Ok(ts) = Timestamp::from_str(iso) {
        ts.to_zoned(tz.clone()).date().to_string()
    } else {
        // フォールバック: naive 日付文字列の先頭 10 文字
        iso.chars().take(10).collect()
    }
}

fn detect_cycle(adj: &[Vec<usize>]) -> Result<(), AppError> {
    let n = adj.len();
    let mut color = vec![0u8; n];
    fn dfs(v: usize, adj: &[Vec<usize>], color: &mut [u8]) -> bool {
        color[v] = 1;
        for &u in &adj[v] {
            if color[u] == 1 {
                return true;
            }
            if color[u] == 0 && dfs(u, adj, color) {
                return true;
            }
        }
        color[v] = 2;
        false
    }
    for v in 0..n {
        if color[v] == 0 && dfs(v, adj, &mut color) {
            return Err(AppError::BadRequest("circular dependency detected".into()));
        }
    }
    Ok(())
}

#[allow(clippy::type_complexity)]
fn build_dep_graph(
    tasks: &[TaskRow],
) -> Result<(Vec<Vec<usize>>, HashMap<String, usize>), AppError> {
    let mut id_to_idx: HashMap<String, usize> = HashMap::new();
    for (i, t) in tasks.iter().enumerate() {
        id_to_idx.insert(t.id.clone(), i);
    }
    let mut adj = vec![Vec::new(); tasks.len()];
    for t in tasks {
        let idx = id_to_idx[&t.id];
        let deps: Vec<String> = serde_json::from_str(&t.depends).unwrap_or_default();
        for dep_id in &deps {
            if let Some(&dep_idx) = id_to_idx.get(dep_id) {
                adj[idx].push(dep_idx);
            }
        }
    }
    Ok((adj, id_to_idx))
}

fn habit_row_to_config(
    row: &HabitRow,
    tz: &jiff::tz::TimeZone,
) -> Result<takusu_habit::Habit, AppError> {
    let recurrence: takusu_habit::RecurrenceRule = serde_json::from_str(&row.recurrence)
        .map_err(|e| AppError::BadRequest(format!("invalid recurrence: {e}")))?;
    let (sh, sm) = parse_hhmm(&row.start_time);
    let start_time = takusu_habit::TimeOfDay::new(sh, sm)
        .ok_or_else(|| AppError::BadRequest(format!("invalid start_time: {}", row.start_time)))?;
    let duration = NormalDist::new((row.avg_minutes / 5) as u64, (row.sigma_minutes / 5) as u64);
    let (eh, em) = parse_hhmm(&row.end_time);
    // fixed habit のみ end_time を deadline として使う: end_time - start_time の
    // スロット数を deadline_slots に設定する。これにより Planner は
    // [start_time, end_time] の範囲内でタスクを配置できる。
    // 非 fixed habit は従来通り deadline_slots = None (avg ベース)。
    let deadline_slots = if row.fixed {
        let start_minutes = sh as i64 * 60 + sm as i64;
        let end_minutes = eh as i64 * 60 + em as i64;
        let diff = end_minutes - start_minutes;
        if diff > 0 {
            Some((diff / 5) as u64)
        } else {
            // Overnight habits (end_time < start_time) fall back to
            // avg-based deadline since the window crosses midnight.
            None
        }
    } else {
        None
    };
    Ok(takusu_habit::Habit {
        recurrence,
        start_time,
        tz: tz.clone(),
        duration,
        deadline_slots,
        parallelizable: row.parallelizable,
        allows_parallel: row.allows_parallel,
        abandonability: row.abandonability,
        fixed: row.fixed,
    })
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
        if let Some(tz) = &body.tz {
            validate_timezone(tz)?;
        }
        self.storage
            .update_settings(body)
            .await
            .map_err(storage_to_app)
    }

    // ── Tasks ─────────────────────────────────────────────

    pub async fn create_task(&self, body: &CreateTask) -> Result<TaskRow, AppError> {
        validate_minutes(body.avg_minutes, body.sigma_minutes)?;
        if let Some(dep_ids) = &body.depends
            && !dep_ids.is_empty()
        {
            let tasks = self
                .storage
                .list_tasks(&TaskQuery::default())
                .await
                .map_err(storage_to_app)?;
            let (_adj, id_to_idx) = build_dep_graph(&tasks)?;
            // Resolve display_id numbers / UUID prefixes to full UUIDs before
            // validating against the dep graph (which is keyed by UUID).
            let mut resolved = Vec::with_capacity(dep_ids.len());
            for did in dep_ids {
                let full = self.storage.get_task(did).await.map_err(storage_to_app)?.id;
                if !id_to_idx.contains_key(&full) {
                    return Err(AppError::BadRequest(format!(
                        "depends on unknown task: {did}"
                    )));
                }
                resolved.push(full);
            }
            let mut body = body.clone();
            body.depends = Some(resolved);
            return self
                .storage
                .create_task(&body)
                .await
                .map_err(storage_to_app);
        }
        self.storage.create_task(body).await.map_err(storage_to_app)
    }

    pub async fn list_tasks(&self, query: &TaskQuery) -> Result<Vec<TaskRow>, AppError> {
        self.storage.list_tasks(query).await.map_err(storage_to_app)
    }

    pub async fn get_task(&self, id: &str) -> Result<TaskRow, AppError> {
        self.storage.get_task(id).await.map_err(storage_to_app)
    }

    pub async fn update_task(&self, id: &str, body: &UpdateTask) -> Result<TaskRow, AppError> {
        // Validate minutes if provided. avg_minutes is required to be present
        // only when it is actually set in the update body.
        if let Some(avg) = body.avg_minutes {
            validate_minutes(avg, body.sigma_minutes)?;
        } else if let Some(sigma) = body.sigma_minutes {
            validate_minutes(0, Some(sigma))?;
        }
        let mut body = body.clone();
        if let Some(dep_ids) = &body.depends {
            let tasks = self
                .storage
                .list_tasks(&TaskQuery::default())
                .await
                .map_err(storage_to_app)?;
            let (mut adj, id_to_idx) = build_dep_graph(&tasks)?;
            let full_id = self.storage.get_task(id).await.map_err(storage_to_app)?.id;
            let target_idx = id_to_idx
                .get(&full_id)
                .ok_or_else(|| AppError::NotFound(format!("task {id} not found")))?;
            // Resolve display_id numbers / UUID prefixes to full UUIDs before
            // validating against the dep graph (which is keyed by UUID).
            let mut resolved = Vec::with_capacity(dep_ids.len());
            for did in dep_ids {
                let full = self.storage.get_task(did).await.map_err(storage_to_app)?.id;
                if !id_to_idx.contains_key(&full) {
                    return Err(AppError::BadRequest(format!(
                        "depends on unknown task: {did}"
                    )));
                }
                resolved.push(full);
            }
            adj[*target_idx] = resolved
                .iter()
                .filter_map(|did| id_to_idx.get(did).copied())
                .collect();
            detect_cycle(&adj)?;
            body.depends = Some(resolved);
        }

        // User-edited flag: for habit-derived tasks, mark as user-edited when
        // habit-managed fields are touched by an HTTP request, unless the
        // caller explicitly set user_edited (e.g. "revert to habit" sets false).
        if body.user_edited.is_none() {
            let existing = self.storage.get_task(id).await.map_err(storage_to_app)?;
            if existing.habit_id.is_some() {
                let touched = body.title.is_some()
                    || body.description.is_some()
                    || body.start_at.is_some()
                    || body.end_at.is_some()
                    || body.avg_minutes.is_some()
                    || body.sigma_minutes.is_some()
                    || body.parallelizable.is_some()
                    || body.allows_parallel.is_some()
                    || body.abandonability.is_some()
                    || body.fixed.is_some();
                if touched {
                    body.user_edited = Some(true);
                }
            }
        }

        self.storage
            .update_task(id, &body)
            .await
            .map_err(storage_to_app)
    }

    pub async fn replace_task(&self, id: &str, body: &CreateTask) -> Result<TaskRow, AppError> {
        validate_minutes(body.avg_minutes, body.sigma_minutes)?;
        if let Some(dep_ids) = &body.depends
            && !dep_ids.is_empty()
        {
            let tasks = self
                .storage
                .list_tasks(&TaskQuery::default())
                .await
                .map_err(storage_to_app)?;
            let (mut adj, id_to_idx) = build_dep_graph(&tasks)?;
            let full_id = self.storage.get_task(id).await.map_err(storage_to_app)?.id;
            let target_idx = id_to_idx
                .get(&full_id)
                .ok_or_else(|| AppError::NotFound(format!("task {id} not found")))?;
            // Resolve display_id numbers / UUID prefixes to full UUIDs before
            // validating against the dep graph (which is keyed by UUID).
            let mut resolved = Vec::with_capacity(dep_ids.len());
            for did in dep_ids {
                let full = self.storage.get_task(did).await.map_err(storage_to_app)?.id;
                if !id_to_idx.contains_key(&full) {
                    return Err(AppError::BadRequest(format!(
                        "depends on unknown task: {did}"
                    )));
                }
                resolved.push(full);
            }
            adj[*target_idx] = resolved
                .iter()
                .filter_map(|did| id_to_idx.get(did).copied())
                .collect();
            detect_cycle(&adj)?;
            let mut body = body.clone();
            body.depends = Some(resolved);
            return self
                .storage
                .replace_task(id, &body)
                .await
                .map_err(storage_to_app);
        }
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
                    habit_id: None,
                    fixed: None,
                    habit_step_id: None,
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
        validate_minutes(body.avg_minutes, body.sigma_minutes)?;
        validate_recurrence(&body.recurrence)?;
        self.storage
            .create_habit(body)
            .await
            .map_err(storage_to_app)
    }

    pub async fn list_habits(&self) -> Result<Vec<HabitRow>, AppError> {
        self.storage.list_habits().await.map_err(storage_to_app)
    }

    pub async fn get_habit(&self, id: &str) -> Result<HabitDetail, AppError> {
        let habit = self.storage.get_habit(id).await.map_err(storage_to_app)?;
        let steps = self
            .storage
            .list_habit_steps(id)
            .await
            .map_err(storage_to_app)?;
        Ok(HabitDetail { habit, steps })
    }

    pub async fn update_habit(&self, id: &str, body: &UpdateHabit) -> Result<HabitRow, AppError> {
        if let Some(avg) = body.avg_minutes {
            validate_minutes(avg, body.sigma_minutes)?;
        } else if let Some(sigma) = body.sigma_minutes {
            validate_minutes(0, Some(sigma))?;
        }
        if let Some(recurrence) = &body.recurrence {
            validate_recurrence(recurrence)?;
        }
        self.storage
            .update_habit(id, body)
            .await
            .map_err(storage_to_app)
    }

    pub async fn replace_habit(&self, id: &str, body: &CreateHabit) -> Result<HabitRow, AppError> {
        validate_minutes(body.avg_minutes, body.sigma_minutes)?;
        validate_recurrence(&body.recurrence)?;
        self.storage
            .replace_habit(id, body)
            .await
            .map_err(storage_to_app)
    }

    pub async fn delete_habit(&self, id: &str) -> Result<(), AppError> {
        self.storage.delete_habit(id).await.map_err(storage_to_app)
    }

    // ── Habit pauses (#303) ───────────────────────────────

    pub async fn list_habit_pauses(&self, id: &str) -> Result<Vec<HabitPauseRow>, AppError> {
        self.storage
            .list_habit_pauses(id)
            .await
            .map_err(storage_to_app)
    }

    pub async fn list_all_habit_pauses(&self) -> Result<Vec<HabitPauseRow>, AppError> {
        self.storage
            .list_all_habit_pauses()
            .await
            .map_err(storage_to_app)
    }

    pub async fn create_habit_pause(
        &self,
        id: &str,
        body: &CreateHabitPause,
    ) -> Result<HabitPauseRow, AppError> {
        validate_pause_dates(&body.start_date, &body.end_date)?;
        self.storage
            .create_habit_pause(id, body)
            .await
            .map_err(storage_to_app)
    }

    pub async fn delete_habit_pause(&self, id: &str, pause_id: &str) -> Result<(), AppError> {
        self.storage
            .delete_habit_pause(id, pause_id)
            .await
            .map_err(storage_to_app)
    }

    // ── Habit steps (#95) ───────────────────────────────

    pub async fn list_habit_steps(&self, id: &str) -> Result<Vec<HabitStepRow>, AppError> {
        self.storage
            .list_habit_steps(id)
            .await
            .map_err(storage_to_app)
    }

    pub async fn list_all_habit_steps(&self) -> Result<Vec<HabitStepRow>, AppError> {
        self.storage
            .list_all_habit_steps()
            .await
            .map_err(storage_to_app)
    }

    pub async fn replace_habit_steps(
        &self,
        id: &str,
        steps: &[HabitStepInput],
    ) -> Result<Vec<HabitStepRow>, AppError> {
        validate_steps(steps)?;
        self.storage
            .replace_habit_steps(id, steps)
            .await
            .map_err(storage_to_app)
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
        let tz = jiff::tz::TimeZone::get(&settings.tz).unwrap_or(jiff::tz::TimeZone::UTC);
        let habit_rows = self.sync_habit_tasks(&tz).await?;
        let all_rows: Vec<TaskRow> = task_rows
            .into_iter()
            .chain(habit_rows)
            // load_task_rows で既に status フィルタ済みだが、habit_rows から
            // 来たタスクは created_at=ステータス変更前に取得された可能性があるため
            // 二重チェックする。「scheduled」も含める理由: 前回生成結果のタスクが
            // 既に scheduled 状態になっているため、再生成でそれらも対象にする。
            .filter(|t| t.status == "pending" || t.status == "scheduled")
            .collect();
        let (mut planner, id_map, id_to_idx) =
            self.build_planner(from_point, sleep, &all_rows, &tz)?;

        // #211: 前回スケジュールを参照として渡し、直近タスクの移動に
        // ペナルティを課す（pinではなく軟制約）。SAは必要なら動かせるが、
        // 直近のタスクは前回位置を維持する方が高スコアになる。
        let existing_schedule = self.storage.get_schedule().await.map_err(storage_to_app)?;
        // unwrap_or_default: if the schedule JSON is corrupt, fall back to
        // an empty vec which disables the stability penalty rather than
        // crashing. This is intentionally more forgiving than reschedule
        // (which returns an error on parse failure) because generate is a
        // full regenerate — the user just wants a new schedule.
        let existing_entries: Vec<ScheduleEntry> = existing_schedule
            .as_ref()
            .and_then(|row| serde_json::from_str(&row.schedule).ok())
            .unwrap_or_default();
        if !existing_entries.is_empty() {
            let prev: Vec<(Point, Point, usize)> = existing_entries
                .iter()
                .filter_map(|entry| {
                    let idx = id_to_idx.get(&entry.task_id)?;
                    let s = iso_to_point(&entry.start_at, &tz).ok()?;
                    let e = iso_to_point(&entry.end_at, &tz).ok()?;
                    Some((s, e, *idx))
                })
                .collect();
            planner.set_previous_schedule(&prev);
        }

        let plan = planner.plan();
        let mut entries = self.plan_to_entries(&plan, &id_map);
        // #354: in_progress タスクは planner の対象外だが、save_schedule が
        // スケジュール全体を上書きするため、進行中タスクのスケジュール情報が
        // 消えてしまう。前回スケジュールから in_progress タスクのエントリを
        // 引き継ぐ。
        entries = self
            .preserve_active_entries(entries, &existing_entries, &["in_progress"])
            .await?;
        let mark_ids: Vec<String> = all_rows.iter().map(|t| t.id.clone()).collect();

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
        let now_point = Point::from_timestamp(Timestamp::now(), 5);

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
        let mut active: Vec<TaskRow> = task_rows
            .into_iter()
            .filter(|t| t.status == "pending" || t.status == "scheduled")
            .collect();

        let tz = jiff::tz::TimeZone::get(&settings.tz).unwrap_or(jiff::tz::TimeZone::UTC);
        let habit_rows = self.sync_habit_tasks(&tz).await?;
        active.extend(
            habit_rows
                .into_iter()
                .filter(|t| t.status == "pending" || t.status == "scheduled"),
        );

        let (planner, id_map, id_to_idx) = self.build_planner(now_point, sleep, &active, &tz)?;

        // Note: stability penalty (#211) is intentionally NOT applied here.
        // reschedule is a user-initiated partial reconfiguration — the user
        // explicitly chose which tasks to move, so we don't want to resist
        // that movement. Stability is only for generate_schedule (full
        // regenerate) where the user hasn't expressed a preference.
        let current_schedule: Vec<(Point, Point, usize)> = entries
            .iter()
            .filter_map(|entry| {
                let idx = *id_to_idx.get(&entry.task_id)?;
                let s = iso_to_point(&entry.start_at, &tz).ok()?;
                let e = iso_to_point(&entry.end_at, &tz).ok()?;
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
                    from: iso_to_point(from_str, &tz)?,
                    until: iso_to_point(until_str, &tz)?,
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
                // pinned 条件: task_ids に含まれない (再スケジュール対象外) または
                // 明示的に pinned 指定されたタスクは固定。残りが再配置される。
                // id_map[idx] で planner index → 文字列ID に変換している。
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

        let mut final_entries = self.plan_to_entries(&plan, &id_map);
        // #354: in_progress タスクは planner の対象外なので、再スケジュール時も
        // 進行中タスクのエントリが消えないよう前回スケジュールから引き継ぐ。
        final_entries = self
            .preserve_active_entries(final_entries, &entries, &["in_progress"])
            .await?;
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

        let settings = self.get_settings_or_default().await?;
        let tz = jiff::tz::TimeZone::get(&settings.tz).unwrap_or(jiff::tz::TimeZone::UTC);

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

        let new_start_point = iso_to_point(new_start, &tz)?;
        let task_row = self
            .storage
            .get_task(&full_task_id)
            .await
            .map_err(storage_to_app)?;
        let old_start = iso_to_point(&entries[idx].start_at, &tz)?;
        let old_end = iso_to_point(&entries[idx].end_at, &tz)?;
        let duration = Point::delta(old_end, old_start);
        let new_end = Point(new_start_point.0 + duration);
        let new_entry = ScheduleEntry {
            task_id: full_task_id.clone(),
            start_at: point_to_iso(new_start_point.0),
            end_at: point_to_iso(new_end.0),
        };

        // move_entry は deadline 超過のみチェックする。
        // 依存関係違反、睡眠侵害、並列違反はチェックしない。
        // これは意図的: 手動移動はユーザーの明示的な操作であり、
        // 自動スケジューラの制約をすべて検証すると自由度が下がるため。
        // force=true で強制上書きも可能。
        let mut warnings = Vec::new();
        let task_deadline = iso_to_point(&task_row.end_at, &tz)?;
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

    pub async fn oauth_callback(
        &self,
        code: &str,
        redirect_uri: Option<&str>,
    ) -> Result<(), AppError> {
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

    /// Backend health check. Returns a short status string from the storage
    /// backend (e.g. "worker ok" or "sqlite ok (v3.x)").
    pub async fn health_check(&self) -> Result<String, AppError> {
        self.storage.health_check().await.map_err(storage_to_app)
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
                if !result.failed.is_empty() {
                    let summary = result
                        .failed
                        .iter()
                        .map(|f| format!("{}({}): {}", f.task_id, f.operation, f.error))
                        .collect::<Vec<_>>()
                        .join("; ");
                    tracing::warn!(
                        "google calendar sync: {} failure(s): {summary}",
                        result.failed.len()
                    );
                    return Err(format!(
                        "google calendar sync partially failed: {} operation(s) could not complete — DB and Calendar may diverge",
                        result.failed.len()
                    ));
                }
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

    /// スケジュール生成対象のタスクをロード。
    ///
    /// - task_ids 指定時: 指定された ID のタスクのみ取得。
    ///   存在しない ID は無視される (ユーザーが削除済みのタスクを指定した場合など)。
    ///   これは意図的な設計: 指定 ID の一部が消失しても生成を継続する。
    ///   ただし、ユーザーはどの ID が無視されたか通知されないため、
    ///   API レベルで警告を返す余地がある。
    /// - task_ids なし: 全タスクから pending/scheduled のみをフィルタ。
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

    pub async fn sync_habit_tasks(
        &self,
        tz: &jiff::tz::TimeZone,
    ) -> Result<Vec<TaskRow>, AppError> {
        let habits = self.storage.list_habits().await.map_err(storage_to_app)?;
        let active_habits: Vec<HabitRow> = habits.into_iter().filter(|h| h.active).collect();
        if active_habits.is_empty() {
            return Ok(vec![]);
        }

        let now_ts = Timestamp::now();
        let now = Point::from_timestamp(now_ts, 5);
        // 過去のハビットタスクは生成しない: 過去分を残すと Planner が
        // 開始時刻を過ぎたタスクを今日以降に再配置してしまい、別の日に
        // 実行される問題 (#204/#205/#207) が起きるため、from を今日の
        // 0時 (tz ローカル) にする。今日の 0 時にすることで、今日の
        // 開始時刻を過ぎたハビットタスクも expected に残り、cleanup
        // ループで削除されないようにする。
        // now_ts を再利用して日付境界をまたぐレースを防ぐ。
        // start_of_day() は DST の spring-forward で 0 時が存在しない
        // タイムゾーンでも安全に開始時刻を返す。
        let start_of_today = now_ts
            .to_zoned(tz.clone())
            .start_of_day()
            .map_err(|e| AppError::Internal(format!("start_of_day: {e}")))?
            .timestamp();
        let from = Point::from_timestamp(start_of_today, 5);
        let until = now + 14 * 24 * 12;

        // Habit pauses (#303): fetch all pause periods once and build a
        // habit_id → Vec<(start, end)> map. Occurrences whose local date
        // falls inside any pause period are skipped, so the existing cleanup
        // loop deletes the now-unexpected pending/unedited tasks.
        let all_pauses = self
            .storage
            .list_all_habit_pauses()
            .await
            .map_err(storage_to_app)?;
        let mut pauses_by_habit: HashMap<String, Vec<(String, String)>> = HashMap::new();
        for p in &all_pauses {
            pauses_by_habit
                .entry(p.habit_id.clone())
                .or_default()
                .push((p.start_date.clone(), p.end_date.clone()));
        }

        // Habit steps (#95): fetch all steps once and group by habit_id.
        // Habits with at least one step emit one task per step per occurrence
        // (each with its own window/cost/flags and step-id-keyed depends);
        // habits with no steps keep the legacy single-task-per-occurrence
        // behavior.
        let all_steps = self
            .storage
            .list_all_habit_steps()
            .await
            .map_err(storage_to_app)?;
        let mut steps_by_habit: HashMap<String, Vec<HabitStepRow>> = HashMap::new();
        for s in all_steps {
            steps_by_habit
                .entry(s.habit_id.clone())
                .or_default()
                .push(s);
        }

        // expected entry:
        //   (habit_id, step_id_opt, date, core_task, habit_desc, step_title_opt)
        #[allow(clippy::type_complexity)]
        let mut expected: Vec<(
            String,
            Option<String>,
            String,
            CoreTask,
            Option<String>,
            Option<String>,
        )> = Vec::new();
        for row in &active_habits {
            let config = habit_row_to_config(row, tz)?;
            let mut store = takusu_habit::HabitStore::new();
            store.add(config);
            let pauses = pauses_by_habit.get(&row.id);
            let steps = steps_by_habit.get(&row.id);

            for gt in store.generate(from, until) {
                let start_point = gt.task.start.unwrap_or(Point(0));
                let date = point_to_local_date(start_point.0, tz);
                if let Some(pauses) = pauses
                    && pauses
                        .iter()
                        .any(|(s, e)| date.as_str() >= s.as_str() && date.as_str() <= e.as_str())
                {
                    continue;
                }

                if let Some(steps) = steps
                    && !steps.is_empty()
                {
                    // Multi-step habit: emit one task per step. The habit's
                    // own window/cost is ignored; each step carries its own.
                    // Steps are emitted in topological order so dependencies
                    // are created before dependents. The actual depends
                    // wiring (step ids → task ids) happens in the post-pass
                    // below, after we know the created task ids.
                    let order = topo_sort_steps(steps)?;
                    let occ_start = start_point;
                    for &idx in &order {
                        let step = &steps[idx];
                        let core = step_to_core_task(step, occ_start, tz)?;
                        expected.push((
                            row.id.clone(),
                            Some(step.id.clone()),
                            date.clone(),
                            core,
                            step.description.clone(),
                            Some(step.title.clone()),
                        ));
                    }
                } else {
                    // Legacy single-task habit.
                    expected.push((
                        row.id.clone(),
                        None,
                        date,
                        gt.task,
                        row.description.clone(),
                        None,
                    ));
                }
            }
        }

        let all_tasks = self
            .storage
            .list_tasks(&TaskQuery::default())
            .await
            .map_err(storage_to_app)?;

        // Key: (habit_id, step_id_opt, date). step_id_opt is None for legacy
        // single-task habits and "" is not a valid step id, so the tuple
        // distinguishes step-generated tasks from legacy ones.
        let mut existing_by_key: HashMap<(String, Option<String>, String), TaskRow> =
            HashMap::new();
        for task in &all_tasks {
            if let Some(ref hid) = task.habit_id {
                let date = task
                    .start_at
                    .as_deref()
                    .map(|s| iso_to_local_date(s, tz))
                    .unwrap_or_default();
                if !date.is_empty() {
                    existing_by_key.insert(
                        (hid.clone(), task.habit_step_id.clone(), date),
                        task.clone(),
                    );
                }
            }
        }

        let mut result: Vec<TaskRow> = Vec::new();
        // Per-occurrence map: (habit_id, date) → step_id → created/updated
        // task id, used to wire step depends after the create/update pass.
        let mut occ_task_ids: HashMap<(String, String), HashMap<String, String>> = HashMap::new();

        for (habit_id, step_id_opt, date, core_task, habit_desc, step_title_opt) in &expected {
            let key = (habit_id.clone(), step_id_opt.clone(), date.clone());
            let habit_row = active_habits.iter().find(|h| h.id == *habit_id);
            let title = match (habit_row, step_title_opt) {
                (Some(h), Some(st)) => format!("{} — {} ({})", h.title, st, date),
                (Some(h), None) => format!("{} ({})", h.title, date),
                (None, Some(st)) => format!("{} ({})", st, date),
                (None, None) => format!("habit:{}", date),
            };

            if let Some(existing) = existing_by_key.remove(&key) {
                if existing.status == "pending" && !existing.user_edited {
                    // ユーザーが habit 由来タスクを編集していない場合は、
                    // habit の現在値で全フィールドを上書きする。
                    let update = UpdateTask {
                        start_at: core_task.start.map(|p| point_to_iso(p.0)),
                        end_at: Some(point_to_iso(core_task.end.0)),
                        title: Some(title),
                        description: habit_desc.clone(),
                        avg_minutes: Some(core_task.cost_estimate.avg as i64 * 5),
                        sigma_minutes: Some(core_task.cost_estimate.sigma as i64 * 5),
                        parallelizable: Some(core_task.parallelizable),
                        allows_parallel: Some(core_task.allows_parallel),
                        abandonability: Some(core_task.abandonability),
                        fixed: Some(core_task.fixed),
                        habit_step_id: step_id_opt.clone(),
                        ..Default::default()
                    };
                    let updated = self
                        .storage
                        .update_task(&existing.id, &update)
                        .await
                        .map_err(storage_to_app)?;
                    if let Some(sid) = step_id_opt {
                        occ_task_ids
                            .entry((habit_id.clone(), date.clone()))
                            .or_default()
                            .insert(sid.clone(), updated.id.clone());
                    }
                    result.push(updated);
                } else {
                    // 非 pending またはユーザーが編集済みの場合は何も変更しない。
                    if let Some(sid) = step_id_opt {
                        occ_task_ids
                            .entry((habit_id.clone(), date.clone()))
                            .or_default()
                            .insert(sid.clone(), existing.id.clone());
                    }
                    result.push(existing.clone());
                }
            } else {
                let create = CreateTask {
                    title,
                    start_at: core_task.start.map(|p| point_to_iso(p.0)),
                    end_at: point_to_iso(core_task.end.0),
                    avg_minutes: core_task.cost_estimate.avg as i64 * 5,
                    sigma_minutes: Some(core_task.cost_estimate.sigma as i64 * 5),
                    depends: Some(vec![]),
                    parallelizable: Some(core_task.parallelizable),
                    allows_parallel: Some(core_task.allows_parallel),
                    abandonability: Some(core_task.abandonability),
                    description: habit_desc.clone(),
                    ical_uid: None,
                    habit_id: Some(habit_id.clone()),
                    fixed: Some(core_task.fixed),
                    habit_step_id: step_id_opt.clone(),
                };
                let created = self
                    .storage
                    .create_task(&create)
                    .await
                    .map_err(storage_to_app)?;
                if let Some(sid) = step_id_opt {
                    occ_task_ids
                        .entry((habit_id.clone(), date.clone()))
                        .or_default()
                        .insert(sid.clone(), created.id.clone());
                }
                result.push(created);
            }
        }

        // Wire step depends (#95): for each occurrence, set each step task's
        // depends to the task ids of its step-level dependencies. Only
        // pending + unedited tasks are updated (consistent with the sync
        // overwrite policy above).
        let steps_by_habit_ref = &steps_by_habit;
        for ((habit_id, _date), step_to_task) in &occ_task_ids {
            let Some(steps) = steps_by_habit_ref.get(habit_id) else {
                continue;
            };
            for step in steps {
                let Some(task_id) = step_to_task.get(&step.id) else {
                    continue;
                };
                let deps: Vec<String> = serde_json::from_str(&step.depends_on).unwrap_or_default();
                if deps.is_empty() {
                    continue;
                }
                let mut dep_task_ids: Vec<String> = Vec::new();
                for dep_step_id in &deps {
                    if let Some(dep_task_id) = step_to_task.get(dep_step_id) {
                        dep_task_ids.push(dep_task_id.clone());
                    }
                }
                if dep_task_ids.is_empty() {
                    continue;
                }
                // Find the task row to check pending + unedited.
                let Some(task_row) = result.iter().find(|t| &t.id == task_id) else {
                    continue;
                };
                if task_row.status != "pending" || task_row.user_edited {
                    continue;
                }
                let update = UpdateTask {
                    depends: Some(dep_task_ids),
                    ..Default::default()
                };
                let updated = self
                    .storage
                    .update_task(task_id, &update)
                    .await
                    .map_err(storage_to_app)?;
                // Replace the entry in result.
                if let Some(slot) = result.iter_mut().find(|t| t.id == *task_id) {
                    *slot = updated;
                }
            }
        }

        // 過去の生成で作られたが、今回期待されなくなった習慣タスクを削除。
        // ただし pending かつユーザーが編集していないものだけ:
        // 手動で status 変更したタスク (scheduled, in_progress, completed, skipped) および
        // ユーザーが編集したタスクは削除しない。
        for (_, task) in existing_by_key {
            if task.status == "pending" && !task.user_edited {
                self.storage
                    .delete_task(&task.id)
                    .await
                    .map_err(storage_to_app)?;
            } else {
                result.push(task);
            }
        }

        Ok(result)
    }

    /// Planner を構築し、CoreTask のインデックスと Row ID の対応を返す。
    ///
    /// task_rows の順序が Planner の内部インデックスを決める。
    /// 戻り値:
    /// - planner: SA で最適化する Planner
    /// - id_map: `planner.tasks[i].id` に対応する DB の task row ID
    ///   (planner のタスクインデックス → 文字列ID の O(1) 変換テーブル)
    /// - id_to_idx: 文字列ID → planner のタスクインデックス (逆引き)
    ///   build_planner 内で依存関係解決に使われた後、
    ///   呼び出し元 (reschedule など) でもスケジュールエントリのフィルタリングに使われる。
    ///
    /// id_to_idx は最初に task_rows のインデックスで初期化された後、
    /// planner.add() 後に planner のインデックスで上書きされる。
    /// 両者は同じ順序なので一致するが、一部の add が失敗すると
    /// 不整合が生じる。その場合は関数全体がエラーを返すため問題ない。
    #[allow(clippy::type_complexity)]
    fn build_planner(
        &self,
        start: Point,
        sleep: SleepConfig,
        task_rows: &[TaskRow],
        tz: &jiff::tz::TimeZone,
    ) -> Result<(Planner, Vec<String>, HashMap<String, usize>), AppError> {
        let mut id_to_idx: HashMap<String, usize> = HashMap::new();
        for (i, row) in task_rows.iter().enumerate() {
            id_to_idx.insert(row.id.clone(), i);
        }

        let mut all_depends: Vec<Vec<usize>> = Vec::with_capacity(task_rows.len());
        for row in task_rows {
            let dep_ids: Vec<String> = serde_json::from_str(&row.depends).unwrap_or_default();
            let mut resolved = Vec::new();
            for dep_id in &dep_ids {
                if let Some(&idx) = id_to_idx.get(dep_id) {
                    resolved.push(idx);
                } else {
                    return Err(AppError::BadRequest(format!(
                        "task {} depends on unknown task {}",
                        row.id, dep_id
                    )));
                }
            }
            all_depends.push(resolved);
        }

        detect_cycle(&all_depends)?;

        // #306: Build habit_id → group index map so that tasks from the same
        // habit share a habit_group index, enabling the consistency bonus.
        let mut habit_group_map: std::collections::HashMap<String, usize> =
            std::collections::HashMap::new();
        let mut next_group = 0usize;
        for row in task_rows.iter() {
            if let Some(ref hid) = row.habit_id
                && !habit_group_map.contains_key(hid)
            {
                habit_group_map.insert(hid.clone(), next_group);
                next_group += 1;
            }
        }

        let mut planner = Planner::new(start, sleep);
        let mut id_map: Vec<String> = Vec::with_capacity(task_rows.len());

        for (i, row) in task_rows.iter().enumerate() {
            let start_opt = row
                .start_at
                .as_ref()
                .map(|s| iso_to_point(s, tz))
                .transpose()?;
            let end = iso_to_point(&row.end_at, tz)?;
            let core_task = CoreTask {
                id: planner.tasks().len(),
                start: start_opt,
                end,
                cost_estimate: NormalDist::new(
                    (row.avg_minutes / 5) as u64,
                    (row.sigma_minutes / 5) as u64,
                ),
                depends: all_depends[i].clone(),
                parallelizable: row.parallelizable,
                allows_parallel: row.allows_parallel,
                abandonability: row.abandonability,
                fixed: row.fixed,
                habit_group: row
                    .habit_id
                    .as_ref()
                    .and_then(|hid| habit_group_map.get(hid).copied()),
            };
            planner
                .add(core_task)
                .map_err(|e| AppError::BadRequest(e.to_string()))?;
            id_map.push(row.id.clone());
            id_to_idx.insert(row.id.clone(), planner.tasks().len() - 1);
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

    /// Preserve schedule entries for tasks that are excluded from the planner
    /// (e.g. `in_progress`) so that regenerating or rescheduling the schedule
    /// does not wipe out their schedule info (#354).
    ///
    /// `new_entries` is the freshly computed schedule. `existing_entries` is
    /// the previous schedule. For each task whose status is in `statuses` and
    /// that is not already present in `new_entries`, its previous entry is
    /// carried over verbatim.
    async fn preserve_active_entries(
        &self,
        mut new_entries: Vec<ScheduleEntry>,
        existing_entries: &[ScheduleEntry],
        statuses: &[&str],
    ) -> Result<Vec<ScheduleEntry>, AppError> {
        if existing_entries.is_empty() {
            return Ok(new_entries);
        }
        let mut preserve_ids: std::collections::HashSet<String> = std::collections::HashSet::new();
        for status in statuses {
            let rows = self
                .storage
                .list_tasks(&TaskQuery {
                    status: Some((*status).to_string()),
                    ..Default::default()
                })
                .await
                .map_err(storage_to_app)?;
            for row in rows {
                preserve_ids.insert(row.id);
            }
        }
        if preserve_ids.is_empty() {
            return Ok(new_entries);
        }
        let new_ids: std::collections::HashSet<String> =
            new_entries.iter().map(|e| e.task_id.clone()).collect();
        for entry in existing_entries {
            if preserve_ids.contains(&entry.task_id) && !new_ids.contains(&entry.task_id) {
                new_entries.push(entry.clone());
            }
        }
        Ok(new_entries)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn iso_to_point_with_offset() {
        let tz = jiff::tz::TimeZone::UTC;
        // オフセット付きはそのままパースできる
        let p = iso_to_point("2026-07-04T10:00:00Z", &tz).unwrap();
        let p2 = iso_to_point("2026-07-04T19:00:00+09:00", &tz).unwrap();
        assert_eq!(p.0, p2.0); // 同一時刻
    }

    #[test]
    fn iso_to_point_naive_falls_back_to_tz() {
        let tz = jiff::tz::TimeZone::get("Asia/Tokyo").unwrap();
        // オフセット無しの naive 日時は tz で解釈される
        let naive = iso_to_point("2026-07-04T10:00:00", &tz).unwrap();
        let with_offset = iso_to_point("2026-07-04T10:00:00+09:00", &tz).unwrap();
        assert_eq!(naive.0, with_offset.0);
    }

    #[test]
    fn iso_to_point_now() {
        let tz = jiff::tz::TimeZone::UTC;
        let _ = iso_to_point("now", &tz).unwrap();
    }
}
