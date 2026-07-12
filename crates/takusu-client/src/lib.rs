//! # takusu-client — HTTP client for takusu REST API
//!
//! Provides types and a `Client` for interacting with the takusu REST API.

use serde::{Deserialize, Serialize};

#[derive(Debug)]
pub enum ClientError {
    Http(reqwest::Error),
    Api { status: u16, body: String },
}

impl std::fmt::Display for ClientError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ClientError::Http(e) => write!(f, "HTTP error: {e}"),
            ClientError::Api { status, body } => write!(f, "API error {status}: {body}"),
        }
    }
}

impl From<reqwest::Error> for ClientError {
    fn from(e: reqwest::Error) -> Self {
        ClientError::Http(e)
    }
}

impl std::error::Error for ClientError {}

#[derive(Clone)]
pub struct Client {
    http: reqwest::Client,
    base_url: String,
    token: String,
}

impl Client {
    pub fn new(base_url: &str, token: &str) -> Self {
        Self {
            http: reqwest::Client::new(),
            base_url: base_url.trim_end_matches('/').to_string(),
            token: token.to_string(),
        }
    }

    async fn request(&self, method: reqwest::Method, path: &str) -> reqwest::RequestBuilder {
        self.http
            .request(method, format!("{}{path}", self.base_url))
            .bearer_auth(&self.token)
    }

    // ── Health ──

    pub async fn health(&self) -> Result<String, ClientError> {
        let resp = self
            .http
            .get(format!("{}/health", self.base_url))
            .send()
            .await?;
        Ok(resp.text().await?)
    }

    // ── Task ──

    pub async fn list_tasks(&self, query: &TaskQuery) -> Result<Vec<TaskRow>, ClientError> {
        let mut url = format!("{}/api/tasks", self.base_url);
        let mut sep = '?';
        if let Some(ref s) = query.status {
            url.push_str(&format!("{sep}status={s}"));
            sep = '&';
        }
        if let Some(ref v) = query.from {
            url.push_str(&format!("{sep}from={v}"));
            sep = '&';
        }
        if let Some(ref v) = query.until {
            url.push_str(&format!("{sep}until={v}"));
            sep = '&';
        }
        if let Some(ref v) = query.habit_id {
            url.push_str(&format!("{sep}habit_id={v}"));
        }
        let resp = self.http.get(&url).bearer_auth(&self.token).send().await?;
        let status = resp.status().as_u16();
        if status >= 400 {
            let body = resp.text().await.unwrap_or_default();
            return Err(ClientError::Api { status, body });
        }
        Ok(resp.json().await?)
    }

    pub async fn get_task(&self, id: &str) -> Result<TaskRow, ClientError> {
        let encoded_id = id.replace('#', "%23");
        let resp = self
            .request(reqwest::Method::GET, &format!("/api/tasks/{encoded_id}"))
            .await
            .send()
            .await?;
        let status = resp.status().as_u16();
        if status >= 400 {
            let body = resp.text().await.unwrap_or_default();
            return Err(ClientError::Api { status, body });
        }
        Ok(resp.json().await?)
    }

    pub async fn create_task(&self, body: &CreateTask) -> Result<TaskRow, ClientError> {
        let resp = self
            .request(reqwest::Method::POST, "/api/tasks")
            .await
            .json(body)
            .send()
            .await?;
        let status = resp.status().as_u16();
        if status >= 400 {
            let body = resp.text().await.unwrap_or_default();
            return Err(ClientError::Api { status, body });
        }
        Ok(resp.json().await?)
    }

    pub async fn update_task(&self, id: &str, body: &UpdateTask) -> Result<TaskRow, ClientError> {
        let resp = self
            .request(reqwest::Method::PATCH, &format!("/api/tasks/{id}"))
            .await
            .json(body)
            .send()
            .await?;
        let status = resp.status().as_u16();
        if status >= 400 {
            let body = resp.text().await.unwrap_or_default();
            return Err(ClientError::Api { status, body });
        }
        Ok(resp.json().await?)
    }

