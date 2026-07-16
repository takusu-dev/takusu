use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use takusu_audio::{DownloadProgress, DownloadStage, ModelCache};

use crate::TakusuError;

/// Check whether a bundled local audio model is already downloaded and complete.
#[uniffi::export]
pub fn is_model_cached(cache_dir: String, model_id: String) -> Result<bool, TakusuError> {
    let cache = ModelCache::new(&cache_dir);
    cache
        .is_cached(&model_id)
        .map_err(|error| TakusuError::Model {
            detail: error.to_string(),
        })
}

/// Download a bundled local audio model and write progress atomically to a
/// small status file so Android WorkManager can render progress while the
/// blocking archive extraction runs on its worker thread.
#[uniffi::export]
pub fn download_model(
    cache_dir: String,
    model_id: String,
    status_path: String,
) -> Result<String, TakusuError> {
    let cache = ModelCache::new(&cache_dir);
    let status_path = PathBuf::from(status_path);
    let status_model_id = model_id.clone();
    let last_write = Arc::new(Mutex::new(Instant::now()));
    let callback = Arc::new(move |progress: DownloadProgress| {
        let stage = match progress.stage {
            DownloadStage::Downloading => "downloading",
            DownloadStage::Extracting => "extracting",
            DownloadStage::Verifying => "verifying",
        };
        let should_write = stage != "downloading"
            || last_write
                .lock()
                .is_ok_and(|last| last.elapsed() >= Duration::from_millis(500));
        if !should_write {
            return;
        }
        let body = serde_json::json!({
            "modelId": status_model_id,
            "downloadedBytes": progress.downloaded_bytes,
            "totalBytes": progress.total_bytes,
            "stage": stage,
        });
        let temp = status_path.with_extension("tmp");
        if fs::write(&temp, body.to_string()).is_ok() {
            let _ = fs::rename(temp, &status_path);
        }
        if let Ok(mut last) = last_write.lock() {
            *last = Instant::now();
        }
    });
    cache
        .ensure_with_progress(&model_id, Some(callback))
        .map(|path| path.to_string_lossy().into_owned())
        .map_err(|error| TakusuError::Model {
            detail: error.to_string(),
        })
}
