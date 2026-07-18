use std::path::Path;
use std::sync::Mutex;

use takusu_audio::{
    CartesiaOutputFormat, CartesiaSonic, CartesiaSonicConfig, Hush, SherpaOnnxAsr,
    SherpaOnnxAsrConfig, SherpaOnnxModel, SpeechToText, TextToSpeech, TtsOptions, TtsRequest,
};
use tokio::runtime::Runtime;

use crate::TakusuError;

/// Android audio bridge. Recording is performed by Kotlin AudioRecord; model
/// inference and provider TTS stay in Rust so desktop and Android share the
/// same audio backend behavior.
#[derive(uniffi::Object)]
pub struct MobileAudio {
    hush: Mutex<Hush>,
    stt: SherpaOnnxAsr,
    tts: CartesiaSonic,
    runtime: Runtime,
    voice_id: String,
    sample_rate: u32,
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
    ) -> Result<Self, TakusuError> {
        let root = Path::new(&model_dir);
        let hush = Hush::from_model_dir(root.join("hush")).map_err(|error| TakusuError::Audio {
            detail: format!("failed to load Hush: {error}"),
        })?;
        let stt = SherpaOnnxAsr::from_config(&SherpaOnnxAsrConfig {
            model: SherpaOnnxModel::SenseVoice,
            model_dir: root.join("sherpa-sense-voice-int8"),
            sample_rate: 16_000,
            language: Some(language),
            use_itn: true,
            ..Default::default()
        })
        .map_err(|error| TakusuError::Audio {
            detail: format!("failed to load SenseVoice: {error}"),
        })?;
        let mut tts_config = CartesiaSonicConfig::new(api_key);
        tts_config.voice_id = voice_id.clone();
        tts_config.output_format = CartesiaOutputFormat::mp3(sample_rate, 128_000);
        Ok(Self {
            hush: Mutex::new(hush),
            stt,
            tts: CartesiaSonic::new(tts_config),
            runtime: Runtime::new().map_err(|error| TakusuError::Audio {
                detail: format!("failed to create audio runtime: {error}"),
            })?,
            voice_id,
            sample_rate,
        })
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
        let enhanced = self
            .hush
            .lock()
            .map_err(|error| TakusuError::Audio {
                detail: format!("Hush lock poisoned: {error}"),
            })?
            .enhance(&pcm)
            .map_err(|error| TakusuError::Audio {
                detail: format!("Hush inference failed: {error}"),
            })?;
        self.runtime
            .block_on(self.stt.transcribe(&enhanced))
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
        let request = TtsRequest {
            text,
            voice: Some(self.voice_id.clone()),
            reference_audio_path: None,
            options: TtsOptions {
                response_format: Some("mp3".to_string()),
                speed: None,
            },
        };
        self.runtime
            .block_on(self.tts.synthesize(&request))
            .map_err(|error| TakusuError::Audio {
                detail: format!("TTS failed at {} Hz: {error}", self.sample_rate),
            })
    }
}
