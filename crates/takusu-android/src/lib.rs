uniffi::setup_scaffolding!();

use std::sync::{Arc, Mutex};

use takusu_local::router::router;
use takusu_local::state::AppState;
use takusu_local_lib::app::TakusuApp;
use takusu_local_lib::storage_workers::WorkersStorage;
use takusu_local_lib::token_cache::TokenCache;
use takusu_storage::Storage;
use tokio::net::TcpListener;

/// Error type for FFI
#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum TakusuError {
    #[error("server already running")]
    AlreadyRunning,
    #[error("server not running")]
    NotRunning,
    #[error("invalid configuration: {detail}")]
    InvalidConfig { detail: String },
    #[error("server error: {detail}")]
    Server { detail: String },
}

#[derive(Debug, Clone, uniffi::Enum)]
pub enum ServerStatus {
    Stopped,
    Running { port: u16 },
}

/// Embedded takusu server for Android.
///
/// Spawns an axum server on localhost that serves the full takusu-local REST API.
/// Storage backend is WorkersStorage (HTTP → Cloudflare Worker).
#[derive(uniffi::Object)]
pub struct TakusuServer {
    runtime: Mutex<Option<tokio::runtime::Runtime>>,
    port: Mutex<u16>,
}

#[uniffi::export]
impl TakusuServer {
    #[uniffi::constructor]
    pub fn new() -> Self {
        Self {
            runtime: Mutex::new(None),
            port: Mutex::new(0),
        }
    }

    /// Start the server on the given port, using the provided Workers URL and token.
    pub fn start(
        &self,
        port: u16,
        workers_url: String,
        root_token: String,
    ) -> Result<(), TakusuError> {
        let mut runtime_guard = self.runtime.lock().map_err(|e| TakusuError::Server {
            detail: format!("lock poisoned: {e}"),
        })?;
        if runtime_guard.is_some() {
            return Err(TakusuError::AlreadyRunning);
        }

        if workers_url.is_empty() {
            return Err(TakusuError::InvalidConfig {
                detail: "workers_url must not be empty".to_string(),
            });
        }
        if root_token.is_empty() {
            return Err(TakusuError::InvalidConfig {
                detail: "root_token must not be empty".to_string(),
            });
        }

        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .map_err(|e| TakusuError::Server {
                detail: format!("failed to create runtime: {e}"),
            })?;

        let storage: Arc<dyn Storage> =
            Arc::new(WorkersStorage::new_with(workers_url, root_token.clone()));
        let token_cache = Arc::new(TokenCache::with_default_ttl());
        let app = Arc::new(TakusuApp::new(storage, root_token, token_cache));
        let state = AppState::new(app);
        let app_router = router(state);

        let bind_addr = format!("127.0.0.1:{port}");
        let listener = runtime
            .block_on(async { TcpListener::bind(&bind_addr).await })
            .map_err(|e| TakusuError::Server {
                detail: format!("failed to bind {bind_addr}: {e}"),
            })?;

        let actual_port = listener.local_addr().map(|a| a.port()).unwrap_or(port);
        *self.port.lock().map_err(|e| TakusuError::Server {
            detail: format!("lock poisoned: {e}"),
        })? = actual_port;

        runtime.spawn(async move {
            if let Err(e) = axum::serve(listener, app_router).await {
                eprintln!("server error: {e}");
            }
        });

        *runtime_guard = Some(runtime);
        Ok(())
    }

    /// Stop the server gracefully.
    pub fn stop(&self) -> Result<(), TakusuError> {
        let mut runtime_guard = self.runtime.lock().map_err(|e| TakusuError::Server {
            detail: format!("lock poisoned: {e}"),
        })?;
        let runtime = runtime_guard.take().ok_or(TakusuError::NotRunning)?;
        if let Ok(mut p) = self.port.lock() {
            *p = 0;
        }
        runtime.shutdown_background();
        Ok(())
    }

    /// Get the current server status.
    pub fn status(&self) -> ServerStatus {
        let runtime_guard = self.runtime.lock();
        let port_guard = self.port.lock();
        match (runtime_guard, port_guard) {
            (Ok(guard), Ok(port)) if guard.is_some() && *port > 0 => {
                ServerStatus::Running { port: *port }
            }
            _ => ServerStatus::Stopped,
        }
    }
}
