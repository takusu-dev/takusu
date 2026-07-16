use std::time::Duration;

use async_trait::async_trait;
use reqwest::Client;
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::json;
use takusu_storage::{
    CreateHabit, CreateHabitPause, CreateSkill, CreateTask, GoogleCalEventRow,
    GoogleCalSettingsRow, HabitPauseRow, HabitRow, HabitStepInput, HabitStepRow,
    SaveScheduleRequest, ScheduleRow, SettingsRow, SkillRow, Storage, StorageError, TaskQuery,
    TaskRow, TokenCreateResponse, TokenRow, UpdateGoogleCalSettings, UpdateHabit, UpdateSettings,
    UpdateSkill, UpdateTask, storage::StorageResult,
};

const RETRY_STATUSES: &[u16] = &[429, 500, 502, 503, 504];
const RETRY_DELAYS_MS: &[u64] = &[100, 200, 400];

pub struct WorkersStorage {
    http: Client,
    base_url: String,
    token: String,
}

impl WorkersStorage {
    pub fn new_with(base_url: String, token: String) -> Self {
        Self {
            http: Client::new(),
            base_url: base_url.trim_end_matches('/').to_string(),
            token,
        }
    }

    /// Like [`new_with`](Self::new_with) but with a caller-supplied HTTP
    /// client.  On Android the default `Client::new()` pulls in
    /// `rustls-platform-verifier`, which panics unless initialised with a JNI
    /// context.  Callers that cannot provide that context should instead build
    /// a client with bundled root certificates (e.g. `webpki-root-certs`) and
    /// pass it here.
    pub fn new_with_client(client: Client, base_url: String, token: String) -> Self {
        Self {
            http: client,
            base_url: base_url.trim_end_matches('/').to_string(),
            token,
        }
    }

    async fn request<T: DeserializeOwned>(
        &self,
        method: reqwest::Method,
        path: &str,
    ) -> StorageResult<T> {
        let url = format!("{}{}", self.base_url, path);
        let resp = self
            .send_with_retry(|| {
                self.http
                    .request(method.clone(), &url)
                    .bearer_auth(&self.token)
                    .build()
            })
            .await?;
        map_response(resp).await
    }

    async fn request_body<T: DeserializeOwned, B: Serialize>(
        &self,
        method: reqwest::Method,
        path: &str,
        body: &B,
    ) -> StorageResult<T> {
        let url = format!("{}{}", self.base_url, path);
        let resp = self
            .send_with_retry(|| {
                self.http
                    .request(method.clone(), &url)
                    .bearer_auth(&self.token)
                    .json(body)
                    .build()
            })
            .await?;
        map_response(resp).await
    }

    async fn request_body_empty<B: Serialize>(
        &self,
        method: reqwest::Method,
        path: &str,
        body: &B,
    ) -> StorageResult<()> {
        let url = format!("{}{}", self.base_url, path);
        let resp = self
            .send_with_retry(|| {
                self.http
                    .request(method.clone(), &url)
                    .bearer_auth(&self.token)
                    .json(body)
                    .build()
            })
            .await?;
        map_empty(resp).await
    }

    async fn request_no_body(&self, method: reqwest::Method, path: &str) -> StorageResult<()> {
        let url = format!("{}{}", self.base_url, path);
        let resp = self
            .send_with_retry(|| {
                self.http
                    .request(method.clone(), &url)
                    .bearer_auth(&self.token)
                    .build()
            })
            .await?;
        map_empty(resp).await
    }

