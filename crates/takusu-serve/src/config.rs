use config::{Config, Environment};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct ServeConfig {
    #[serde(default = "default_db_url")]
    pub db_url: String,
    #[serde(default = "default_bind_addr")]
    pub bind_addr: String,
}

fn default_db_url() -> String {
    "sqlite:./takusu.db".into()
}

fn default_bind_addr() -> String {
    "127.0.0.1:3000".into()
}

pub fn load_config() -> Result<ServeConfig, config::ConfigError> {
    Config::builder()
        .add_source(Environment::with_prefix("TAKUSU").separator("_"))
        .build()?
        .try_deserialize()
}

pub fn load_root_token() -> String {
    std::env::var("TAKUSU_ROOT_TOKEN").expect("TAKUSU_ROOT_TOKEN is required")
}
