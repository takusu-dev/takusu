//! Speech-to-text provider trait.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum SttError {
    #[error("connection error: {0}")]
    Connection(String),
    #[error("server error: {0}")]
    Server(String),
    #[error("no result received")]
    NoResult,
    #[error("other error: {0}")]
    Other(String),
}

#[async_trait::async_trait]
pub trait SpeechToText: Send + Sync {
    async fn transcribe(&self, audio: &[f32]) -> Result<String, SttError>;
}