    async fn send_with_retry<F>(&self, build: F) -> StorageResult<reqwest::Response>
    where
        F: Fn() -> reqwest::Result<reqwest::Request>,
    {
        let mut attempt = 0;
        loop {
            let req = build().map_err(|e| StorageError::Internal(format!("build request: {e}")))?;
            let result = self.http.execute(req).await;
            match result {
                Ok(resp) if !RETRY_STATUSES.contains(&resp.status().as_u16()) => return Ok(resp),
                Ok(resp) if attempt < RETRY_DELAYS_MS.len() => {
                    let status = resp.status().as_u16();
                    let delay = RETRY_DELAYS_MS[attempt];
                    tracing::warn!(
                        "worker returned retryable status {status} (attempt {}), sleeping {delay}ms",
                        attempt + 1
                    );
                    tokio::time::sleep(Duration::from_millis(delay)).await;
                    attempt += 1;
                }
                Ok(resp) => return Ok(resp),
                Err(e) if attempt < RETRY_DELAYS_MS.len() => {
                    let delay = RETRY_DELAYS_MS[attempt];
                    tracing::warn!(
                        "worker request failed (attempt {}): {e}, sleeping {delay}ms",
                        attempt + 1
                    );
                    tokio::time::sleep(Duration::from_millis(delay)).await;
                    attempt += 1;
                }
                Err(e) => {
                    return Err(StorageError::Internal(format!("worker http: {e}")));
                }
            }
        }
    }
}

async fn map_response<T: DeserializeOwned>(resp: reqwest::Response) -> StorageResult<T> {
    let status = resp.status().as_u16();
    if status >= 400 {
        let body = resp.text().await.unwrap_or_default();
        return Err(map_status(status, body));
    }
    resp.json::<T>()
        .await
        .map_err(|e| StorageError::Internal(format!("decode: {e}")))
}

async fn map_empty(resp: reqwest::Response) -> StorageResult<()> {
    let status = resp.status().as_u16();
    if status >= 400 {
        let body = resp.text().await.unwrap_or_default();
        return Err(map_status(status, body));
    }
    Ok(())
}

fn map_status(status: u16, body: String) -> StorageError {
    match status {
        401 => StorageError::Unauthorized,
        404 => StorageError::NotFound(body),
        400 => StorageError::BadRequest(body),
        409 => StorageError::Conflict(body),
        _ => StorageError::Internal(format!("status {status}: {body}")),
    }
}

#[async_trait]
impl Storage for WorkersStorage {
    async fn verify_token(&self, token: &str) -> StorageResult<bool> {
        let url = format!("{}/api/auth/verify", self.base_url);
        let resp = self
            .send_with_retry(|| self.http.get(&url).bearer_auth(token).build())
            .await?;
        match resp.status().as_u16() {
            200 => Ok(true),
            401 => Ok(false),
            other => {
                let body = resp.text().await.unwrap_or_default();
                Err(StorageError::Internal(format!(
                    "verify status {other}: {body}"
                )))
            }
        }
    }

    async fn list_tasks(&self, _query: &TaskQuery) -> StorageResult<Vec<TaskRow>> {
        let mut path = String::from("/api/tasks");
        let q = _query;
        let mut parts: Vec<String> = Vec::new();
        if let Some(s) = &q.status {
            parts.push(format!("status={}", url_encode(s)));
        }
        if let Some(f) = &q.from {
            parts.push(format!("from={}", url_encode(f)));
        }
        if let Some(u) = &q.until {
            parts.push(format!("until={}", url_encode(u)));
        }
        if let Some(h) = &q.habit_id {
            parts.push(format!("habit_id={}", url_encode(h)));
        }
        if !parts.is_empty() {
            path.push('?');
            path.push_str(&parts.join("&"));
        }
        self.request(reqwest::Method::GET, &path).await
    }

    async fn get_task(&self, id: &str) -> StorageResult<TaskRow> {
        let full = self.resolve_task_id(id).await?;
        self.request(
            reqwest::Method::GET,
            &format!("/api/tasks/{}", url_encode(&full)),
        )
        .await
    }

    async fn create_task(&self, body: &CreateTask) -> StorageResult<TaskRow> {
        self.request_body(reqwest::Method::POST, "/api/tasks", body)
            .await
    }

    async fn update_task(&self, id: &str, body: &UpdateTask) -> StorageResult<TaskRow> {
        let full = self.resolve_task_id(id).await?;
        self.request_body(
            reqwest::Method::PATCH,
            &format!("/api/tasks/{}", url_encode(&full)),
            body,
        )
        .await
    }

