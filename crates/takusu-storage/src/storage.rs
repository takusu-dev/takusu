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

    async fn get_schedule(&self) -> StorageResult<Option<ScheduleRow>>;
    async fn save_schedule(&self, req: &SaveScheduleRequest) -> StorageResult<ScheduleRow>;
    async fn clear_schedule(&self) -> StorageResult<()>;

    async fn create_token(&self, label: Option<&str>) -> StorageResult<TokenCreateResponse>;
    async fn list_tokens(&self) -> StorageResult<Vec<TokenRow>>;
    async fn revoke_token(&self, id: i64) -> StorageResult<()>;

    async fn get_settings(&self) -> StorageResult<SettingsRow>;
    async fn update_settings(&self, body: &UpdateSettings) -> StorageResult<SettingsRow>;

    async fn get_gcal_settings(&self) -> StorageResult<GoogleCalSettingsRow>;
    async fn update_gcal_settings(
        &self,
        body: &UpdateGoogleCalSettings,
    ) -> StorageResult<GoogleCalSettingsRow>;
    async fn list_gcal_mappings(&self) -> StorageResult<Vec<GoogleCalEventRow>>;
    async fn upsert_gcal_mappings(&self, mappings: &[(String, String)]) -> StorageResult<()>;
    async fn delete_gcal_mappings(&self, task_ids: &[String]) -> StorageResult<()>;
    async fn clear_gcal_mappings(&self) -> StorageResult<()>;
}
