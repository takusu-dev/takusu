use serde::Deserialize;

#[derive(Debug, Clone, Default, Deserialize)]
pub struct LocalConfig {
    #[serde(default = "default_db_url")]
    pub db: String,
    #[serde(default = "default_bind_addr")]
    pub bind: String,
    #[serde(default = "default_worker_url")]
    pub worker_url: String,
    #[serde(default = "default_storage")]
    pub storage: String,
}

fn default_db_url() -> String {
    "sqlite:./takusu.db".into()
}

fn default_bind_addr() -> String {
    "127.0.0.1:3000".into()
}

fn default_worker_url() -> String {
    String::new()
}

fn default_storage() -> String {
    "sqlite".into()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StorageKind {
    Sqlite,
    Workers,
}

impl LocalConfig {
    pub fn storage_kind(&self) -> StorageKind {
        match self.storage.to_ascii_lowercase().as_str() {
            "workers" | "cloudflare" | "d1" => StorageKind::Workers,
            _ => StorageKind::Sqlite,
        }
    }

    pub fn db_url(&self) -> &str {
        &self.db
    }

    pub fn bind_addr(&self) -> &str {
        &self.bind
    }

    pub fn workers_url(&self) -> &str {
        &self.worker_url
    }
}
