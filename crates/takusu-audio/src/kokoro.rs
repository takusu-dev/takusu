//! Kokoro TTS backend via `sherpa-onnx` (offline ONNX text-to-speech).
//!
//! Supports the `kokoro-en-v0_19` model bundle from the sherpa-onnx
//! `tts-models` release, which contains English voices.

use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use hound::{WavSpec, WavWriter};
use sherpa_onnx::{
    GenerationConfig, OfflineTts, OfflineTtsConfig, OfflineTtsKokoroModelConfig,
    OfflineTtsModelConfig,
};

use crate::models::find_file_recursive;
use crate::tts::{TextToSpeech, TtsError, TtsRequest};

/// Configuration for [`Kokoro`].
#[derive(Debug, Clone)]
pub struct KokoroConfig {
    pub model_dir: PathBuf,
    pub num_threads: i32,
    pub provider: String,
    pub debug: bool,
}

impl Default for KokoroConfig {
    fn default() -> Self {
        Self {
            model_dir: PathBuf::new(),
            num_threads: 2,
            provider: "cpu".to_string(),
            debug: false,
        }
    }
}

/// Kokoro ONNX text-to-speech backend.
///
/// The model directory is expected to contain the files from the
/// `kokoro-en-v0_19` release:
///
///   - `model.onnx` (or `model.int8.onnx`)
///   - `voices.bin`
///   - `tokens.txt`
///   - `espeak-ng-data/` directory
///
/// Use [`ModelCache::ensure("kokoro-en-v0_19")`][crate::ModelCache::ensure]
/// to download the bundle on first run.
pub struct Kokoro {
    tts: Arc<OfflineTts>,
    sample_rate: i32,
}

impl Kokoro {
    /// Create a Kokoro TTS engine from a full configuration.
    pub fn from_config(config: &KokoroConfig) -> Result<Self, TtsError> {
        let dir = &config.model_dir;
        if !dir.is_dir() {
            return Err(TtsError::Other(format!(
                "model directory not found: {}",
                dir.display()
            )));
        }

        let model = find_file(dir, "model")
            .ok_or_else(|| TtsError::Other(format!("no model*.onnx found in {}", dir.display())))?;
        let voices = dir.join("voices.bin");
        let tokens = dir.join("tokens.txt");
        let data_dir = find_data_dir(dir).ok_or_else(|| {
            TtsError::Other(format!("espeak-ng-data not found in {}", dir.display()))
        })?;

        for (name, path) in [("voices.bin", &voices), ("tokens.txt", &tokens)] {
            if !path.exists() {
                return Err(TtsError::Other(format!(
                    "{name} not found in {}",
                    dir.display()
                )));
            }
        }

        let tts_config = OfflineTtsConfig {
            model: OfflineTtsModelConfig {
                kokoro: OfflineTtsKokoroModelConfig {
                    model: Some(model.to_string_lossy().to_string()),
                    voices: Some(voices.to_string_lossy().to_string()),
                    tokens: Some(tokens.to_string_lossy().to_string()),
                    data_dir: Some(data_dir.to_string_lossy().to_string()),
                    length_scale: 1.0,
                    ..Default::default()
                },
                num_threads: config.num_threads,
                provider: Some(config.provider.clone()),
                debug: config.debug,
                ..Default::default()
            },
            ..Default::default()
        };

        let tts = OfflineTts::create(&tts_config)
            .ok_or_else(|| TtsError::Other("failed to create Kokoro TTS engine".to_string()))?;
        let sample_rate = tts.sample_rate();
        Ok(Self {
            tts: Arc::new(tts),
            sample_rate,
        })
    }

    /// Create a Kokoro TTS engine from a model directory with sensible defaults.
    pub fn from_model_dir(dir: impl AsRef<Path>) -> Result<Self, TtsError> {
        Self::from_config(&KokoroConfig {
            model_dir: dir.as_ref().to_path_buf(),
            ..Default::default()
        })
    }
}

#[async_trait::async_trait]
impl TextToSpeech for Kokoro {
    async fn synthesize(&self, request: &TtsRequest) -> Result<Vec<u8>, TtsError> {
        let sid = parse_voice(&request.voice)?;
        let speed = request.options.speed.unwrap_or(1.0);
        let text = request.text.clone();
        let gen_config = GenerationConfig {
            sid,
            speed,
            ..Default::default()
        };

        let tts = self.tts.clone();
        let samples = tokio::task::spawn_blocking(move || {
            let audio = tts
                .generate_with_config(&text, &gen_config, None::<fn(&[f32], f32) -> bool>)
                .ok_or_else(|| TtsError::Other("failed to generate audio".to_string()))?;
            Ok::<Vec<f32>, TtsError>(audio.samples().to_vec())
        })
        .await
        .map_err(|e| TtsError::Other(format!("synthesis task failed: {e}")))??;

        samples_to_wav(&samples, self.sample_rate)
    }
}

fn parse_voice(voice: &Option<String>) -> Result<i32, TtsError> {
    match voice {
        Some(v) => v
            .parse::<i32>()
            .map_err(|e| TtsError::Other(format!("invalid voice id '{v}': {e}"))),
        None => Ok(0),
    }
}

fn samples_to_wav(samples: &[f32], sample_rate: i32) -> Result<Vec<u8>, TtsError> {
    let spec = WavSpec {
        channels: 1,
        sample_rate: sample_rate as u32,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };

    let mut cursor = Cursor::new(Vec::new());
    {
        let mut writer =
            WavWriter::new(&mut cursor, spec).map_err(|e| TtsError::Other(e.to_string()))?;
        let max = samples.iter().map(|s| s.abs()).fold(0.0f32, f32::max);
        let scale = if max > 1.0 { 32767.0 / max } else { 32767.0 };
        for &s in samples {
            let clamped = (s * scale).clamp(-32768.0, 32767.0);
            writer
                .write_sample(clamped as i16)
                .map_err(|e| TtsError::Other(e.to_string()))?;
        }
        writer
            .finalize()
            .map_err(|e| TtsError::Other(e.to_string()))?;
    }
    Ok(cursor.into_inner())
}

fn find_file(dir: &Path, prefix: &str) -> Option<PathBuf> {
    let names = ["", ".int8"];
    for name in names {
        let candidate = dir.join(format!("{}{}.onnx", prefix, name));
        if candidate.exists() {
            return Some(candidate);
        }
    }
    None
}

fn find_data_dir(dir: &Path) -> Option<PathBuf> {
    let espeak = dir.join("espeak-ng-data");
    if espeak.is_dir() && espeak.join("phontab").exists() {
        return Some(espeak);
    }
    if let Some(phontab) = find_file_recursive(dir, "phontab") {
        return phontab.parent().map(PathBuf::from);
    }
    None
}
