//! # google-cal — Google Calendar API client
//!
//! OAuth2認証とGoogle Calendarイベントの同期を提供する。
//! イベント同期は差分ベース: 既存マッピングと比較して
//! 新規作成・更新・削除を行う。

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

const GOOGLE_AUTH_URL: &str = "https://accounts.google.com/o/oauth2/v2/auth";
const GOOGLE_TOKEN_URL: &str = "https://oauth2.googleapis.com/token";
const GOOGLE_CALENDAR_BASE: &str = "https://www.googleapis.com/calendar/v3";

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("OAuth2 error: {0}")]
    OAuth2(String),
    #[error("API error: {0}")]
    Api(String),
}

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: String,
    expires_in: i64,
    refresh_token: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct OAuthTokens {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_in: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncEntry {
    pub task_id: String,
    pub summary: String,
    pub description: Option<String>,
    pub start: String,
    pub end: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SyncResult {
    pub mappings: Vec<(String, String)>,
    pub deleted: Vec<String>,
}

pub struct Client {
    http: reqwest::Client,
    client_id: String,
    client_secret: String,
    refresh_token: String,
    calendar_id: String,
}

pub fn oauth_url(client_id: &str, redirect_uri: &str) -> String {
    format!(
        "{}?client_id={}&redirect_uri={}&response_type=code&scope=https://www.googleapis.com/auth/calendar.events&access_type=offline&prompt=consent",
        GOOGLE_AUTH_URL,
        urlencoding::encode(client_id),
        urlencoding::encode(redirect_uri),
    )
}

pub async fn exchange_code(
    client_id: &str,
    client_secret: &str,
    code: &str,
    redirect_uri: &str,
) -> Result<OAuthTokens> {
    let http = reqwest::Client::new();
    let resp = http
        .post(GOOGLE_TOKEN_URL)
        .form(&[
            ("code", code),
            ("client_id", client_id),
            ("client_secret", client_secret),
            ("redirect_uri", redirect_uri),
            ("grant_type", "authorization_code"),
        ])
        .send()
        .await?;

    if !resp.status().is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err(Error::OAuth2(body));
    }

    let token: TokenResponse = resp.json().await?;
    Ok(OAuthTokens {
        access_token: token.access_token,
        refresh_token: token.refresh_token.unwrap_or_default(),
        expires_in: token.expires_in,
    })
}

mod urlencoding {
    pub fn encode(s: &str) -> String {
        s.bytes()
            .flat_map(|b| match b {
                b'0'..=b'9' | b'a'..=b'z' | b'A'..=b'Z' | b'-' | b'_' | b'.' | b'~' => {
                    vec![b as char]
                }
                _ => format!("%{b:02X}").chars().collect(),
            })
            .collect()
    }
}

impl Client {
    pub fn new(
        client_id: String,
        client_secret: String,
        refresh_token: String,
        calendar_id: String,
    ) -> Self {
        Self {
            http: reqwest::Client::new(),
            client_id,
            client_secret,
            refresh_token,
            calendar_id,
        }
    }

    async fn refresh_access_token(&self) -> Result<String> {
        let resp = self
            .http
            .post(GOOGLE_TOKEN_URL)
            .form(&[
                ("client_id", self.client_id.as_str()),
                ("client_secret", self.client_secret.as_str()),
                ("refresh_token", self.refresh_token.as_str()),
                ("grant_type", "refresh_token"),
            ])
            .send()
            .await?;

        if !resp.status().is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(Error::OAuth2(body));
        }

        let token: TokenResponse = resp.json().await?;
        Ok(token.access_token)
    }

    fn events_url(&self, suffix: &str) -> String {
        format!(
            "{}/calendars/{}/{}",
            GOOGLE_CALENDAR_BASE,
            urlencoding::encode(&self.calendar_id),
            suffix,
        )
    }

    fn event_url(&self, event_id: &str) -> String {
        format!(
            "{}/calendars/{}/events/{}",
            GOOGLE_CALENDAR_BASE,
            urlencoding::encode(&self.calendar_id),
            urlencoding::encode(event_id),
        )
    }

    fn event_body(&self, entry: &SyncEntry) -> serde_json::Value {
        let mut body = serde_json::json!({
            "summary": entry.summary,
            "start": { "dateTime": entry.start },
            "end": { "dateTime": entry.end },
            "extendedProperties": {
                "private": {
                    "takusuTaskId": entry.task_id,
                }
            },
        });
        if let Some(desc) = &entry.description {
            body["description"] = serde_json::Value::String(desc.clone());
        }
        body
    }

    async fn create_event(&self, token: &str, entry: &SyncEntry) -> Result<String> {
        let url = self.events_url("events");
        let body = self.event_body(entry);
        let resp = self
            .http
            .post(&url)
            .bearer_auth(token)
            .json(&body)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(Error::Api(format!("create event ({status}): {text}")));
        }

        let result: serde_json::Value = resp.json().await?;
        result["id"]
            .as_str()
            .map(|s| s.to_string())
            .ok_or_else(|| Error::Api("missing event id in response".into()))
    }

    async fn update_event(&self, token: &str, event_id: &str, entry: &SyncEntry) -> Result<String> {
        let url = self.event_url(event_id);
        let body = self.event_body(entry);
        let resp = self
            .http
            .put(&url)
            .bearer_auth(token)
            .json(&body)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(Error::Api(format!("update event ({status}): {text}")));
        }

        let result: serde_json::Value = resp.json().await?;
        result["id"]
            .as_str()
            .map(|s| s.to_string())
            .ok_or_else(|| Error::Api("missing event id in response".into()))
    }

    async fn delete_event(&self, token: &str, event_id: &str) -> Result<()> {
        let url = self.event_url(event_id);
        let resp = self.http.delete(&url).bearer_auth(token).send().await?;

        if !resp.status().is_success() && resp.status().as_u16() != 410 {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(Error::Api(format!("delete event ({status}): {text}")));
        }

        Ok(())
    }

    pub async fn sync(
        &self,
        entries: &[SyncEntry],
        existing: &HashMap<String, String>,
    ) -> Result<SyncResult> {
        let token = self.refresh_access_token().await?;
        let mut mappings = Vec::new();
        let mut deleted = Vec::new();

        let entry_ids: Vec<&str> = entries.iter().map(|e| e.task_id.as_str()).collect();

        for (task_id, event_id) in existing {
            if !entry_ids.contains(&task_id.as_str()) {
                match self.delete_event(&token, event_id).await {
                    Ok(()) => deleted.push(event_id.clone()),
                    Err(e) => tracing::warn!("failed to delete event {event_id}: {e}"),
                }
            }
        }

        for entry in entries {
            if let Some(event_id) = existing.get(&entry.task_id) {
                match self.update_event(&token, event_id, entry).await {
                    Ok(new_id) => mappings.push((entry.task_id.clone(), new_id)),
                    Err(e) => {
                        tracing::warn!(
                            "failed to update event for task {}: {e}, trying create",
                            entry.task_id
                        );
                        match self.create_event(&token, entry).await {
                            Ok(id) => mappings.push((entry.task_id.clone(), id)),
                            Err(e2) => {
                                tracing::error!(
                                    "failed to create event for task {}: {e2}",
                                    entry.task_id
                                );
                            }
                        }
                    }
                }
            } else {
                match self.create_event(&token, entry).await {
                    Ok(id) => mappings.push((entry.task_id.clone(), id)),
                    Err(e) => {
                        tracing::error!("failed to create event for task {}: {e}", entry.task_id);
                    }
                }
            }
        }

        Ok(SyncResult { mappings, deleted })
    }

    pub async fn delete_all(&self, event_ids: &[(String, String)]) -> Result<Vec<String>> {
        let token = self.refresh_access_token().await?;
        let mut deleted = Vec::new();

        for (task_id, event_id) in event_ids {
            match self.delete_event(&token, event_id).await {
                Ok(()) => {
                    deleted.push(task_id.clone());
                }
                Err(e) => {
                    tracing::warn!("failed to delete event {event_id}: {e}");
                }
            }
        }

        Ok(deleted)
    }
}
