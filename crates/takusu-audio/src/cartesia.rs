//! Cartesia Sonic streaming text-to-speech backend.
//!
//! Uses Cartesia's `POST /tts/bytes` endpoint to stream audio for a complete
//! transcript. The response body is a stream of raw bytes (e.g. WAV) that is
//! exposed as a [`TtsStream`](crate::tts::TtsStream).

use futures_util::TryStreamExt;
use serde::{Deserialize, Serialize};

use crate::tts::{TextToSpeech, TtsError, TtsRequest, TtsStream};

const DEFAULT_URL: &str = "https://api.cartesia.ai/tts/bytes";
const DEFAULT_VERSION: &str = "2026-03-01";
const DEFAULT_MODEL_ID: &str = "sonic-3.5";
const DEFAULT_VOICE_ID: &str = "db6b0ed5-d5d3-463d-ae85-518a07d3c2b4";

/// Audio output format for Cartesia Sonic.
#[derive(Debug, Clone, Serialize)]
pub struct CartesiaOutputFormat {
    pub container: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub encoding: String,
    pub sample_rate: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bit_rate: Option<u32>,
}

impl Default for CartesiaOutputFormat {
    fn default() -> Self {
        Self {
            container: "wav".to_string(),
            encoding: "pcm_s16le".to_string(),
            sample_rate: 44100,
            bit_rate: None,
        }
    }
}

impl CartesiaOutputFormat {
    /// Raw PCM output.
    pub fn raw(encoding: impl Into<String>, sample_rate: u32) -> Self {
        Self {
            container: "raw".to_string(),
            encoding: encoding.into(),
            sample_rate,
            bit_rate: None,
        }
    }

    /// WAV output.
    pub fn wav(encoding: impl Into<String>, sample_rate: u32) -> Self {
        Self {
            container: "wav".to_string(),
            encoding: encoding.into(),
            sample_rate,
            bit_rate: None,
        }
    }

    /// MP3 output.
    pub fn mp3(sample_rate: u32, bit_rate: u32) -> Self {
        Self {
            container: "mp3".to_string(),
            encoding: String::new(),
            sample_rate,
            bit_rate: Some(bit_rate),
        }
    }
}

/// Generation configuration (speed, volume, emotion).
#[derive(Debug, Clone, Default, Serialize)]
pub struct CartesiaGenerationConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub volume: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub speed: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub emotion: Option<String>,
}

/// Configuration for the Cartesia Sonic TTS backend.
#[derive(Debug, Clone)]
pub struct CartesiaSonicConfig {
    pub api_key: String,
    pub url: String,
    pub version: String,
    pub model_id: String,
    pub voice_id: String,
    pub language: Option<String>,
    pub output_format: CartesiaOutputFormat,
    pub generation_config: Option<CartesiaGenerationConfig>,
}

impl Default for CartesiaSonicConfig {
    fn default() -> Self {
        Self {
            api_key: String::new(),
            url: DEFAULT_URL.to_string(),
            version: DEFAULT_VERSION.to_string(),
            model_id: DEFAULT_MODEL_ID.to_string(),
            voice_id: DEFAULT_VOICE_ID.to_string(),
            language: None,
            output_format: CartesiaOutputFormat::default(),
            generation_config: None,
        }
    }
}

impl CartesiaSonicConfig {
    /// Create a config with the given API key and otherwise default settings.
    pub fn new(api_key: impl Into<String>) -> Self {
        Self {
            api_key: api_key.into(),
            ..Self::default()
        }
    }

    /// Create a config from the environment, reading `CARTESIA_API_KEY`.
    pub fn from_env() -> Result<Self, TtsError> {
        let api_key = std::env::var("CARTESIA_API_KEY").map_err(|_| TtsError::Api {
            status: 401,
            message: "CARTESIA_API_KEY environment variable not set".to_string(),
        })?;
        Ok(Self::new(api_key))
    }
}

/// Cartesia Sonic TTS client.
#[derive(Debug, Clone)]
pub struct CartesiaSonic {
    client: reqwest::Client,
    config: CartesiaSonicConfig,
}

impl CartesiaSonic {
    /// Create a new client from the given config.
    pub fn new(config: CartesiaSonicConfig) -> Self {
        #[cfg(target_os = "android")]
        let client = {
            let certs: Vec<reqwest::Certificate> = webpki_root_certs::TLS_SERVER_ROOT_CERTS
                .iter()
                .filter_map(|c| reqwest::Certificate::from_der(c.as_ref()).ok())
                .collect();
            assert!(
                !certs.is_empty(),
                "no bundled root certificates were loaded; Cartesia HTTPS cannot be used"
            );
            reqwest::Client::builder()
                .use_rustls_tls()
                .tls_certs_only(certs)
                // Bind to the IPv4 unspecified address so reqwest prefers IPv4
                // when resolving dual-stack hosts. Some Android networks return
                // unusable IPv6 records for api.cartesia.ai and reqwest can fail
                // to fall back to IPv4, surfacing as "error sending request".
                .local_address(std::net::IpAddr::V4(std::net::Ipv4Addr::UNSPECIFIED))
                .build()
                .expect("failed to build Cartesia HTTP client")
        };
        #[cfg(not(target_os = "android"))]
        let client = reqwest::Client::new();
        Self { client, config }
    }

    /// Create a new client from the environment, reading `CARTESIA_API_KEY`.
    pub fn from_env() -> Result<Self, TtsError> {
        Ok(Self::new(CartesiaSonicConfig::from_env()?))
    }
}

