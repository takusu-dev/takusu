use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct TaskRow {
    pub id: String,
    pub title: String,
    pub description: Option<String>,
    pub start_at: Option<String>,
    pub end_at: String,
    pub avg_minutes: i64,
    pub sigma_minutes: i64,
    pub depends: String,
    pub parallelizable: bool,
    pub allows_parallel: bool,
    pub abandonability: f64,
    pub status: String,
    pub habit_id: Option<String>,
    pub ical_uid: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Deserialize)]
pub struct CreateTask {
    pub title: String,
    pub description: Option<String>,
    pub start_at: Option<String>,
    pub end_at: String,
    pub avg_minutes: i64,
    #[serde(default)]
    pub sigma_minutes: i64,
    #[serde(default)]
    pub depends: Vec<String>,
    #[serde(default)]
    pub parallelizable: bool,
    #[serde(default)]
    pub allows_parallel: bool,
    #[serde(default = "default_abandonability")]
    pub abandonability: f64,
}

fn default_abandonability() -> f64 {
    0.5
}

#[derive(Debug, Deserialize)]
pub struct UpdateTask {
    pub title: Option<String>,
    pub description: Option<String>,
    pub start_at: Option<String>,
    pub end_at: Option<String>,
    pub avg_minutes: Option<i64>,
    pub sigma_minutes: Option<i64>,
    pub depends: Option<Vec<String>>,
    pub parallelizable: Option<bool>,
    pub allows_parallel: Option<bool>,
    pub abandonability: Option<f64>,
    pub status: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct HabitRow {
    pub id: String,
    pub title: String,
    pub description: Option<String>,
    pub recurrence: String,
    pub start_time: String,
    pub end_time: String,
    pub avg_minutes: i64,
    pub sigma_minutes: i64,
    pub parallelizable: bool,
    pub allows_parallel: bool,
    pub abandonability: f64,
    pub active: bool,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Deserialize)]
pub struct CreateHabit {
    pub title: String,
    pub description: Option<String>,
    pub recurrence: String,
    pub start_time: String,
    pub end_time: String,
    pub avg_minutes: i64,
    #[serde(default)]
    pub sigma_minutes: i64,
    #[serde(default)]
    pub parallelizable: bool,
    #[serde(default)]
    pub allows_parallel: bool,
    #[serde(default)]
    pub abandonability: f64,
}

#[derive(Debug, Deserialize)]
pub struct UpdateHabit {
    pub title: Option<String>,
    pub description: Option<String>,
    pub recurrence: Option<String>,
    pub start_time: Option<String>,
    pub end_time: Option<String>,
    pub avg_minutes: Option<i64>,
    pub sigma_minutes: Option<i64>,
    pub parallelizable: Option<bool>,
    pub allows_parallel: Option<bool>,
    pub abandonability: Option<f64>,
    pub active: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct ScheduleRow {
    pub id: String,
    pub created_at: String,
    pub updated_at: String,
    pub schedule: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ScheduleEntry {
    pub task_id: String,
    pub start_at: String,
    pub end_at: String,
}

#[derive(Debug, Deserialize)]
pub struct GenerateSchedule {
    #[serde(default)]
    pub task_ids: Option<Vec<String>>,
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

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct TokenRow {
    pub id: i64,
    pub token_hash: String,
    pub label: Option<String>,
    pub created_by: String,
    pub created_at: String,
    pub revoked_at: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CreateToken {
    pub label: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct GoogleCalSettingsRow {
    pub id: String,
    pub enabled: bool,
    pub calendar_id: String,
    pub client_id: String,
    pub client_secret: String,
    pub refresh_token: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Serialize)]
pub struct GoogleCalSettingsResponse {
    pub enabled: bool,
    pub calendar_id: String,
    pub client_id: String,
    pub has_client_secret: bool,
    pub has_refresh_token: bool,
}

#[derive(Debug, Deserialize)]
pub struct UpdateGoogleCalSettings {
    pub enabled: Option<bool>,
    pub calendar_id: Option<String>,
    pub client_id: Option<String>,
    pub client_secret: Option<String>,
    pub refresh_token: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct GoogleCalEventRow {
    pub task_id: String,
    pub google_event_id: String,
    pub updated_at: String,
}

#[derive(Debug, Deserialize)]
pub struct OAuthCallbackRequest {
    pub code: String,
    pub redirect_uri: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct SettingsRow {
    pub id: String,
    pub tz: String,
    pub sleep_start: String,
    pub sleep_end: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Serialize)]
pub struct SettingsResponse {
    pub tz: String,
    pub sleep_start: String,
    pub sleep_end: String,
}

#[derive(Debug, Deserialize)]
pub struct UpdateSettings {
    pub tz: Option<String>,
    pub sleep_start: Option<String>,
    pub sleep_end: Option<String>,
}