    async fn replace_task(&self, id: &str, body: &CreateTask) -> StorageResult<TaskRow> {
        let full = self.resolve_task_id(id).await?;
        self.request_body(
            reqwest::Method::PUT,
            &format!("/api/tasks/{}", url_encode(&full)),
            body,
        )
        .await
    }

    async fn delete_task(&self, id: &str) -> StorageResult<()> {
        let full = self.resolve_task_id(id).await?;
        self.request_no_body(
            reqwest::Method::DELETE,
            &format!("/api/tasks/{}", url_encode(&full)),
        )
        .await
    }

    async fn list_habits(&self) -> StorageResult<Vec<HabitRow>> {
        self.request(reqwest::Method::GET, "/api/habits").await
    }

    async fn get_habit(&self, id: &str) -> StorageResult<HabitRow> {
        self.request(
            reqwest::Method::GET,
            &format!("/api/habits/{}", url_encode(id)),
        )
        .await
    }

    async fn create_habit(&self, body: &CreateHabit) -> StorageResult<HabitRow> {
        self.request_body(reqwest::Method::POST, "/api/habits", body)
            .await
    }

    async fn update_habit(&self, id: &str, body: &UpdateHabit) -> StorageResult<HabitRow> {
        self.request_body(
            reqwest::Method::PATCH,
            &format!("/api/habits/{}", url_encode(id)),
            body,
        )
        .await
    }

    async fn replace_habit(&self, id: &str, body: &CreateHabit) -> StorageResult<HabitRow> {
        self.request_body(
            reqwest::Method::PUT,
            &format!("/api/habits/{}", url_encode(id)),
            body,
        )
        .await
    }

    async fn delete_habit(&self, id: &str) -> StorageResult<()> {
        self.request_no_body(
            reqwest::Method::DELETE,
            &format!("/api/habits/{}", url_encode(id)),
        )
        .await
    }

    async fn list_habit_pauses(&self, habit_id: &str) -> StorageResult<Vec<HabitPauseRow>> {
        self.request(
            reqwest::Method::GET,
            &format!("/api/habits/{}/pauses", url_encode(habit_id)),
        )
        .await
    }

    async fn list_all_habit_pauses(&self) -> StorageResult<Vec<HabitPauseRow>> {
        self.request(reqwest::Method::GET, "/api/habits/pauses")
            .await
    }

    async fn create_habit_pause(
        &self,
        habit_id: &str,
        body: &CreateHabitPause,
    ) -> StorageResult<HabitPauseRow> {
        self.request_body(
            reqwest::Method::POST,
            &format!("/api/habits/{}/pauses", url_encode(habit_id)),
            body,
        )
        .await
    }

    async fn delete_habit_pause(&self, habit_id: &str, pause_id: &str) -> StorageResult<()> {
        self.request_no_body(
            reqwest::Method::DELETE,
            &format!(
                "/api/habits/{}/pauses/{}",
                url_encode(habit_id),
                url_encode(pause_id)
            ),
        )
        .await
    }

    async fn list_habit_steps(&self, habit_id: &str) -> StorageResult<Vec<HabitStepRow>> {
        self.request(
            reqwest::Method::GET,
            &format!("/api/habits/{}/steps", url_encode(habit_id)),
        )
        .await
    }

    async fn list_all_habit_steps(&self) -> StorageResult<Vec<HabitStepRow>> {
        self.request(reqwest::Method::GET, "/api/habits/steps")
            .await
    }

    async fn replace_habit_steps(
        &self,
        habit_id: &str,
        steps: &[HabitStepInput],
    ) -> StorageResult<Vec<HabitStepRow>> {
        self.request_body(
            reqwest::Method::PUT,
            &format!("/api/habits/{}/steps", url_encode(habit_id)),
            &steps,
        )
        .await
    }

