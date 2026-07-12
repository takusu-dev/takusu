//! Text-to-speech client for the Irodori-TTS inference server.

use std::path::{Path, PathBuf};

use reqwest::header::AUTHORIZATION;
use serde::Serialize;
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TtsBackend {
    Irodori,
}

impl std::fmt::Display for TtsBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TtsBackend::Irodori => write!(f, "irodori"),
        }
    }
}

impl std::str::FromStr for TtsBackend {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "irodori" => Ok(TtsBackend::Irodori),
            _ => Err(format!("unknown TTS backend: {s}")),
        }
    }
}

#[derive(Debug, Clone)]
pub struct TtsConfig {
    pub backend: TtsBackend,
    pub url: String,
    pub api_key: Option<String>,
}

impl Default for TtsConfig {
    fn default() -> Self {
        Self {
            backend: TtsBackend::Irodori,
            url: "http://127.0.0.1:8088".to_string(),
            api_key: None,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct TtsOptions {
    pub response_format: Option<String>,
    pub speed: Option<f32>,
}

#[derive(Debug, Clone, Default)]
pub struct TtsRequest {
    pub text: String,
    pub voice: Option<String>,
    pub reference_audio_path: Option<PathBuf>,
    pub options: TtsOptions,
}

#[derive(Debug, Error)]
pub enum TtsError {
    #[error("http error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("api error {status}: {message}")]
    Api { status: u16, message: String },
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

pub struct TtsClient {
    config: TtsConfig,
    http: reqwest::Client,
}

impl TtsClient {
    pub fn new(config: TtsConfig) -> Self {
        let http = reqwest::Client::builder()
            .connect_timeout(std::time::Duration::from_secs(10))
            .timeout(std::time::Duration::from_secs(120))
            .build()
            .unwrap_or_default();
        Self { config, http }
    }

    pub fn config(&self) -> &TtsConfig {
        &self.config
    }

    pub async fn synthesize(&self, request: &TtsRequest) -> Result<Vec<u8>, TtsError> {
        #[derive(Serialize)]
        struct Body {
            model: String,
            input: String,
            #[serde(skip_serializing_if = "Option::is_none")]
            voice: Option<String>,
            #[serde(skip_serializing_if = "Option::is_none")]
            response_format: Option<String>,
            #[serde(skip_serializing_if = "Option::is_none")]
            speed: Option<f32>,
        }

        let body = Body {
            model: "irodori-tts".to_string(),
            input: request.text.clone(),
            voice: request.voice.clone(),
            response_format: request.options.response_format.clone(),
            speed: request.options.speed,
        };

        let url = format!("{}/v1/audio/speech", self.config.url.trim_end_matches('/'));
        let mut builder = self.http.post(&url).json(&body);
        if let Some(key) = &self.config.api_key {
            builder = builder.header(AUTHORIZATION, format!("Bearer {key}"));
        }

        let response = builder.send().await?;
        let status = response.status();
        if !status.is_success() {
            let message = response.text().await.unwrap_or_default();
            return Err(TtsError::Api {
                status: status.as_u16(),
                message,
            });
        }
        Ok(response.bytes().await?.to_vec())
    }
}

#[async_trait::async_trait]
pub trait TextToSpeech: Send + Sync {
    async fn synthesize(&self, request: &TtsRequest) -> Result<Vec<u8>, TtsError>;
}

#[async_trait::async_trait]
impl TextToSpeech for TtsClient {
    async fn synthesize(&self, request: &TtsRequest) -> Result<Vec<u8>, TtsError> {
        self.synthesize(request).await
    }
}

/// Pick the first audio file in `refs_dir` and return its path and stem.
pub fn pick_reference_voice(refs_dir: &Path) -> std::io::Result<Option<(PathBuf, String)>> {
    if !refs_dir.is_dir() {
        return Ok(None);
    }
    let mut entries: Vec<_> = std::fs::read_dir(refs_dir)?
        .filter_map(|e| e.ok())
        .collect();
    entries.sort_by_key(|e| e.path());
    for entry in entries {
        let path = entry.path();
        if path.is_file()
            && let Some(ext) = path.extension()
        {
            let ext = ext.to_string_lossy().to_lowercase();
            if ["wav", "mp3", "flac", "m4a", "ogg", "opus", "aac", "webm"].contains(&ext.as_str()) {
                let stem = path
                    .file_stem()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string();
                return Ok(Some((path, stem)));
            }
        }
    }
    Ok(None)
}