    pub async fn replace_task(&self, id: &str, body: &CreateTask) -> Result<TaskRow, ClientError> {
        let resp = self
            .request(reqwest::Method::PUT, &format!("/api/tasks/{id}"))
            .await
            .json(body)
            .send()
            .await?;
        let status = resp.status().as_u16();
        if status >= 400 {
            let body = resp.text().await.unwrap_or_default();
            return Err(ClientError::Api { status, body });
        }
        Ok(resp.json().await?)
    }

    pub async fn delete_task(&self, id: &str) -> Result<(), ClientError> {
        let resp = self
            .request(reqwest::Method::DELETE, &format!("/api/tasks/{id}"))
            .await
            .send()
            .await?;
        let status = resp.status().as_u16();
        if status >= 400 {
            let body = resp.text().await.unwrap_or_default();
            return Err(ClientError::Api { status, body });
        }
        Ok(())
    }

    pub async fn analyze_task_dependencies(
        &self,
    ) -> Result<DependencyAnalysisResponse, ClientError> {
        let resp = self
            .request(reqwest::Method::GET, "/api/tasks/dependency-analysis")
            .await
            .send()
            .await?;
        let status = resp.status().as_u16();
        if status >= 400 {
            let body = resp.text().await.unwrap_or_default();
            return Err(ClientError::Api { status, body });
        }
        Ok(resp.json().await?)
    }

    // ── Habit ──

    pub async fn list_habits(&self) -> Result<Vec<HabitRow>, ClientError> {
        let resp = self
            .request(reqwest::Method::GET, "/api/habits")
            .await
            .send()
            .await?;
        let status = resp.status().as_u16();
        if status >= 400 {
            let body = resp.text().await.unwrap_or_default();
            return Err(ClientError::Api { status, body });
        }
        Ok(resp.json().await?)
    }

    pub async fn get_habit(&self, id: &str) -> Result<HabitDetail, ClientError> {
        let resp = self
            .request(reqwest::Method::GET, &format!("/api/habits/{id}"))
            .await
            .send()
            .await?;
        let status = resp.status().as_u16();
        if status >= 400 {
            let body = resp.text().await.unwrap_or_default();
            return Err(ClientError::Api { status, body });
        }
        Ok(resp.json().await?)
    }

    pub async fn create_habit(&self, body: &CreateHabit) -> Result<HabitRow, ClientError> {
        let resp = self
            .request(reqwest::Method::POST, "/api/habits")
            .await
            .json(body)
            .send()
            .await?;
        let status = resp.status().as_u16();
        if status >= 400 {
            let body = resp.text().await.unwrap_or_default();
            return Err(ClientError::Api { status, body });
        }
        Ok(resp.json().await?)
    }

    pub async fn update_habit(
        &self,
        id: &str,
        body: &UpdateHabit,
    ) -> Result<HabitRow, ClientError> {
        let resp = self
            .request(reqwest::Method::PATCH, &format!("/api/habits/{id}"))
            .await
            .json(body)
            .send()
            .await?;
        let status = resp.status().as_u16();
        if status >= 400 {
            let body = resp.text().await.unwrap_or_default();
            return Err(ClientError::Api { status, body });
        }
        Ok(resp.json().await?)
    }

    pub async fn replace_habit(
        &self,
        id: &str,
        body: &CreateHabit,
    ) -> Result<HabitRow, ClientError> {
        let resp = self
            .request(reqwest::Method::PUT, &format!("/api/habits/{id}"))
            .await
            .json(body)
            .send()
            .await?;
        let status = resp.status().as_u16();
        if status >= 400 {
            let body = resp.text().await.unwrap_or_default();
            return Err(ClientError::Api { status, body });
        }
        Ok(resp.json().await?)
    }

    pub async fn delete_habit(&self, id: &str) -> Result<(), ClientError> {
        let resp = self
            .request(reqwest::Method::DELETE, &format!("/api/habits/{id}"))
            .await
            .send()
            .await?;
        let status = resp.status().as_u16();
        if status >= 400 {
            let body = resp.text().await.unwrap_or_default();
            return Err(ClientError::Api { status, body });
        }
        Ok(())
    }

    // ── Habit pauses (#303) ──

