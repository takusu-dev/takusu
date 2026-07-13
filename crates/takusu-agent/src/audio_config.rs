use serde::Deserialize;

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
    #[serde(default)]
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