#[async_trait::async_trait]
impl TextToSpeech for CartesiaSonic {
    async fn synthesize_stream(&self, request: &TtsRequest) -> Result<TtsStream, TtsError> {
        if self.config.api_key.is_empty() {
            return Err(TtsError::Api {
                status: 401,
                message: "missing Cartesia API key".to_string(),
            });
        }

        let voice_id = request.voice.as_deref().unwrap_or(&self.config.voice_id);
        let output_format = output_format_for_request(&self.config.output_format, request);

        let mut generation_config = self.config.generation_config.clone();
        if let Some(speed) = request.options.speed {
            let mut gc = generation_config.unwrap_or_default();
            gc.speed = Some(speed);
            generation_config = Some(gc);
        }

        let body = TtsBytesRequest {
            model_id: &self.config.model_id,
            transcript: &request.text,
            voice: VoiceSpecifier {
                mode: "id",
                id: voice_id,
            },
            output_format: &output_format,
            language: self.config.language.as_deref(),
            generation_config: generation_config.as_ref(),
            pronunciation_dict_id: None,
        };

        let json = serde_json::to_vec(&body)?;
        let response = self
            .client
            .post(&self.config.url)
            .header("Authorization", format!("Bearer {}", self.config.api_key))
            .header("Cartesia-Version", &self.config.version)
            .header("Content-Type", "application/json")
            .body(json)
            .send()
            .await?;

        let status = response.status();
        if !status.is_success() {
            let body_text = response.text().await.unwrap_or_default();
            let message = parse_error_message(&body_text);
            return Err(TtsError::Api {
                status: status.as_u16(),
                message,
            });
        }

        let stream = response.bytes_stream().map_err(TtsError::Http);
        Ok(Box::pin(stream))
    }
}

#[derive(Debug, Serialize)]
struct VoiceSpecifier<'a> {
    mode: &'a str,
    id: &'a str,
}

#[derive(Debug, Serialize)]
struct TtsBytesRequest<'a> {
    model_id: &'a str,
    transcript: &'a str,
    voice: VoiceSpecifier<'a>,
    output_format: &'a CartesiaOutputFormat,
    #[serde(skip_serializing_if = "Option::is_none")]
    language: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    generation_config: Option<&'a CartesiaGenerationConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pronunciation_dict_id: Option<&'a str>,
}

#[derive(Debug, Deserialize)]
struct CartesiaApiError {
    title: Option<String>,
    message: Option<String>,
}

fn parse_error_message(body: &str) -> String {
    if let Ok(error) = serde_json::from_str::<CartesiaApiError>(body) {
        if let Some(message) = error.message {
            return message;
        }
        if let Some(title) = error.title {
            return title;
        }
    }

    let trimmed = body.trim();
    if trimmed.is_empty() {
        "unknown Cartesia API error".to_string()
    } else {
        trimmed.to_string()
    }
}

fn output_format_for_request(
    config_format: &CartesiaOutputFormat,
    request: &TtsRequest,
) -> CartesiaOutputFormat {
    let Some(response_format) = request.options.response_format.as_deref() else {
        return config_format.clone();
    };

    match response_format.to_lowercase().as_str() {
        "wav" => CartesiaOutputFormat::wav("pcm_s16le", config_format.sample_rate),
        "mp3" => CartesiaOutputFormat::mp3(config_format.sample_rate, 128_000),
        "raw" => CartesiaOutputFormat::raw("pcm_s16le", config_format.sample_rate),
        "pcm_s16le" => CartesiaOutputFormat::raw("pcm_s16le", config_format.sample_rate),
        "pcm_f32le" => CartesiaOutputFormat::raw("pcm_f32le", config_format.sample_rate),
        "pcm_mulaw" => CartesiaOutputFormat::raw("pcm_mulaw", config_format.sample_rate),
        "pcm_alaw" => CartesiaOutputFormat::raw("pcm_alaw", config_format.sample_rate),
        _ => config_format.clone(),
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use crate::tts::{TtsBackend, TtsOptions, TtsRequest};

    use super::*;

    #[test]
    fn tts_backend_parses_cartesia() {
        assert_eq!(
            TtsBackend::from_str("cartesia").unwrap(),
            TtsBackend::Cartesia
        );
        assert_eq!(
            TtsBackend::from_str("CARTESIA").unwrap(),
            TtsBackend::Cartesia
        );
        assert!(TtsBackend::from_str("unknown").is_err());
    }

    #[test]
    fn output_format_respects_response_format() {
        let config = CartesiaOutputFormat::default();
        let request = |format: &str| TtsRequest {
            text: "hello".to_string(),
            options: TtsOptions {
                response_format: Some(format.to_string()),
                ..Default::default()
            },
            ..Default::default()
        };

        let wav = output_format_for_request(&config, &request("wav"));
        assert_eq!(wav.container, "wav");
        assert_eq!(wav.encoding, "pcm_s16le");

        let raw = output_format_for_request(&config, &request("raw"));
        assert_eq!(raw.container, "raw");
        assert_eq!(raw.encoding, "pcm_s16le");

        let mp3 = output_format_for_request(&config, &request("mp3"));
        assert_eq!(mp3.container, "mp3");
        assert_eq!(mp3.bit_rate, Some(128_000));

        let f32le = output_format_for_request(&config, &request("pcm_f32le"));
        assert_eq!(f32le.container, "raw");
        assert_eq!(f32le.encoding, "pcm_f32le");
    }
}