    pub async fn list_habit_pauses(&self, id: &str) -> Result<Vec<HabitPauseRow>, ClientError> {
        let resp = self
            .request(reqwest::Method::GET, &format!("/api/habits/{id}/pauses"))
            .await
            .send()
            .await?;
        let status = resp.status().as_u16();
        if status >= 400 {
            let body = resp.text().await.unwrap_or_default();
            return Err(ClientError::Api { status, body });
        }
        Ok(resp.json().await?)
    }

    pub async fn list_all_habit_pauses(&self) -> Result<Vec<HabitPauseRow>, ClientError> {
        let resp = self
            .request(reqwest::Method::GET, "/api/habits/pauses")
            .await
            .send()
            .await?;
        let status = resp.status().as_u16();
        if status >= 400 {
            let body = resp.text().await.unwrap_or_default();
            return Err(ClientError::Api { status, body });
        }
        Ok(resp.json().await?)
    }

    pub async fn create_habit_pause(
        &self,
        id: &str,
        body: &CreateHabitPause,
    ) -> Result<HabitPauseRow, ClientError> {
        let resp = self
            .request(reqwest::Method::POST, &format!("/api/habits/{id}/pauses"))
            .await
            .json(body)
            .send()
            .await?;
        let status = resp.status().as_u16();
        if status >= 400 {
            let body = resp.text().await.unwrap_or_default();
            return Err(ClientError::Api { status, body });
        }
        Ok(resp.json().await?)
    }

    pub async fn delete_habit_pause(&self, id: &str, pause_id: &str) -> Result<(), ClientError> {
        let resp = self
            .request(
                reqwest::Method::DELETE,
                &format!("/api/habits/{id}/pauses/{pause_id}"),
            )
            .await
            .send()
            .await?;
        let status = resp.status().as_u16();
        if status >= 400 {
            let body = resp.text().await.unwrap_or_default();
            return Err(ClientError::Api { status, body });
        }
        Ok(())
    }

    // ── Habit steps (#95) ──

    pub async fn list_habit_steps(&self, id: &str) -> Result<Vec<HabitStepRow>, ClientError> {
        let resp = self
            .request(reqwest::Method::GET, &format!("/api/habits/{id}/steps"))
            .await
            .send()
            .await?;
        let status = resp.status().as_u16();
        if status >= 400 {
            let body = resp.text().await.unwrap_or_default();
            return Err(ClientError::Api { status, body });
        }
        Ok(resp.json().await?)
    }

    pub async fn list_all_habit_steps(&self) -> Result<Vec<HabitStepRow>, ClientError> {
        let resp = self
            .request(reqwest::Method::GET, "/api/habits/steps")
            .await
            .send()
            .await?;
        let status = resp.status().as_u16();
        if status >= 400 {
            let body = resp.text().await.unwrap_or_default();
            return Err(ClientError::Api { status, body });
        }
        Ok(resp.json().await?)
    }

    pub async fn replace_habit_steps(
        &self,
        id: &str,
        steps: &[HabitStepInput],
    ) -> Result<Vec<HabitStepRow>, ClientError> {
        let resp = self
            .request(reqwest::Method::PUT, &format!("/api/habits/{id}/steps"))
            .await
            .json(steps)
            .send()
            .await?;
        let status = resp.status().as_u16();
        if status >= 400 {
            let body = resp.text().await.unwrap_or_default();
            return Err(ClientError::Api { status, body });
        }
        Ok(resp.json().await?)
    }

    pub async fn analyze_habit_step_dependencies(
        &self,
        habit_id: &str,
    ) -> Result<DependencyAnalysisResponse, ClientError> {
        let resp = self
            .request(
                reqwest::Method::GET,
                &format!("/api/habits/{habit_id}/steps/dependency-analysis"),
            )
            .await
            .send()
            .await?;
        let status = resp.status().as_u16();
        if status >= 400 {
            let body = resp.text().await.unwrap_or_default();
            return Err(ClientError::Api { status, body });
        }
        Ok(resp.json().await?)
    }

    // ── Schedule ──

