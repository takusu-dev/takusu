pub mod models;
pub mod record;
pub mod stt;
pub mod tts;

#[cfg(feature = "funasr")]
pub mod funasr;

#[cfg(feature = "hush")]
pub mod hush;

#[cfg(feature = "sherpa")]
pub mod sherpa;

#[cfg(feature = "hush")]
pub use hush::Hush;
#[cfg(feature = "sherpa")]
pub use sherpa::{SherpaOnnxAsr, SherpaOnnxAsrConfig, SherpaOnnxModel};

#[cfg(feature = "funasr")]
pub use funasr::{FunASRClient, FunASRConfig, FunASRError, FunASRMode, default_hotwords};
pub use models::{ModelCache, ModelError, ModelRegistry, ModelSpec};
pub use record::{RecordConfig, RecorderError, record};
pub use stt::{SpeechToText, SttError};
pub use tts::{TextToSpeech, TtsBackend, TtsConfig, TtsError, TtsOptions, TtsRequest};
