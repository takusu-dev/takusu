//! Text-to-speech provider trait and shared request/response types.
//!
//! Backends other than the legacy Irodori-TTS implementation will be added
//! here as concrete `TextToSpeech` implementors.

use std::path::PathBuf;

use thiserror::Error;

/// TTS backend identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TtsBackend {
    Kokoro,
}

impl std::fmt::Display for TtsBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TtsBackend::Kokoro => write!(f, "kokoro"),
        }
    }
}

impl std::str::FromStr for TtsBackend {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "kokoro" | "kokoro-onnx" => Ok(TtsBackend::Kokoro),
            _ => Err(format!("unknown TTS backend: {s}")),
        }
    }
}

#[derive(Debug, Clone)]
pub struct TtsConfig {
    pub backend: TtsBackend,
    pub url: String,
    pub api_key: Option<String>,
    pub model_dir: Option<PathBuf>,
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
    #[error("other error: {0}")]
    Other(String),
}

#[async_trait::async_trait]
pub trait TextToSpeech: Send + Sync {
    async fn synthesize(&self, request: &TtsRequest) -> Result<Vec<u8>, TtsError>;
}