    pub async fn get_schedule(&self) -> Result<ScheduleRow, ClientError> {
        let resp = self
            .request(reqwest::Method::GET, "/api/schedule")
            .await
            .send()
            .await?;
        let status = resp.status().as_u16();
        if status >= 400 {
            let body = resp.text().await.unwrap_or_default();
            return Err(ClientError::Api { status, body });
        }
        Ok(resp.json().await?)
    }

    pub async fn generate_schedule(
        &self,
        body: &GenerateSchedule,
    ) -> Result<ScheduleRow, ClientError> {
        let resp = self
            .request(reqwest::Method::POST, "/api/schedule/generate")
            .await
            .json(body)
            .send()
            .await?;
        let status = resp.status().as_u16();
        if status >= 400 {
            let body = resp.text().await.unwrap_or_default();
            return Err(ClientError::Api { status, body });
        }
        Ok(resp.json().await?)
    }

    pub async fn reschedule(&self, body: &Reschedule) -> Result<ScheduleRow, ClientError> {
        let resp = self
            .request(reqwest::Method::POST, "/api/schedule/reschedule")
            .await
            .json(body)
            .send()
            .await?;
        let status = resp.status().as_u16();
        if status >= 400 {
            let body = resp.text().await.unwrap_or_default();
            return Err(ClientError::Api { status, body });
        }
        Ok(resp.json().await?)
    }

    pub async fn move_entry(
        &self,
        task_id: &str,
        body: &MoveEntry,
    ) -> Result<serde_json::Value, ClientError> {
        let resp = self
            .request(
                reqwest::Method::PATCH,
                &format!("/api/schedule/entries/{task_id}"),
            )
            .await
            .json(body)
            .send()
            .await?;
        let status = resp.status().as_u16();
        if status >= 400 {
            let body = resp.text().await.unwrap_or_default();
            return Err(ClientError::Api { status, body });
        }
        Ok(resp.json().await?)
    }

    pub async fn clear_schedule(&self) -> Result<(), ClientError> {
        let resp = self
            .request(reqwest::Method::DELETE, "/api/schedule")
            .await
            .send()
            .await?;
        let status = resp.status().as_u16();
        if status >= 400 {
            let body = resp.text().await.unwrap_or_default();
            return Err(ClientError::Api { status, body });
        }
        Ok(())
    }

    // ── Token ──

    pub async fn create_token(
        &self,
        label: Option<&str>,
    ) -> Result<TokenCreateResponse, ClientError> {
        let body = serde_json::json!({ "label": label });
        let resp = self
            .request(reqwest::Method::POST, "/api/tokens")
            .await
            .json(&body)
            .send()
            .await?;
        let status = resp.status().as_u16();
        if status >= 400 {
            let body = resp.text().await.unwrap_or_default();
            return Err(ClientError::Api { status, body });
        }
        Ok(resp.json().await?)
    }

    pub async fn list_tokens(&self) -> Result<Vec<TokenRow>, ClientError> {
        let resp = self
            .request(reqwest::Method::GET, "/api/tokens")
            .await
            .send()
            .await?;
        let status = resp.status().as_u16();
        if status >= 400 {
            let body = resp.text().await.unwrap_or_default();
            return Err(ClientError::Api { status, body });
        }
        Ok(resp.json().await?)
    }

    pub async fn revoke_token(&self, id: i64) -> Result<(), ClientError> {
        let resp = self
            .request(reqwest::Method::DELETE, &format!("/api/tokens/{id}"))
            .await
            .send()
            .await?;
        let status = resp.status().as_u16();
        if status >= 400 {
            let body = resp.text().await.unwrap_or_default();
            return Err(ClientError::Api { status, body });
        }
        Ok(())
    }

    // ── Sync (Google Calendar) ──

    pub async fn get_sync_settings(&self) -> Result<SyncSettingsResponse, ClientError> {
        let resp = self
            .request(reqwest::Method::GET, "/api/sync/settings")
            .await
            .send()
            .await?;
        let status = resp.status().as_u16();
        if status >= 400 {
            let body = resp.text().await.unwrap_or_default();
            return Err(ClientError::Api { status, body });
        }
        Ok(resp.json().await?)
    }

