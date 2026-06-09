use std::path::{Path, PathBuf};

use hf_hub::api::tokio as hf_api;
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

use thiserror::Error;

pub use hf_hub::Repo;
pub use whisper_rs::WhisperContextParameters as TranscriberParams;

pub const DEFAULT_MODEL_REPO: &str = "ggerganov/whisper.cpp";
pub const DEFAULT_MODEL_FILE: &str = "ggml-base.bin";

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
    n_threads: u32,
}

impl Transcriber {
    pub async fn download(repo: &str, filename: &str) -> Result<PathBuf, STTError> {
        let hf = hf_api::Api::new()?;
        let repo = hf_hub::Repo::new(repo.to_string(), hf_hub::RepoType::Model);
        let path = hf.repo(repo).get(filename).await?;
        Ok(path)
    }

    pub fn new(model_path: &Path) -> Result<Self, STTError> {
        Self::with_threads(model_path, std::thread::available_parallelism()
            .map(|n| n.get() as u32)
            .unwrap_or(4))
    }

    pub fn with_threads(model_path: &Path, n_threads: u32) -> Result<Self, STTError> {
        let mut params = WhisperContextParameters::default();
        params.flash_attn(true);
        let ctx = WhisperContext::new_with_params(model_path, params)?;
        Ok(Self { ctx, n_threads })
    }

    pub fn transcribe(&self, audio: &[f32], language: Option<&str>) -> Result<String, STTError> {
        let mut state = self.ctx.create_state()?;
        let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });

        params.set_n_threads(self.n_threads as i32);
        params.set_language(Some(language.unwrap_or("ja")));
        params.set_print_progress(false);
        params.set_print_special(false);
        params.set_print_realtime(false);
        params.set_print_timestamps(false);
        params.set_single_segment(true);
        params.set_no_timestamps(true);
        

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