    async fn get_schedule(&self) -> StorageResult<Option<ScheduleRow>> {
        let url = format!("{}/api/schedule", self.base_url);
        let resp = self
            .send_with_retry(|| self.http.get(&url).bearer_auth(&self.token).build())
            .await?;
        match resp.status().as_u16() {
            200 => {
                let row: ScheduleRow = resp
                    .json()
                    .await
                    .map_err(|e| StorageError::Internal(format!("decode: {e}")))?;
                Ok(Some(row))
            }
            404 => Ok(None),
            other => {
                let body = resp.text().await.unwrap_or_default();
                Err(StorageError::Internal(format!(
                    "schedule status {other}: {body}"
                )))
            }
        }
    }

    async fn save_schedule(&self, req: &SaveScheduleRequest) -> StorageResult<ScheduleRow> {
        self.request_body(reqwest::Method::POST, "/api/schedule/save", req)
            .await
    }

    async fn clear_schedule(&self) -> StorageResult<()> {
        self.request_no_body(reqwest::Method::DELETE, "/api/schedule")
            .await
    }

    async fn create_token(&self, label: Option<&str>) -> StorageResult<TokenCreateResponse> {
        self.request_body(
            reqwest::Method::POST,
            "/api/tokens",
            &json!({ "label": label }),
        )
        .await
    }

    async fn list_tokens(&self) -> StorageResult<Vec<TokenRow>> {
        self.request(reqwest::Method::GET, "/api/tokens").await
    }

    async fn revoke_token(&self, id: i64) -> StorageResult<()> {
        self.request_no_body(reqwest::Method::DELETE, &format!("/api/tokens/{id}"))
            .await
    }

    async fn get_settings(&self) -> StorageResult<SettingsRow> {
        self.request(reqwest::Method::GET, "/api/settings").await
    }

    async fn update_settings(&self, body: &UpdateSettings) -> StorageResult<SettingsRow> {
        self.request_body(reqwest::Method::PUT, "/api/settings", body)
            .await
    }

    async fn get_gcal_settings(&self) -> StorageResult<GoogleCalSettingsRow> {
        self.request(reqwest::Method::GET, "/api/sync/settings")
            .await
    }

    async fn update_gcal_settings(
        &self,
        body: &UpdateGoogleCalSettings,
    ) -> StorageResult<GoogleCalSettingsRow> {
        self.request_body(reqwest::Method::PUT, "/api/sync/settings", body)
            .await
    }

    async fn list_gcal_mappings(&self) -> StorageResult<Vec<GoogleCalEventRow>> {
        self.request(reqwest::Method::GET, "/api/sync/mappings")
            .await
    }

    async fn upsert_gcal_mappings(&self, mappings: &[(String, String)]) -> StorageResult<()> {
        let body = json!({
            "mappings": mappings.iter().map(|(t, e)| json!({
                "task_id": t,
                "google_event_id": e
            })).collect::<Vec<_>>()
        });
        self.request_body_empty(reqwest::Method::POST, "/api/sync/mappings", &body)
            .await
    }

    async fn delete_gcal_mappings(&self, task_ids: &[String]) -> StorageResult<()> {
        self.request_body_empty(
            reqwest::Method::DELETE,
            "/api/sync/mappings",
            &json!({ "task_ids": task_ids }),
        )
        .await
    }

    async fn clear_gcal_mappings(&self) -> StorageResult<()> {
        let url = format!("{}/api/sync/mappings?all=1", self.base_url);
        let resp = self
            .send_with_retry(|| self.http.delete(&url).bearer_auth(&self.token).build())
            .await?;
        map_empty(resp).await
    }

    async fn list_skills(&self) -> StorageResult<Vec<SkillRow>> {
        self.request(reqwest::Method::GET, "/api/skills").await
    }

    async fn get_skill(&self, slug: &str) -> StorageResult<SkillRow> {
        self.request(
            reqwest::Method::GET,
            &format!("/api/skills/{}", url_encode(slug)),
        )
        .await
    }

    async fn create_skill(&self, body: &CreateSkill) -> StorageResult<SkillRow> {
        self.request_body(reqwest::Method::POST, "/api/skills", body)
            .await
    }

    async fn update_skill(&self, slug: &str, body: &UpdateSkill) -> StorageResult<SkillRow> {
        self.request_body(
            reqwest::Method::PATCH,
            &format!("/api/skills/{}", url_encode(slug)),
            body,
        )
        .await
    }

