use std::path::{Path, PathBuf};
use std::sync::Mutex;

use takusu_audio::{
    CartesiaOutputFormat, CartesiaSonic, CartesiaSonicConfig, Hush, SherpaOnnxAsr,
    SherpaOnnxAsrConfig, SherpaOnnxModel, SpeechToText, SttError, TextToSpeech, TtsOptions,
    TtsRequest,
};
use tokio::runtime::Runtime;

use crate::TakusuError;

/// Android audio bridge. Recording is performed by Kotlin AudioRecord; model
/// inference and provider TTS stay in Rust so desktop and Android share the
/// same audio backend behavior.
///
/// STT models (Hush + Sherpa) are loaded lazily on the first call to
/// `transcribe_pcm` so that users who only want Android system TTS do not need
/// to download STT weights up front.
#[derive(uniffi::Object)]
pub struct MobileAudio {
    hush: Mutex<Option<Hush>>,
    stt: Mutex<Option<SherpaOnnxAsr>>,
    tts: Option<CartesiaSonic>,
    runtime: Mutex<Option<Runtime>>,
    model_dir: PathBuf,
    language: String,
    voice_id: String,
    sample_rate: u32,
    speed: Option<f32>,
}

#[uniffi::export]
impl MobileAudio {
    #[uniffi::constructor]
    pub fn new(
        model_dir: String,
        api_key: String,
        voice_id: String,
        language: String,
        sample_rate: u32,
        speed: Option<f32>,
    ) -> Result<Self, TakusuError> {
        let root = Path::new(&model_dir).to_path_buf();
        let tts = if api_key.trim().is_empty() {
            None
        } else {
            let mut tts_config = CartesiaSonicConfig::new(api_key);
            tts_config.voice_id = voice_id.clone();
            tts_config.language = Some(language.clone());
            tts_config.output_format = CartesiaOutputFormat::mp3(sample_rate, 128_000);
            Some(CartesiaSonic::new(tts_config))
        };
        Ok(Self {
            hush: Mutex::new(None),
            stt: Mutex::new(None),
            tts,
            runtime: Mutex::new(Some(Runtime::new().map_err(|error| TakusuError::Audio {
                detail: format!("failed to create audio runtime: {error}"),
            })?)),
            model_dir: root,
            language,
            voice_id,
            sample_rate,
            speed,
        })
    }

    pub fn shutdown(&self) -> Result<(), TakusuError> {
        let mut guard = self.runtime.lock().map_err(|error| TakusuError::Audio {
            detail: format!("runtime lock poisoned: {error}"),
        })?;
        if let Some(runtime) = guard.take() {
            runtime.shutdown_background();
        }
        Ok(())
    }

    pub fn transcribe_pcm(&self, samples: Vec<i16>) -> Result<String, TakusuError> {
        if samples.is_empty() {
            return Err(TakusuError::Audio {
                detail: "recording was empty".to_string(),
            });
        }
        let pcm: Vec<f32> = samples
            .into_iter()
            .map(|sample| sample as f32 / i16::MAX as f32)
            .collect();

        let mut hush_guard = self.hush.lock().map_err(|error| TakusuError::Audio {
            detail: format!("Hush lock poisoned: {error}"),
        })?;
        if hush_guard.is_none() {
            let hush = Hush::from_model_dir(self.model_dir.join("hush")).map_err(|error| {
                TakusuError::Audio {
                    detail: format!("failed to load Hush: {error}"),
                }
            })?;
            *hush_guard = Some(hush);
        }
        let hush = hush_guard.as_mut().unwrap();
        let enhanced = hush.enhance(&pcm).map_err(|error| TakusuError::Audio {
            detail: format!("Hush inference failed: {error}"),
        })?;
        drop(hush_guard);

        let mut stt_guard = self.stt.lock().map_err(|error| TakusuError::Audio {
            detail: format!("SenseVoice lock poisoned: {error}"),
        })?;
        if stt_guard.is_none() {
            let stt = SherpaOnnxAsr::from_config(&SherpaOnnxAsrConfig {
                model: SherpaOnnxModel::SenseVoice,
                model_dir: self.model_dir.join("sherpa-sense-voice-int8"),
                sample_rate: 16_000,
                language: Some(self.language.clone()),
                use_itn: true,
                ..Default::default()
            })
            .map_err(|error: SttError| TakusuError::Audio {
                detail: format!("failed to load SenseVoice: {error}"),
            })?;
            *stt_guard = Some(stt);
        }
        let stt = stt_guard.as_ref().unwrap();
        let runtime = self.runtime.lock().map_err(|error| TakusuError::Audio {
            detail: format!("runtime lock poisoned: {error}"),
        })?;
        let runtime = runtime.as_ref().ok_or(TakusuError::Audio {
            detail: "audio runtime has been shut down".to_string(),
        })?;
        runtime
            .block_on(stt.transcribe(&enhanced))
            .map_err(|error| TakusuError::Audio {
                detail: format!("SenseVoice inference failed: {error}"),
            })
    }

    pub fn synthesize(&self, text: String) -> Result<Vec<u8>, TakusuError> {
        if text.trim().is_empty() {
            return Err(TakusuError::Audio {
                detail: "TTS text was empty".to_string(),
            });
        }
        let tts = self.tts.as_ref().ok_or(TakusuError::Audio {
            detail: "TTS backend is not configured".to_string(),
        })?;
        let request = TtsRequest {
            text,
            voice: Some(self.voice_id.clone()),
            reference_audio_path: None,
            options: TtsOptions {
                response_format: Some("mp3".to_string()),
                speed: self.speed,
            },
        };
        let guard = self.runtime.lock().map_err(|error| TakusuError::Audio {
            detail: format!("runtime lock poisoned: {error}"),
        })?;
        let runtime = guard.as_ref().ok_or(TakusuError::Audio {
            detail: "audio runtime has been shut down".to_string(),
        })?;
        runtime
            .block_on(tts.synthesize(&request))
            .map_err(|error| TakusuError::Audio {
                detail: format!("TTS failed at {} Hz: {error}", self.sample_rate),
            })
    }
}
