//! # takusu-client — HTTP client for takusu REST API
//!
//! Provides types and a `Client` for interacting with the takusu REST API.

use serde::{Deserialize, Serialize};
use std::time::Duration;

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

/// Build a `reqwest::Client` that is safe to use on Android.
///
/// `reqwest` 0.13 defaults to `rustls-platform-verifier` for certificate
/// verification. On Android that verifier requires a JNI context that is not
/// available in the embedded UniFFI runtime, so any HTTPS request panics and
/// kills the server task, surfacing as "unexpected end of stream" to the
/// client. Use bundled webpki root certificates instead on Android.
pub fn default_http_client(
    timeout_seconds: Option<u64>,
) -> Result<reqwest::Client, reqwest::Error> {
    #[cfg(target_os = "android")]
    {
        let certs: Vec<reqwest::Certificate> = webpki_root_certs::TLS_SERVER_ROOT_CERTS
            .iter()
            .filter_map(|c| reqwest::Certificate::from_der(c.as_ref()).ok())
            .collect();
        assert!(
            !certs.is_empty(),
            "no bundled root certificates were loaded; HTTPS cannot be used"
        );
        let mut builder = reqwest::Client::builder()
            .use_rustls_tls()
            .tls_certs_only(certs);
        if let Some(secs) = timeout_seconds {
            builder = builder.timeout(Duration::from_secs(secs));
        }
        builder.build()
    }
    #[cfg(not(target_os = "android"))]
    {
        let mut builder = reqwest::Client::builder();
        if let Some(secs) = timeout_seconds {
            builder = builder.timeout(Duration::from_secs(secs));
        }
        builder.build()
    }
}

impl Client {
    pub fn new(base_url: &str, token: &str) -> Self {
        Self {
            http: default_http_client(None).expect("failed to build HTTP client"),
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
        let url = format!("{}/api/tasks", self.base_url);
        let mut req = self.http.get(&url).bearer_auth(&self.token);
        let mut params: Vec<(&str, &str)> = Vec::new();
        if let Some(ref s) = query.status {
            params.push(("status", s));
        }
        if let Some(ref v) = query.from {
            params.push(("from", v));
        }
        if let Some(ref v) = query.until {
            params.push(("until", v));
        }
        if let Some(v) = query.no_overdue {
            params.push(("no_overdue", if v { "true" } else { "false" }));
        }
        if let Some(ref v) = query.habit_id {
            params.push(("habit_id", v));
        }
        if let Some(ref v) = query.ical_uid {
            params.push(("ical_uid", v));
        }
        if !params.is_empty() {
            req = req.query(&params);
        }
        let resp = req.send().await?;
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

    // ── Habit scheduled spans (#303 / #503) ──

    pub async fn list_habit_scheduled_spans(
        &self,
        id: &str,
    ) -> Result<Vec<HabitScheduledSpanRow>, ClientError> {
        let resp = self
            .request(
                reqwest::Method::GET,
                &format!("/api/habits/{id}/scheduled-spans"),
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

    pub async fn list_all_habit_scheduled_spans(
        &self,
    ) -> Result<Vec<HabitScheduledSpanRow>, ClientError> {
        let resp = self
            .request(reqwest::Method::GET, "/api/habits/scheduled-spans")
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

    pub async fn create_habit_scheduled_span(
        &self,
        id: &str,
        body: &CreateHabitScheduledSpan,
    ) -> Result<HabitScheduledSpanRow, ClientError> {
        let resp = self
            .request(
                reqwest::Method::POST,
                &format!("/api/habits/{id}/scheduled-spans"),
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

    pub async fn delete_habit_scheduled_span(
        &self,
        id: &str,
        span_id: &str,
    ) -> Result<(), ClientError> {
        let resp = self
            .request(
                reqwest::Method::DELETE,
                &format!("/api/habits/{id}/scheduled-spans/{span_id}"),
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

    pub async fn preview_schedule(
        &self,
        body: &SchedulePreviewRequest,
    ) -> Result<serde_json::Value, ClientError> {
        let resp = self
            .request(reqwest::Method::POST, "/api/schedule/preview")
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

    pub async fn replace_schedule(
        &self,
        body: &SaveScheduleRequest,
    ) -> Result<ScheduleRow, ClientError> {
        let resp = self
            .request(reqwest::Method::POST, "/api/schedule/replace")
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

    pub async fn delete_all_gcal_events(&self) -> Result<DeleteAllGcalResponse, ClientError> {
        let resp = self
            .request(reqwest::Method::POST, "/api/sync/delete-all")
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

    // ── Skills (#WI-6) ──

    pub async fn list_skills(&self) -> Result<Vec<SkillRow>, ClientError> {
        let resp = self
            .request(reqwest::Method::GET, "/api/skills")
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

    pub async fn get_skill(&self, slug: &str) -> Result<SkillRow, ClientError> {
        let resp = self
            .request(reqwest::Method::GET, &format!("/api/skills/{slug}"))
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

    pub async fn create_skill(&self, body: &CreateSkill) -> Result<SkillRow, ClientError> {
        let resp = self
            .request(reqwest::Method::POST, "/api/skills")
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

    pub async fn update_skill(
        &self,
        slug: &str,
        body: &UpdateSkill,
    ) -> Result<SkillRow, ClientError> {
        let resp = self
            .request(reqwest::Method::PATCH, &format!("/api/skills/{slug}"))
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

    pub async fn delete_skill(&self, slug: &str) -> Result<(), ClientError> {
        let resp = self
            .request(reqwest::Method::DELETE, &format!("/api/skills/{slug}"))
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

#[derive(Debug, Serialize, Deserialize)]
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

#[derive(Debug, Default, Serialize, Deserialize)]
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
    pub no_overdue: Option<bool>,
    pub habit_id: Option<String>,
    pub ical_uid: Option<String>,
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

#[derive(Debug, Serialize, Deserialize)]
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

#[derive(Debug, Default, Serialize, Deserialize)]
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

#[derive(Debug, Serialize)]
pub struct CreateHabitScheduledSpan {
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

#[derive(Debug, Serialize, Deserialize)]
pub struct SchedulePreviewRequest {
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

#[derive(Debug, Serialize, Deserialize)]
pub struct SaveScheduleRequest {
    pub entries: Vec<ScheduleEntry>,
    #[serde(default)]
    pub mark_scheduled_task_ids: Vec<String>,
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

#[derive(Debug, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeleteAllGcalResponse {
    pub deleted: usize,
    pub failed: Vec<DeleteAllGcalFailure>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeleteAllGcalFailure {
    pub task_id: String,
    pub error: String,
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

// ── Skill types (#WI-6) ──

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillRow {
    pub slug: String,
    pub name: String,
    pub description: String,
    pub body: String,
    #[serde(default)]
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub built_in: Option<bool>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct UpdateSkill {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body: Option<String>,
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