    pub async fn update_sync_settings(
        &self,
        body: &UpdateSyncSettings,
    ) -> Result<SyncSettingsResponse, ClientError> {
        let resp = self
            .request(reqwest::Method::PUT, "/api/sync/settings")
            .await
            .json(body)
            .send()
            .await?;
        let status = resp.status().as_u16();
        if status >= 400 {
            let body = resp.text().await.unwrap_or_default();
            return Err(ClientError::Api { status, body });
        }
        Ok(resp.json().await?)
    }

    pub async fn get_oauth_url(
        &self,
        redirect_uri: &str,
    ) -> Result<serde_json::Value, ClientError> {
        let body = serde_json::json!({ "redirect_uri": redirect_uri });
        let resp = self
            .request(reqwest::Method::POST, "/api/sync/oauth/url")
            .await
            .json(&body)
            .send()
            .await?;
        let status = resp.status().as_u16();
        if status >= 400 {
            let body = resp.text().await.unwrap_or_default();
            return Err(ClientError::Api { status, body });
        }
        Ok(resp.json().await?)
    }

    pub async fn oauth_callback(
        &self,
        code: &str,
        redirect_uri: Option<&str>,
    ) -> Result<serde_json::Value, ClientError> {
        let body = if let Some(uri) = redirect_uri {
            serde_json::json!({ "code": code, "redirect_uri": uri })
        } else {
            serde_json::json!({ "code": code })
        };
        let resp = self
            .request(reqwest::Method::POST, "/api/sync/oauth/callback")
            .await
            .json(&body)
            .send()
            .await?;
        let status = resp.status().as_u16();
        if status >= 400 {
            let body = resp.text().await.unwrap_or_default();
            return Err(ClientError::Api { status, body });
        }
        Ok(resp.json().await?)
    }

    pub async fn trigger_sync(&self) -> Result<serde_json::Value, ClientError> {
        let resp = self
            .request(reqwest::Method::POST, "/api/sync/trigger")
            .await
            .send()
            .await?;
        let status = resp.status().as_u16();
        if status >= 400 {
            let body = resp.text().await.unwrap_or_default();
            return Err(ClientError::Api { status, body });
        }
        Ok(resp.json().await?)
    }

    // ── Settings ──

    pub async fn get_settings(&self) -> Result<SettingsResponse, ClientError> {
        let resp = self
            .request(reqwest::Method::GET, "/api/settings")
            .await
            .send()
            .await?;
        let status = resp.status().as_u16();
        if status >= 400 {
            let body = resp.text().await.unwrap_or_default();
            return Err(ClientError::Api { status, body });
        }
        Ok(resp.json().await?)
    }

    pub async fn update_settings(
        &self,
        body: &UpdateSettings,
    ) -> Result<SettingsResponse, ClientError> {
        let resp = self
            .request(reqwest::Method::PUT, "/api/settings")
            .await
            .json(body)
            .send()
            .await?;
        let status = resp.status().as_u16();
        if status >= 400 {
            let body = resp.text().await.unwrap_or_default();
            return Err(ClientError::Api { status, body });
        }
        Ok(resp.json().await?)
    }
}

// ── Types (mirrors server model.rs) ──

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskRow {
    pub id: String,
    pub display_id: i64,
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
    #[serde(default)]
    pub user_edited: bool,
    #[serde(default)]
    pub fixed: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub habit_step_id: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Serialize)]
pub struct CreateTask {
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_at: Option<String>,
    pub end_at: String,
    pub avg_minutes: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sigma_minutes: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub depends: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parallelizable: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allows_parallel: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub abandonability: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ical_uid: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub habit_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub fixed: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub habit_step_id: Option<String>,
}

#[derive(Debug, Default, Serialize)]
pub struct UpdateTask {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub avg_minutes: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sigma_minutes: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub depends: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parallelizable: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allows_parallel: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub abandonability: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub habit_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_edited: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub fixed: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub habit_step_id: Option<String>,
}

