use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use thiserror::Error;
use tokio_tungstenite::tungstenite::Message;

#[derive(Debug, Error)]
pub enum MoonshineError {
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
pub struct MoonshineConfig {
    pub url: String,
    pub language: String,
    pub model_arch: Option<u32>,
}

impl Default for MoonshineConfig {
    fn default() -> Self {
        Self {
            url: "ws://127.0.0.1:10096".to_string(),
            language: "ja".to_string(),
            model_arch: None,
        }
    }
}

#[derive(Debug, Deserialize)]
struct ResultMessage {
    #[serde(rename = "type")]
    msg_type: String,
    text: Option<String>,
    message: Option<String>,
}

pub struct MoonshineClient {
    config: MoonshineConfig,
}

impl MoonshineClient {
    pub fn new(config: MoonshineConfig) -> Self {
        Self { config }
    }

    pub async fn transcribe(&self, audio: &[f32]) -> Result<String, MoonshineError> {
        let (mut ws_stream, _) = tokio_tungstenite::connect_async(&self.config.url)
            .await
            .map_err(|e| {
                MoonshineError::Connection(format!(
                    "failed to connect to {}: {e}",
                    self.config.url
                ))
            })?;

        let mut start = serde_json::json!({
            "type": "start",
            "language": self.config.language,
        });
        if let Some(arch) = self.config.model_arch {
            start["model_arch"] = serde_json::json!(arch);
        }
        let start_json = serde_json::to_string(&start)
            .map_err(|e| MoonshineError::Connection(format!("json error: {e}")))?;
        ws_stream.send(Message::Text(start_json.into())).await?;

        let audio_bytes: Vec<u8> = audio.iter().flat_map(|s| s.to_le_bytes()).collect();
        ws_stream.send(Message::Binary(audio_bytes.into())).await?;

        let end_msg = r#"{"type":"end"}"#;
        ws_stream.send(Message::Text(end_msg.into())).await?;

        let mut final_text = String::new();
        while let Some(msg) = ws_stream.next().await {
            match msg {
                Ok(Message::Text(text)) => {
                    let parsed: ResultMessage = serde_json::from_str(&text)
                        .map_err(|e| MoonshineError::Connection(format!("json parse error: {e}")))?;

                    match parsed.msg_type.as_str() {
                        "result" => {
                            final_text = parsed.text.unwrap_or_default();
                            let _ = ws_stream.close(None).await;
                            break;
                        }
                        "error" => {
                            return Err(MoonshineError::Server(
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
                Err(e) => return Err(MoonshineError::WebSocket(e)),
            }
        }

        if final_text.is_empty() && !audio.is_empty() {
            return Err(MoonshineError::NoResult);
        }

        Ok(final_text)
    }

    pub async fn check_available(&self) -> bool {
        tokio_tungstenite::connect_async(&self.config.url)
            .await
            .is_ok()
    }
}
