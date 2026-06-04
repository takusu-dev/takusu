use serde::Deserialize;
use std::path::PathBuf;

#[derive(Debug, Deserialize, Default)]
pub struct CliConfig {
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub token: Option<String>,
    #[serde(default)]
    pub tz: Option<String>,
}

pub fn config_path() -> PathBuf {
    let base = if let Ok(dir) = std::env::var("XDG_CONFIG_HOME") {
        PathBuf::from(dir)
    } else {
        let home = std::env::var("HOME").unwrap_or_default();
        PathBuf::from(home).join(".config")
    };
    base.join("takusu").join("config.toml")
}

pub fn load() -> CliConfig {
    let path = config_path();
    if !path.exists() {
        return CliConfig::default();
    }
    let content = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(_) => return CliConfig::default(),
    };
    toml::from_str(&content).unwrap_or_default()
}

pub fn show() {
    let path = config_path();
    println!("Config file: {}", path.display());
    if path.exists() {
        match std::fs::read_to_string(&path) {
            Ok(content) => println!("{}", content.trim()),
            Err(e) => println!("  (error reading: {e})"),
        }
    } else {
        println!("  (not found)");
    }
}

pub fn init() {
    let path = config_path();
    if path.exists() {
        println!("Config file already exists: {}", path.display());
        return;
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).ok();
    }
    let default_config = r#"# takusu CLI configuration
# url = "http://127.0.0.1:3000"
# token = "tsk_xxx"
# tz = "Asia/Tokyo"
"#;
    match std::fs::write(&path, default_config) {
        Ok(()) => println!("Created: {}", path.display()),
        Err(e) => eprintln!("Error creating config: {e}"),
    }
}
