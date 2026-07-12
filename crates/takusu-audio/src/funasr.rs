//! FunASR WebSocket client for streaming/offline transcription via SenseVoice.

use std::collections::HashMap;

use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio_tungstenite::tungstenite::Message;

use crate::stt::{SpeechToText, SttError};

const CONNECT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);
const RESULT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(120);

#[derive(Debug, Error)]
pub enum FunASRError {
    #[error("websocket error: {0}")]
    WebSocket(#[from] tokio_tungstenite::tungstenite::Error),
    #[error("connection error: {0}")]
    Connection(String),
    #[error("server error: {0}")]
    Server(String),
    #[error("no result received")]
    NoResult,
}

#[derive(Debug, Clone)]
pub struct FunASRConfig {
    pub url: String,
    pub language: String,
    pub hotwords: Vec<String>,
    pub mode: FunASRMode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FunASRMode {
    Offline,
    TwoPass,
}

impl std::fmt::Display for FunASRMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FunASRMode::Offline => write!(f, "offline"),
            FunASRMode::TwoPass => write!(f, "2pass"),
        }
    }
}

impl Default for FunASRConfig {
    fn default() -> Self {
        Self {
            url: "ws://127.0.0.1:10095".to_string(),
            language: "ja".to_string(),
            hotwords: Vec::new(),
            mode: FunASRMode::Offline,
        }
    }
}

#[derive(Debug, Serialize)]
struct StartMessage {
    #[serde(rename = "type")]
    msg_type: String,
    language: String,
    hotwords: String,
    mode: String,
}

#[derive(Debug, Deserialize)]
struct ResultMessage {
    #[serde(rename = "type")]
    msg_type: String,
    text: Option<String>,
    message: Option<String>,
}

pub struct FunASRClient {
    config: FunASRConfig,
}

impl FunASRClient {
    pub fn new(config: FunASRConfig) -> Self {
        Self { config }
    }

    pub async fn transcribe(&self, audio: &[f32]) -> Result<String, FunASRError> {
        let (mut ws_stream, _) = tokio::time::timeout(
            CONNECT_TIMEOUT,
            tokio_tungstenite::connect_async(&self.config.url),
        )
        .await
        .map_err(|_| {
            FunASRError::Connection(format!("timed out connecting to {}", self.config.url))
        })?
        .map_err(|e| {
            FunASRError::Connection(format!("failed to connect to {}: {e}", self.config.url))
        })?;

        let hotwords_str = self.config.hotwords.join(" ");
        let start_msg = StartMessage {
            msg_type: "start".to_string(),
            language: self.config.language.clone(),
            hotwords: hotwords_str,
            mode: self.config.mode.to_string(),
        };
        let start_json = serde_json::to_string(&start_msg)
            .map_err(|e| FunASRError::Connection(format!("json error: {e}")))?;
        ws_stream.send(Message::Text(start_json.into())).await?;

        let audio_bytes: Vec<u8> = audio.iter().flat_map(|s| s.to_le_bytes()).collect();
        ws_stream.send(Message::Binary(audio_bytes.into())).await?;

        let end_msg = r#"{"type":"end"}"#;
        ws_stream.send(Message::Text(end_msg.into())).await?;

        let mut final_text = String::new();
        while let Some(msg) = tokio::time::timeout(RESULT_TIMEOUT, ws_stream.next())
            .await
            .map_err(|_| FunASRError::Connection("timed out waiting for result".to_string()))?
        {
            match msg {
                Ok(Message::Text(text)) => {
                    let parsed: ResultMessage = serde_json::from_str(&text)
                        .map_err(|e| FunASRError::Connection(format!("json parse error: {e}")))?;

                    match parsed.msg_type.as_str() {
                        "result" => {
                            final_text = parsed.text.unwrap_or_default();
                            let _ = ws_stream.close(None).await;
                            break;
                        }
                        "partial" => {
                            if let Some(t) = parsed.text {
                                eprintln!("[partial] {t}");
                            }
                        }
                        "error" => {
                            return Err(FunASRError::Server(
                                parsed
                                    .message
                                    .unwrap_or_else(|| "unknown error".to_string()),
                            ));
                        }
                        _ => {}
                    }
                }
                Ok(Message::Close(_)) => break,
                Ok(_) => continue,
                Err(e) => return Err(FunASRError::WebSocket(e)),
            }
        }

        if final_text.is_empty() && !audio.is_empty() {
            return Err(FunASRError::NoResult);
        }

        Ok(final_text)
    }

    pub async fn check_available(&self) -> bool {
        matches!(
            tokio::time::timeout(
                CONNECT_TIMEOUT,
                tokio_tungstenite::connect_async(&self.config.url),
            )
            .await,
            Ok(Ok(_))
        )
    }
}

impl From<FunASRError> for SttError {
    fn from(e: FunASRError) -> Self {
        match e {
            FunASRError::WebSocket(e) => Self::Connection(e.to_string()),
            FunASRError::Connection(msg) => Self::Connection(msg),
            FunASRError::Server(msg) => Self::Server(msg),
            FunASRError::NoResult => Self::NoResult,
        }
    }
}

#[async_trait::async_trait]
impl SpeechToText for FunASRClient {
    async fn transcribe(&self, audio: &[f32]) -> Result<String, SttError> {
        FunASRClient::transcribe(self, audio)
            .await
            .map_err(Into::into)
    }
}

pub fn default_hotwords() -> HashMap<String, Vec<String>> {
    let mut m = HashMap::new();
    m.insert("ja".to_string(), vec![]);
    m
}
