use std::sync::Arc;

use takusu_local_lib::app::TakusuApp;
use tokio::sync::RwLock;

#[derive(Clone)]
pub struct AppState {
    pub app: Arc<TakusuApp>,
    /// Root token configured for this server. Requests bearing this token are
    /// treated as root even if the storage backend cannot verify them, which
    /// lets runtime worker credential updates succeed when the current worker
    /// is unreachable or the token is intended for a new worker.
    pub root_token: Arc<RwLock<String>>,
}

impl AppState {
    pub fn new(app: Arc<TakusuApp>, root_token: String) -> Self {
        Self {
            app,
            root_token: Arc::new(RwLock::new(root_token)),
        }
    }
}
