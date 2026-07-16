//! Async storage backend trait.
//!
//! Implemented by `SqliteStorage` (direct `sqlx`) and `WorkersStorage`
//! (reqwest against the Cloudflare Worker + D1). The local server injects
//! the chosen backend into its axum router.

use async_trait::async_trait;

use crate::error::StorageError;
use crate::model::*;

pub type StorageResult<T> = Result<T, StorageError>;

#[async_trait]
pub trait Storage: Send + Sync + 'static {
    async fn verify_token(&self, token: &str) -> StorageResult<bool>;

    async fn list_tasks(&self, query: &TaskQuery) -> StorageResult<Vec<TaskRow>>;
    async fn get_task(&self, id: &str) -> StorageResult<TaskRow>;
    async fn create_task(&self, body: &CreateTask) -> StorageResult<TaskRow>;
    async fn update_task(&self, id: &str, body: &UpdateTask) -> StorageResult<TaskRow>;
    async fn replace_task(&self, id: &str, body: &CreateTask) -> StorageResult<TaskRow>;
    async fn delete_task(&self, id: &str) -> StorageResult<()>;

    async fn list_habits(&self) -> StorageResult<Vec<HabitRow>>;
    async fn get_habit(&self, id: &str) -> StorageResult<HabitRow>;
    async fn create_habit(&self, body: &CreateHabit) -> StorageResult<HabitRow>;
    async fn update_habit(&self, id: &str, body: &UpdateHabit) -> StorageResult<HabitRow>;
    async fn replace_habit(&self, id: &str, body: &CreateHabit) -> StorageResult<HabitRow>;
    async fn delete_habit(&self, id: &str) -> StorageResult<()>;

    // ── Habit scheduled spans (#303 / #503) ─────────────────
    /// List scheduled spans for a single habit.
    async fn list_habit_scheduled_spans(
        &self,
        habit_id: &str,
    ) -> StorageResult<Vec<HabitScheduledSpanRow>>;
    /// List scheduled spans for all habits (used by sync_habit_tasks).
    async fn list_all_habit_scheduled_spans(&self) -> StorageResult<Vec<HabitScheduledSpanRow>>;
    /// Create a scheduled span for a habit.
    async fn create_habit_scheduled_span(
        &self,
        habit_id: &str,
        body: &CreateHabitScheduledSpan,
    ) -> StorageResult<HabitScheduledSpanRow>;
    /// Delete a scheduled span by its id.
    async fn delete_habit_scheduled_span(&self, habit_id: &str, span_id: &str)
    -> StorageResult<()>;

    // ── Habit steps (#95) ─────────────────────────────────
    /// List steps for a single habit, ordered by position.
    async fn list_habit_steps(&self, habit_id: &str) -> StorageResult<Vec<HabitStepRow>>;
    /// List steps for all habits (used by sync_habit_tasks).
    async fn list_all_habit_steps(&self) -> StorageResult<Vec<HabitStepRow>>;
    /// Bulk-replace a habit's steps. Steps with an `id` matching an existing
    /// row are updated; steps without a matching `id` are created; existing
    /// steps absent from `steps` are deleted. Runs atomically. DAG validation
    /// (cycle detection, intra-habit references) is the caller's
    /// responsibility.
    async fn replace_habit_steps(
        &self,
        habit_id: &str,
        steps: &[HabitStepInput],
    ) -> StorageResult<Vec<HabitStepRow>>;

    async fn get_schedule(&self) -> StorageResult<Option<ScheduleRow>>;
    async fn save_schedule(&self, req: &SaveScheduleRequest) -> StorageResult<ScheduleRow>;
    async fn clear_schedule(&self) -> StorageResult<()>;

    async fn create_token(&self, label: Option<&str>) -> StorageResult<TokenCreateResponse>;
    async fn list_tokens(&self) -> StorageResult<Vec<TokenRow>>;
    async fn revoke_token(&self, id: i64) -> StorageResult<()>;

    async fn get_settings(&self) -> StorageResult<SettingsRow>;
    async fn update_settings(&self, body: &UpdateSettings) -> StorageResult<SettingsRow>;

    // ── Skills (#WI-6) ────────────────────────────────────
    async fn list_skills(&self) -> StorageResult<Vec<SkillRow>>;
    async fn get_skill(&self, slug: &str) -> StorageResult<SkillRow>;
    async fn create_skill(&self, body: &CreateSkill) -> StorageResult<SkillRow>;
    async fn update_skill(&self, slug: &str, body: &UpdateSkill) -> StorageResult<SkillRow>;
    async fn delete_skill(&self, slug: &str) -> StorageResult<()>;

    async fn get_gcal_settings(&self) -> StorageResult<GoogleCalSettingsRow>;
    async fn update_gcal_settings(
        &self,
        body: &UpdateGoogleCalSettings,
    ) -> StorageResult<GoogleCalSettingsRow>;
    async fn list_gcal_mappings(&self) -> StorageResult<Vec<GoogleCalEventRow>>;
    async fn upsert_gcal_mappings(&self, mappings: &[(String, String)]) -> StorageResult<()>;
    async fn delete_gcal_mappings(&self, task_ids: &[String]) -> StorageResult<()>;
    async fn clear_gcal_mappings(&self) -> StorageResult<()>;

    /// Backend health check. Returns a short human-readable status string.
    /// For `WorkersStorage` this pings the Cloudflare Worker `/health`;
    /// for `SqliteStorage` it reports the local DB is reachable.
    async fn health_check(&self) -> StorageResult<String>;
}