    async fn delete_skill(&self, slug: &str) -> StorageResult<()> {
        self.request_no_body(
            reqwest::Method::DELETE,
            &format!("/api/skills/{}", url_encode(slug)),
        )
        .await
    }

    async fn health_check(&self) -> StorageResult<String> {
        let url = format!("{}/health", self.base_url);
        // Per-request timeout so an unreachable worker fails fast instead of
        // hanging indefinitely (the shared client has no default timeout).
        let resp = self
            .http
            .get(&url)
            .timeout(std::time::Duration::from_secs(5))
            .send()
            .await
            .map_err(|e| StorageError::Internal(format!("worker health check failed: {e}")))?;
        let status = resp.status();
        if !status.is_success() {
            return Err(StorageError::Internal(format!(
                "worker health check returned {status}"
            )));
        }
        let body = resp
            .text()
            .await
            .map_err(|e| StorageError::Internal(format!("worker health check body read: {e}")))?;
        Ok(format!("worker ok: {}", body.trim()))
    }
}

impl WorkersStorage {
    async fn resolve_task_id(&self, id: &str) -> StorageResult<String> {
        // `h{habit_display_id}#{task_display_id}` → habit task lookup (#380).
        if let Some(rest) = id.strip_prefix(['h', 'H'])
            && let Some((hdisp, tdisp)) = rest.split_once('#')
            && let (Ok(hnum), Ok(tnum)) = (hdisp.parse::<i64>(), tdisp.parse::<i64>())
        {
            let tasks: Vec<TaskRow> = self
                .request::<Vec<TaskRow>>(reqwest::Method::GET, "/api/tasks")
                .await?;
            let habits: Vec<HabitRow> = self
                .request::<Vec<HabitRow>>(reqwest::Method::GET, "/api/habits")
                .await?;
            let habit_id = habits
                .iter()
                .find(|h| h.display_id == hnum)
                .map(|h| h.id.as_str());
            if let Some(hid) = habit_id
                && let Some(t) = tasks
                    .iter()
                    .find(|t| t.habit_id.as_deref() == Some(hid) && t.display_id == tnum)
            {
                return Ok(t.id.clone());
            }
            return Err(StorageError::NotFound(format!("task {id} not found")));
        }
        // Numeric input → resolve via display_id for non-habit tasks only (#380).
        if let Ok(num) = id.parse::<i64>() {
            let tasks: Vec<TaskRow> = self
                .request::<Vec<TaskRow>>(reqwest::Method::GET, "/api/tasks")
                .await?;
            if let Some(t) = tasks
                .iter()
                .find(|t| t.display_id == num && t.habit_id.is_none())
            {
                return Ok(t.id.clone());
            }
            return Err(StorageError::NotFound(format!("task {id} not found")));
        }
        if id.contains('-') {
            return Ok(id.to_string());
        }
        // UUID prefix — fetch all tasks and filter client-side (matches
        // SqliteStorage's `LIKE prefix%` behaviour).
        let tasks: Vec<TaskRow> = self
            .request::<Vec<TaskRow>>(reqwest::Method::GET, "/api/tasks")
            .await?;
        let mut matches: Vec<String> = tasks
            .iter()
            .filter(|t| t.id.starts_with(id))
            .map(|t| t.id.clone())
            .collect();
        match matches.len() {
            0 => Err(StorageError::NotFound(format!("task {id} not found"))),
            1 => Ok(matches.remove(0)),
            _ => Err(StorageError::BadRequest(format!(
                "ambiguous task id prefix: {id}"
            ))),
        }
    }
}

fn url_encode(s: &str) -> String {
    s.bytes()
        .flat_map(|b| match b {
            b'0'..=b'9' | b'a'..=b'z' | b'A'..=b'Z' | b'-' | b'_' | b'.' | b'~' => {
                vec![b as char]
            }
            _ => format!("%{b:02X}").chars().collect(),
        })
        .collect()
}