#[derive(Debug, Default)]
pub struct TaskQuery {
    pub status: Option<String>,
    pub from: Option<String>,
    pub until: Option<String>,
    pub habit_id: Option<String>,
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
    pub parallelizable: bool,
    pub allows_parallel: bool,
    pub abandonability: f64,
    pub active: bool,
    #[serde(default)]
    pub fixed: bool,
    #[serde(default)]
    pub window_mode: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Serialize)]
pub struct CreateHabit {
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub recurrence: String,
    pub start_time: String,
    pub end_time: String,
    pub avg_minutes: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sigma_minutes: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parallelizable: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allows_parallel: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub abandonability: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub fixed: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub window_mode: Option<String>,
}

#[derive(Debug, Default, Serialize)]
pub struct UpdateHabit {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recurrence: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_time: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_time: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub avg_minutes: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sigma_minutes: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parallelizable: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allows_parallel: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub abandonability: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub fixed: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub window_mode: Option<String>,
}

/// A pause period that suppresses task generation for a habit (#303).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HabitPauseRow {
    pub id: String,
    pub habit_id: String,
    pub start_date: String,
    pub end_date: String,
    pub reason: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Serialize)]
pub struct CreateHabitPause {
    pub start_date: String,
    pub end_date: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// A step of a multi-step habit (#95).
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
    pub parallelizable: bool,
    pub allows_parallel: bool,
    pub abandonability: f64,
    pub fixed: bool,
    pub depends_on: String,
    pub created_at: String,
}

/// Input element for `PUT /api/habits/:id/steps` (bulk replace, #95).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HabitStepInput {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub position: i64,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub start_time: String,
    pub end_time: String,
    pub avg_minutes: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sigma_minutes: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parallelizable: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allows_parallel: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub abandonability: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
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

/// A node on a dependency witness path (#355).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DependencyNode {
    pub id: String,
    pub title: String,
}

/// A redundant (composite) dependency edge with a witness path (#355).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RedundantDependency {
    pub from: String,
    pub from_title: String,
    pub to: String,
    pub to_title: String,
    pub via: Vec<DependencyNode>,
}

/// Response for `GET /api/tasks/dependency-analysis` and the habit step
/// variant (#355).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DependencyAnalysisResponse {
    pub redundant: Vec<RedundantDependency>,
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

#[derive(Debug, Serialize)]
pub struct GenerateSchedule {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task_ids: Option<Vec<String>>,
    #[serde(default = "default_sleep")]
    pub sleep: String,
}

#[allow(dead_code)]
fn default_sleep() -> String {
    "recommended".to_string()
}

#[derive(Debug, Serialize)]
pub struct Reschedule {
    pub mode: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub until: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task_ids: Option<Vec<String>>,
    #[serde(default)]
    pub pinned: Vec<String>,
    #[serde(default = "default_sleep")]
    pub sleep: String,
}

#[derive(Debug, Serialize)]
pub struct MoveEntry {
    pub start_at: String,
    #[serde(default)]
    pub force: bool,
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

// ── Sync types ──

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncSettingsResponse {
    pub enabled: bool,
    pub calendar_id: String,
    pub client_id: String,
    pub has_client_secret: bool,
    pub has_refresh_token: bool,
}

#[derive(Debug, Serialize)]
pub struct UpdateSyncSettings {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub calendar_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_secret: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub refresh_token: Option<String>,
}

// ── Settings types ──

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SettingsResponse {
    pub tz: String,
    pub sleep_start: String,
    pub sleep_end: String,
    /// #459: 1 日の快適な作業時間（分）。`None` または `0` の場合はデフォルトを使う。
    pub comfortable_minutes: Option<i64>,
    /// #459: 1 日の最大作業時間（分）。`None` または `0` の場合はデフォルトを使う。
    pub maximum_minutes: Option<i64>,
}

#[derive(Debug, Default, Serialize)]
pub struct UpdateSettings {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tz: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sleep_start: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sleep_end: Option<String>,
    /// #459: 1 日の快適な作業時間（分）。`None` または `0` の場合はデフォルトを使う。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub comfortable_minutes: Option<i64>,
    /// #459: 1 日の最大作業時間（分）。`None` または `0` の場合はデフォルトを使う。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub maximum_minutes: Option<i64>,
}
