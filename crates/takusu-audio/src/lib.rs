pub mod funasr;
pub mod record;
pub mod stt;
pub mod tts;

pub use funasr::{FunASRClient, FunASRConfig, FunASRError, FunASRMode, default_hotwords};
pub use record::{RecordConfig, RecorderError, record};
pub use stt::{SpeechToText, SttError};
pub use tts::{
    TextToSpeech, TtsBackend, TtsClient, TtsConfig, TtsError, TtsOptions, TtsRequest,
    pick_reference_voice,
};
