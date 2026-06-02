use hf_hub::api::tokio as api;

use whisper_rs::{WhisperContext, WhisperState};

use thiserror::Error;

pub use hf_hub::Repo;
pub use whisper_rs::WhisperContextParameters;

#[derive(Debug, Error)]
pub enum STTError {
    #[error("hf api error: {0}")]
    HFApi(#[from] api::ApiError),
    #[error("whisper error: {0}")]
    Whisper(#[from] whisper_rs::WhisperError),
}

async fn state(
    repo: Repo,
    filename: &str,
    params: WhisperContextParameters<'_>,
) -> Result<WhisperModel, STTError> {
    let hf_api = api::Api::new()?;

    let path = hf_api.repo(repo).get(filename).await?;

    let context = WhisperContext::new_with_params(path, params)?;
    let state = context.create_state()?;
    Ok(WhisperModel(state))
}

pub struct WhisperModel(WhisperState);

impl WhisperModel {}
