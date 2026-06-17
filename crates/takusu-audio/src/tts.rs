//! Text-to-speech clients for Irodori-TTS and fish-speech inference servers.

use std::path::{Path, PathBuf};

use reqwest::header::{AUTHORIZATION, CONTENT_TYPE, HeaderMap, HeaderValue};
use serde::Serialize;
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TtsBackend {
    Irodori,
    FishSpeech,
}

impl std::fmt::Display for TtsBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TtsBackend::Irodori => write!(f, "irodori"),
            TtsBackend::FishSpeech => write!(f, "fish-speech"),
        }
    }
}

impl std::str::FromStr for TtsBackend {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "irodori" => Ok(TtsBackend::Irodori),
            "fish" | "fish-speech" => Ok(TtsBackend::FishSpeech),
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
    pub chunk_length: Option<usize>,
    pub top_p: Option<f32>,
    pub temperature: Option<f32>,
    pub repetition_penalty: Option<f32>,
    pub max_new_tokens: Option<usize>,
    pub seed: Option<i64>,
}

#[derive(Debug, Clone, Default)]
pub struct TtsRequest {
    pub text: String,
    pub voice: Option<String>,
    pub reference_id: Option<String>,
    pub reference_audio_path: Option<PathBuf>,
    pub reference_text: Option<String>,
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
    #[error("serialization error: {0}")]
    Serialization(#[from] rmp_serde::encode::Error),
    #[error("invalid header value")]
    InvalidHeader(#[from] reqwest::header::InvalidHeaderValue),
}

pub struct TtsClient {
    config: TtsConfig,
    http: reqwest::Client,
}

impl TtsClient {
    pub fn new(config: TtsConfig) -> Self {
        Self {
            config,
            http: reqwest::Client::new(),
        }
    }

    pub fn config(&self) -> &TtsConfig {
        &self.config
    }

    pub async fn synthesize(&self, request: &TtsRequest) -> Result<Vec<u8>, TtsError> {
        match self.config.backend {
            TtsBackend::Irodori => self.synthesize_irodori(request).await,
            TtsBackend::FishSpeech => self.synthesize_fish(request).await,
        }
    }

    async fn synthesize_irodori(&self, request: &TtsRequest) -> Result<Vec<u8>, TtsError> {
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

    async fn synthesize_fish(&self, request: &TtsRequest) -> Result<Vec<u8>, TtsError> {
        #[derive(Serialize)]
        struct ReferenceAudio {
            audio: Vec<u8>,
            text: String,
        }

        #[derive(Serialize)]
        struct Body {
            text: String,
            references: Vec<ReferenceAudio>,
            #[serde(skip_serializing_if = "Option::is_none")]
            reference_id: Option<String>,
            #[serde(skip_serializing_if = "Option::is_none")]
            format: Option<String>,
            #[serde(skip_serializing_if = "Option::is_none")]
            chunk_length: Option<usize>,
            #[serde(skip_serializing_if = "Option::is_none")]
            top_p: Option<f32>,
            #[serde(skip_serializing_if = "Option::is_none")]
            repetition_penalty: Option<f32>,
            #[serde(skip_serializing_if = "Option::is_none")]
            temperature: Option<f32>,
            #[serde(skip_serializing_if = "Option::is_none")]
            max_new_tokens: Option<usize>,
            #[serde(skip_serializing_if = "Option::is_none")]
            seed: Option<i64>,
            streaming: bool,
            use_memory_cache: String,
            latency: String,
        }

        let references = match &request.reference_audio_path {
            Some(path) => {
                let audio = tokio::fs::read(path).await?;
                vec![ReferenceAudio {
                    audio,
                    text: request.reference_text.clone().unwrap_or_default(),
                }]
            }
            None => Vec::new(),
        };

        let body = Body {
            text: request.text.clone(),
            references,
            reference_id: request.reference_id.clone(),
            format: request.options.response_format.clone(),
            chunk_length: request.options.chunk_length,
            top_p: request.options.top_p,
            repetition_penalty: request.options.repetition_penalty,
            temperature: request.options.temperature,
            max_new_tokens: request.options.max_new_tokens,
            seed: request.options.seed,
            streaming: false,
            use_memory_cache: "off".to_string(),
            latency: "normal".to_string(),
        };

        let url = format!("{}/v1/tts", self.config.url.trim_end_matches('/'));
        let packed = rmp_serde::to_vec_named(&body)?;

        let mut headers = HeaderMap::new();
        headers.insert(
            CONTENT_TYPE,
            HeaderValue::from_static("application/msgpack"),
        );
        if let Some(key) = &self.config.api_key {
            headers.insert(
                AUTHORIZATION,
                HeaderValue::from_str(&format!("Bearer {key}"))?,
            );
        }

        let response = self
            .http
            .post(&url)
            .query(&[("format", "msgpack")])
            .headers(headers)
            .body(packed)
            .send()
            .await?;

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
