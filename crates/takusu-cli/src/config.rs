use serde::Deserialize;
use std::path::PathBuf;

#[derive(Debug, Deserialize, Default, Clone)]
pub struct CliConfig {
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub token: Option<String>,
    #[serde(default)]
    pub tz: Option<String>,
    #[serde(default)]
    pub sleep_start: Option<String>,
    #[serde(default)]
    pub sleep_end: Option<String>,
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
# sleep_start = "22:00"
# sleep_end = "06:00"
"#;
    match std::fs::write(&path, default_config) {
        Ok(()) => println!("Created: {}", path.display()),
        Err(e) => eprintln!("Error creating config: {e}"),
    }
}

pub fn set(key: &str, value: &str) -> Result<(), String> {
    let path = config_path();
    let mut config = if path.exists() {
        let content = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
        toml::from_str::<CliConfig>(&content).unwrap_or_default()
    } else {
        CliConfig::default()
    };

    match key {
        "tz" => config.tz = Some(value.to_string()),
        "sleep_start" => config.sleep_start = Some(value.to_string()),
        "sleep_end" => config.sleep_end = Some(value.to_string()),
        "url" => config.url = Some(value.to_string()),
        "token" => config.token = Some(value.to_string()),
        _ => return Err(format!("unknown key: {key}")),
    }

    write_config(&config)
}

fn write_config(config: &CliConfig) -> Result<(), String> {
    let path = config_path();
    let mut lines = Vec::new();

    if let Some(ref v) = config.url {
        lines.push(format!("url = \"{v}\""));
    }
    if let Some(ref v) = config.token {
        lines.push(format!("token = \"{v}\""));
    }
    if let Some(ref v) = config.tz {
        lines.push(format!("tz = \"{v}\""));
    }
    if let Some(ref v) = config.sleep_start {
        lines.push(format!("sleep_start = \"{v}\""));
    }
    if let Some(ref v) = config.sleep_end {
        lines.push(format!("sleep_end = \"{v}\""));
    }

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).ok();
    }
    std::fs::write(&path, lines.join("\n") + "\n").map_err(|e| e.to_string())
}
