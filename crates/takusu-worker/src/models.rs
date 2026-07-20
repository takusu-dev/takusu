//! Row types that mirror `takusu_storage` shapes for the JSON wire format.
//! They are intentionally duplicated here to avoid pulling in `sqlx` (whose
//! `FromRow` derive the storage crate uses) into the WASM bundle.

use serde::{Deserialize, Serialize};

pub mod bool_compat {
    use serde::{Deserialize, Deserializer};

    pub fn deserialize<'de, D>(deserializer: D) -> Result<bool, D::Error>
    where
        D: Deserializer<'de>,
    {
        let v = serde_json::Value::deserialize(deserializer)?;
        match v {
            serde_json::Value::Bool(b) => Ok(b),
            serde_json::Value::Number(n) => Ok(n.as_f64().map(|f| f != 0.0).unwrap_or(false)),
            serde_json::Value::Null => Ok(false),
            _ => Err(serde::de::Error::custom(
                "expected bool or number for boolean field",
            )),
        }
    }

    pub fn serialize<S>(value: &bool, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serde::Serialize::serialize(value, serializer)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskRow {
    pub id: String,
    #[serde(default)]
    pub display_id: i64,
    pub title: String,
    pub description: Option<String>,
    pub start_at: Option<String>,
    pub end_at: String,
    pub avg_minutes: i64,
    pub sigma_minutes: i64,
    pub depends: String,
    #[serde(with = "bool_compat", default)]
    pub parallelizable: bool,
    #[serde(with = "bool_compat", default)]
    pub allows_parallel: bool,
    pub abandonability: f64,
    pub status: String,
    pub habit_id: Option<String>,
    pub ical_uid: Option<String>,
    #[serde(with = "bool_compat", default)]
    pub user_edited: bool,
    #[serde(with = "bool_compat", default)]
    pub fixed: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub habit_step_id: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CreateTask {
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start_at: Option<String>,
    pub end_at: String,
    pub avg_minutes: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sigma_minutes: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub depends: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parallelizable: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allows_parallel: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub abandonability: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ical_uid: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub habit_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fixed: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub habit_step_id: Option<String>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct UpdateTask {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub avg_minutes: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sigma_minutes: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub depends: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parallelizable: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allows_parallel: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub abandonability: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub habit_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user_edited: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fixed: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub habit_step_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HabitRow {
    pub id: String,
    #[serde(default)]
    pub display_id: i64,
    pub title: String,
    pub description: Option<String>,
    pub recurrence: String,
    pub start_time: String,
    pub end_time: String,
    pub avg_minutes: i64,
    pub sigma_minutes: i64,
    #[serde(with = "bool_compat", default)]
    pub parallelizable: bool,
    #[serde(with = "bool_compat", default)]
    pub allows_parallel: bool,
    pub abandonability: f64,
    #[serde(with = "bool_compat", default)]
    pub active: bool,
    #[serde(with = "bool_compat", default)]
    pub fixed: bool,
    #[serde(default)]
    pub window_mode: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CreateHabit {
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub recurrence: String,
    pub start_time: String,
    pub end_time: String,
    pub avg_minutes: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sigma_minutes: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parallelizable: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allows_parallel: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub abandonability: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fixed: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub window_mode: Option<String>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct UpdateHabit {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recurrence: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start_time: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end_time: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub avg_minutes: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sigma_minutes: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parallelizable: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allows_parallel: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub abandonability: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fixed: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub window_mode: Option<String>,
}

/// A scheduled span for a habit (#303 / #503).
///
/// Effect depends on `habits.active`:
/// - active habit: span dates suppress task generation (a pause).
/// - disabled habit: span dates enable task generation (an activation window).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HabitScheduledSpanRow {
    pub id: String,
    pub habit_id: String,
    pub start_date: String,
    pub end_date: String,
    pub reason: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CreateHabitScheduledSpan {
    pub start_date: String,
    pub end_date: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// A step of a multi-step habit (#95). Mirrors `takusu_storage::HabitStepRow`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HabitStepRow {
    pub id: String,
    pub habit_id: String,
    pub position: i64,
    pub title: String,
    pub description: Option<String>,
    pub start_time: String,
    pub end_time: String,
    pub avg_minutes: i64,
    pub sigma_minutes: i64,
    #[serde(with = "bool_compat", default)]
    pub parallelizable: bool,
    #[serde(with = "bool_compat", default)]
    pub allows_parallel: bool,
    pub abandonability: f64,
    #[serde(with = "bool_compat", default)]
    pub fixed: bool,
    pub depends_on: String,
    pub created_at: String,
}

/// Input element for `PUT /api/habits/:id/steps` (bulk replace, #95).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HabitStepInput {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub position: i64,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub start_time: String,
    pub end_time: String,
    pub avg_minutes: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sigma_minutes: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parallelizable: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allows_parallel: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub abandonability: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fixed: Option<bool>,
    #[serde(default)]
    pub depends_on: Vec<String>,
}

/// Habit detail response: the habit row plus its steps (#95).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HabitDetail {
    #[serde(flatten)]
    pub habit: HabitRow,
    pub steps: Vec<HabitStepRow>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScheduleRow {
    pub id: String,
    pub created_at: String,
    pub updated_at: String,
    pub schedule: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScheduleEntry {
    pub task_id: String,
    pub start_at: String,
    pub end_at: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SaveScheduleRequest {
    pub entries: Vec<ScheduleEntry>,
    #[serde(default)]
    pub mark_scheduled_task_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenRow {
    pub id: i64,
    pub token_hash: String,
    pub label: Option<String>,
    pub created_by: String,
    pub created_at: String,
    pub revoked_at: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TokenCreateResponse {
    pub id: i64,
    pub token: String,
    pub label: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoogleCalSettingsRow {
    pub id: String,
    #[serde(with = "bool_compat", default)]
    pub enabled: bool,
    pub calendar_id: String,
    pub client_id: String,
    pub client_secret: String,
    pub refresh_token: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct UpdateGoogleCalSettings {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub calendar_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_secret: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refresh_token: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoogleCalEventRow {
    pub task_id: String,
    pub google_event_id: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SettingsRow {
    pub id: String,
    pub tz: String,
    pub sleep_start: String,
    pub sleep_end: String,
    /// #459: 1 日の快適な作業時間（分）。`None` または未設定の場合はデフォルトを使う。
    pub comfortable_minutes: Option<i64>,
    /// #459: 1 日の最大作業時間（分）。`None` または未設定の場合はデフォルトを使う。
    pub maximum_minutes: Option<i64>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillRow {
    pub slug: String,
    pub name: String,
    pub description: String,
    pub body: String,
    #[serde(with = "bool_compat", default)]
    pub built_in: bool,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CreateSkill {
    pub slug: String,
    pub name: String,
    pub description: String,
    pub body: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub built_in: Option<bool>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct UpdateSkill {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryRow {
    pub id: String,
    pub kind: String,
    pub key: String,
    #[serde(skip_serializing)]
    pub normalized_key: String,
    pub content: String,
    #[serde(skip_serializing)]
    pub normalized_content: String,
    pub subject_type: String,
    pub subject_id: String,
    pub source: String,
    pub revision: i64,
    pub created_at: String,
    pub updated_at: String,
    pub last_used_at: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CreateMemory {
    pub kind: String,
    pub key: String,
    pub content: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subject_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subject_id: Option<String>,
    #[serde(default)]
    pub upsert: bool,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct UpdateMemory {
    pub observed_revision: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimilarTaskRow {
    pub task_id: String,
    pub display_id: i64,
    pub title: String,
    pub avg_minutes: i64,
    pub sigma_minutes: i64,
    pub actual_minutes: Option<i64>,
    pub completed_at: Option<String>,
    #[serde(skip_serializing)]
    pub updated_at: String,
    pub similarity: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct UpdateSettings {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tz: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sleep_start: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sleep_end: Option<String>,
    /// #459: 1 日の快適な作業時間（分）。`None` または未設定の場合はデフォルトを使う。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub comfortable_minutes: Option<i64>,
    /// #459: 1 日の最大作業時間（分）。`None` または未設定の場合はデフォルトを使う。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub maximum_minutes: Option<i64>,
}
