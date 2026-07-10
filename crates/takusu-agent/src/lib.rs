pub mod tool;
pub mod tools;

pub use tool::ToolRegistry;

use serde::Deserialize;
use std::path::PathBuf;

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct AgentConfig {
    pub llm: LlmConfig,
    pub server: ServerConfig,
    pub audio: AudioConfig,
    pub skills: SkillsConfig,
}

impl AgentConfig {
    /// Load from `$XDG_CONFIG_HOME/takusu/agent.toml` and override with
    /// `TAKUSU_AGENT__<SECTION>__<KEY>` environment variables (e.g. `TAKUSU_AGENT__LLM__BASE_URL`).
    pub fn load() -> Result<Self, config::ConfigError> {
        let mut builder = config::Config::builder();

        if let Some(dir) = config_dir() {
            let path = dir.join("takusu/agent.toml");
            if path.exists() {
                builder =
                    builder.add_source(config::File::from(path).format(config::FileFormat::Toml));
            }
        }

        let cfg = builder
            .add_source(
                config::Environment::with_prefix("TAKUSU_AGENT")
                    .separator("__")
                    .try_parsing(true),
            )
            .build()?
            .try_deserialize()?;

        Ok(cfg)
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct LlmConfig {
    #[serde(default = "default_llm_base_url")]
    pub base_url: String,
    #[serde(default = "default_llm_model")]
    pub model: String,
    #[serde(default = "default_llm_api_key_env")]
    pub api_key_env: String,
}

impl Default for LlmConfig {
    fn default() -> Self {
        Self {
            base_url: default_llm_base_url(),
            model: default_llm_model(),
            api_key_env: default_llm_api_key_env(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct ServerConfig {
    #[serde(default = "default_server_url")]
    pub url: String,
    #[serde(default)]
    pub token: String,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            url: default_server_url(),
            token: String::new(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct AudioConfig {
    #[serde(default = "default_funasr_url")]
    pub funasr_url: String,
    #[serde(default = "default_tts_url")]
    pub tts_url: String,
    #[serde(default = "default_refs_dir")]
    pub refs_dir: String,
}

impl Default for AudioConfig {
    fn default() -> Self {
        Self {
            funasr_url: default_funasr_url(),
            tts_url: default_tts_url(),
            refs_dir: default_refs_dir(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct SkillsConfig {
    #[serde(default = "default_skills_dir")]
    pub dir: String,
}

impl Default for SkillsConfig {
    fn default() -> Self {
        Self {
            dir: default_skills_dir(),
        }
    }
}

fn config_dir() -> Option<PathBuf> {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME").map(|h| {
                let mut p = PathBuf::from(h);
                p.push(".config");
                p
            })
        })
}

fn data_dir() -> Option<PathBuf> {
    std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME").map(|h| {
                let mut p = PathBuf::from(h);
                p.push(".local/share");
                p
            })
        })
}

fn default_llm_base_url() -> String {
    "https://api.openai.com/v1".into()
}

fn default_llm_model() -> String {
    "gpt-4.1-mini".into()
}

fn default_llm_api_key_env() -> String {
    "TAKUSU_LLM_API_KEY".into()
}

fn default_server_url() -> String {
    "http://127.0.0.1:3000".into()
}

fn default_funasr_url() -> String {
    "ws://127.0.0.1:10095".into()
}

fn default_tts_url() -> String {
    "http://127.0.0.1:8088".into()
}

fn default_refs_dir() -> String {
    "./refs".into()
}

fn default_skills_dir() -> String {
    data_dir()
        .map(|d| d.join("takusu/skills").to_string_lossy().into_owned())
        .unwrap_or_else(|| "~/.local/share/takusu/skills".into())
}
