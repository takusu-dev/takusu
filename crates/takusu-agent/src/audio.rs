//! Application-level audio adapter for the takusu agent.
//!
//! This module is responsible for the push-to-talk loop:
//! record → transcribe → agent turn → synthesize → play.
//! It is not exposed as an LLM tool.

use std::time::Duration;

use serde::Deserialize;
use takusu_audio::play::{AudioClip, PlayError};
use takusu_audio::{
    CartesiaSonic, CartesiaSonicConfig, FunASRClient, FunASRConfig, FunASRMode, RecordConfig,
    SpeechToText, TextToSpeech, TtsOptions, TtsRequest, default_hotwords, record,
};
use thiserror::Error;

use crate::{AgentError, AgentSession};

#[derive(Debug, Error)]
pub enum AudioError {
    #[error("recording failed: {0}")]
    Record(String),
    #[error("transcription failed: {0}")]
    Transcribe(String),
    #[error("agent turn failed: {0}")]
    Agent(#[from] AgentError),
    #[error("tts failed: {0}")]
    Tts(String),
    #[error("playback failed: {0}")]
    Play(String),
    #[error("audio backend {0} is not supported")]
    UnsupportedBackend(String),
    #[error("audio operation timed out")]
    Timeout,
}

impl From<takusu_audio::tts::TtsError> for AudioError {
    fn from(e: takusu_audio::tts::TtsError) -> Self {
        AudioError::Tts(e.to_string())
    }
}

impl From<PlayError> for AudioError {
    fn from(e: PlayError) -> Self {
        AudioError::Play(e.to_string())
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct AudioConfig {
    pub stt: SttConfig,
    pub tts: TtsConfig,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct SttConfig {
    #[serde(default = "default_stt_backend")]
    pub backend: String,
    #[serde(default = "default_stt_url")]
    pub url: String,
    #[serde(default = "default_stt_language")]
    pub language: String,
    #[serde(default)]
    pub hotwords: Vec<String>,
    #[serde(default = "default_stt_mode")]
    pub mode: String,
}

impl Default for SttConfig {
    fn default() -> Self {
        Self {
            backend: default_stt_backend(),
            url: default_stt_url(),
            language: default_stt_language(),
            hotwords: Vec::new(),
            mode: default_stt_mode(),
        }
    }
}

fn default_stt_backend() -> String {
    "funasr".into()
}

fn default_stt_url() -> String {
    "ws://127.0.0.1:10095".into()
}

fn default_stt_language() -> String {
    "ja".into()
}

fn default_stt_mode() -> String {
    "offline".into()
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct TtsConfig {
    #[serde(default = "default_tts_backend")]
    pub backend: String,
    #[serde(default = "default_tts_api_key_env")]
    pub api_key_env: String,
    pub api_key: String,
    #[serde(default = "default_tts_voice_id")]
    pub voice_id: String,
    #[serde(default = "default_tts_language")]
    pub language: String,
    #[serde(default = "default_tts_sample_rate")]
    pub sample_rate: u32,
    pub speed: Option<f32>,
}

impl Default for TtsConfig {
    fn default() -> Self {
        Self {
            backend: default_tts_backend(),
            api_key_env: default_tts_api_key_env(),
            api_key: String::new(),
            voice_id: default_tts_voice_id(),
            language: default_tts_language(),
            sample_rate: default_tts_sample_rate(),
            speed: None,
        }
    }
}

fn default_tts_backend() -> String {
    "cartesia".into()
}

fn default_tts_api_key_env() -> String {
    "CARTESIA_API_KEY".into()
}

fn default_tts_voice_id() -> String {
    "db6b0ed5-d5d3-463d-ae85-518a07d3c2b4".into()
}

fn default_tts_language() -> String {
    "ja".into()
}

fn default_tts_sample_rate() -> u32 {
    44100
}

/// Application-level audio adapter. Owns the agent session and the audio clients.
pub struct AudioAdapter {
    session: AgentSession,
    stt: Box<dyn SpeechToText>,
    tts: Box<dyn TextToSpeech>,
    tts_voice_id: String,
    tts_speed: Option<f32>,
}

impl AudioAdapter {
    /// Create an audio adapter from an existing agent session.
    pub fn new(session: AgentSession) -> Result<Self, AudioError> {
        let config = &session.config.audio;
        let stt = build_stt(&config.stt)?;
        let (tts, voice_id, speed) = build_tts(&config.tts)?;
        Ok(Self {
            session,
            stt,
            tts,
            tts_voice_id: voice_id,
            tts_speed: speed,
        })
    }

    /// Run the push-to-talk loop until interrupted or an unrecoverable error occurs.
    pub async fn run(&self, no_tts: bool) -> Result<(), AudioError> {
        loop {
            let samples = record_with_timeout(Duration::from_secs(60)).await?;
            if samples.is_empty() {
                continue;
            }

            let text =
                transcribe_with_timeout(self.stt.as_ref(), &samples, Duration::from_secs(120))
                    .await?;
            if text.trim().is_empty() {
                continue;
            }

            eprintln!("> {text}");

            let result = self.session.run_turn(&text).await?;

            println!("{}", result.text);
            if !result.changes.is_empty() {
                match serde_json::to_string_pretty(&result.changes) {
                    Ok(changes) => eprintln!("{changes}"),
                    Err(e) => eprintln!("changes: {e}"),
                }
            }
            if result.schedule_dirty {
                eprintln!("schedule dirty: true");
            }

            if no_tts || result.text.trim().is_empty() {
                continue;
            }

            let audio = synthesize_with_timeout(
                self.tts.as_ref(),
                &result.text,
                &self.tts_voice_id,
                self.tts_speed,
                Duration::from_secs(120),
            )
            .await?;

            let clip =
                AudioClip::from_wav_bytes(&audio).map_err(|e| AudioError::Play(e.to_string()))?;
            play_with_timeout(&clip, Duration::from_secs(120)).await?;
        }
    }
}

fn build_stt(config: &SttConfig) -> Result<Box<dyn SpeechToText>, AudioError> {
    match config.backend.as_str() {
        "funasr" => {
            let mode = match config.mode.as_str() {
                "2pass" => FunASRMode::TwoPass,
                _ => FunASRMode::Offline,
            };
            let hotwords = if config.hotwords.is_empty() {
                default_hotwords()
                    .get(&config.language)
                    .cloned()
                    .unwrap_or_default()
            } else {
                config.hotwords.clone()
            };
            let client = FunASRClient::new(FunASRConfig {
                url: config.url.clone(),
                language: config.language.clone(),
                hotwords,
                mode,
            });
            Ok(Box::new(client))
        }
        other => Err(AudioError::UnsupportedBackend(other.to_string())),
    }
}

type TtsBuildResult = Result<(Box<dyn TextToSpeech>, String, Option<f32>), AudioError>;

fn build_tts(config: &TtsConfig) -> TtsBuildResult {
    match config.backend.as_str() {
        "cartesia" => {
            let api_key = if config.api_key.is_empty() {
                std::env::var(&config.api_key_env).unwrap_or_default()
            } else {
                config.api_key.clone()
            };
            if api_key.is_empty() {
                return Err(AudioError::Tts("missing Cartesia API key".to_string()));
            }
            let mut tts_config = CartesiaSonicConfig::new(api_key);
            tts_config.voice_id = config.voice_id.clone();
            tts_config.language = Some(config.language.clone());
            tts_config.output_format.sample_rate = config.sample_rate;
            let voice_id = config.voice_id.clone();
            let speed = config.speed;
            Ok((Box::new(CartesiaSonic::new(tts_config)), voice_id, speed))
        }
        other => Err(AudioError::UnsupportedBackend(other.to_string())),
    }
}

async fn record_with_timeout(timeout: Duration) -> Result<Vec<f32>, AudioError> {
    let samples = tokio::task::spawn_blocking(move || {
        let config = RecordConfig {
            max_duration: timeout,
        };
        record(&config)
    })
    .await
    .map_err(|e| AudioError::Record(format!("record task failed: {e}")))?
    .map_err(|e| AudioError::Record(e.to_string()))?;

    Ok(samples)
}

async fn transcribe_with_timeout(
    stt: &dyn SpeechToText,
    samples: &[f32],
    timeout: Duration,
) -> Result<String, AudioError> {
    let text = tokio::time::timeout(timeout, stt.transcribe(samples))
        .await
        .map_err(|_| AudioError::Timeout)?
        .map_err(|e| AudioError::Transcribe(e.to_string()))?;
    Ok(text)
}

async fn synthesize_with_timeout(
    tts: &dyn TextToSpeech,
    text: &str,
    voice_id: &str,
    speed: Option<f32>,
    timeout: Duration,
) -> Result<Vec<u8>, AudioError> {
    let request = TtsRequest {
        text: text.to_string(),
        voice: Some(voice_id.to_string()),
        reference_audio_path: None,
        options: TtsOptions {
            response_format: Some("wav".to_string()),
            speed,
        },
    };

    let audio = tokio::time::timeout(timeout, tts.synthesize(&request))
        .await
        .map_err(|_| AudioError::Timeout)?
        .map_err(|e| AudioError::Tts(e.to_string()))?;
    Ok(audio)
}

async fn play_with_timeout(clip: &AudioClip, timeout: Duration) -> Result<(), AudioError> {
    // Playback is synchronous; run it on a blocking thread so it does not starve the runtime.
    let clip = clip.clone();
    tokio::time::timeout(
        timeout,
        tokio::task::spawn_blocking(move || takusu_audio::play::play(&clip)),
    )
    .await
    .map_err(|_| AudioError::Timeout)?
    .map_err(|e| AudioError::Play(format!("playback task failed: {e}")))?
    .map_err(Into::into)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stt_config_defaults_to_funasr() {
        let config = SttConfig::default();
        assert_eq!(config.backend, "funasr");
        assert_eq!(config.url, "ws://127.0.0.1:10095");
        assert_eq!(config.language, "ja");
    }

    #[test]
    fn tts_config_defaults_to_cartesia() {
        let config = TtsConfig::default();
        assert_eq!(config.backend, "cartesia");
        assert_eq!(config.api_key_env, "CARTESIA_API_KEY");
        assert_eq!(config.sample_rate, 44100);
    }

    #[test]
    fn build_stt_rejects_unknown_backend() {
        let config = SttConfig {
            backend: "unknown".to_string(),
            ..SttConfig::default()
        };
        assert!(build_stt(&config).is_err());
    }

    #[test]
    fn build_tts_rejects_unknown_backend() {
        let config = TtsConfig {
            backend: "unknown".to_string(),
            ..TtsConfig::default()
        };
        assert!(build_tts(&config).is_err());
    }
}
