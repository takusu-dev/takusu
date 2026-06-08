use std::path::{Path, PathBuf};

use hf_hub::api::tokio as hf_api;
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

use thiserror::Error;

pub use hf_hub::Repo;
pub use whisper_rs::WhisperContextParameters as TranscriberParams;

pub const DEFAULT_MODEL_REPO: &str = "kotoba-tech/kotoba-whisper-v2.0-ggml";
pub const DEFAULT_MODEL_FILE: &str = "ggml-kotoba-whisper-v2.0-q5_0.bin";

#[derive(Debug, Error)]
pub enum STTError {
    #[error("hf api error: {0}")]
    HFApi(#[from] hf_api::ApiError),
    #[error("whisper error: {0}")]
    Whisper(#[from] whisper_rs::WhisperError),
    #[error("model file not found")]
    ModelNotFound,
}

pub struct Transcriber {
    ctx: WhisperContext,
}

impl Transcriber {
    pub async fn download(repo: &str, filename: &str) -> Result<PathBuf, STTError> {
        let hf = hf_api::Api::new()?;
        let repo = hf_hub::Repo::new(repo.to_string(), hf_hub::RepoType::Model);
        let path = hf.repo(repo).get(filename).await?;
        Ok(path)
    }

    pub fn new(model_path: &Path) -> Result<Self, STTError> {
        let ctx = WhisperContext::new_with_params(model_path, WhisperContextParameters::default())?;
        Ok(Self { ctx })
    }

    pub fn transcribe(&self, audio: &[f32], language: Option<&str>) -> Result<String, STTError> {
        let mut state = self.ctx.create_state()?;
        let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });

        if let Some(lang) = language {
            params.set_language(Some(lang));
        }
        params.set_print_progress(false);
        params.set_print_special(false);
        params.set_print_realtime(false);
        params.set_print_timestamps(false);

        state.full(params, audio)?;

        let n = state.full_n_segments();
        if n == 0 {
            return Ok(String::new());
        }

        let mut result = String::new();
        for i in 0..n {
            if let Some(seg) = state.get_segment(i) {
                let text = seg.to_string();
                if !result.is_empty() {
                    result.push(' ');
                }
                result.push_str(text.trim());
            }
        }

        Ok(result)
    }
}
