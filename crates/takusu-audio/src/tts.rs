//! Text-to-speech provider trait and shared request/response types.
//!
//! `TextToSpeech` returns a chunked byte stream so callers can play audio
//! incrementally. The default `synthesize` method collects that stream into
//! a single `Vec<u8>` for callers that do not need streaming.

use std::path::PathBuf;
use std::pin::Pin;

use bytes::Bytes;
use futures_util::Stream;
use thiserror::Error;

/// TTS backend identifier.
///
/// Currently empty while the legacy Irodori-TTS backend is being removed.
/// New backends will add variants here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TtsBackend {}

impl std::fmt::Display for TtsBackend {
    fn fmt(&self, _f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match *self {}
    }
}

impl std::str::FromStr for TtsBackend {
    type Err = String;
    fn from_str(_s: &str) -> Result<Self, Self::Err> {
        Err("no TTS backends are currently available".to_string())
    }
}

#[derive(Debug, Clone)]
pub struct TtsConfig {
    pub backend: TtsBackend,
    pub url: String,
    pub api_key: Option<String>,
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

/// A stream of audio chunks produced by a TTS backend.
pub type TtsStream =
    Pin<Box<dyn Stream<Item = Result<Bytes, TtsError>> + Send + 'static>>;

#[async_trait::async_trait]
pub trait TextToSpeech: Send + Sync {
    /// Synthesize the request into a chunked audio stream.
    async fn synthesize_stream(&self, request: &TtsRequest) -> Result<TtsStream, TtsError>;

    /// Synthesize the request into a single audio buffer.
    ///
    /// The default implementation collects `synthesize_stream` into a `Vec<u8>`.
    async fn synthesize(&self, request: &TtsRequest) -> Result<Vec<u8>, TtsError> {
        use futures_util::TryStreamExt;

        let stream = self.synthesize_stream(request).await?;
        let chunks: Vec<Bytes> = stream.try_collect().await?;
        let mut audio = Vec::with_capacity(chunks.iter().map(|c| c.len()).sum());
        for chunk in chunks {
            audio.extend_from_slice(&chunk);
        }
        Ok(audio)
    }
}
