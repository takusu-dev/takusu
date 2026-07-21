use std::sync::Arc;

use tokio::sync::RwLock;

use takusu_local_lib::app::TakusuApp;

#[derive(Clone)]
pub struct AppState {
    pub app: Arc<TakusuApp>,
    pub token: Arc<RwLock<Arc<str>>>,
}

impl AppState {
    pub fn new(app: Arc<TakusuApp>, token: impl AsRef<str>) -> Self {
        Self::new_with_token(app, Arc::new(RwLock::new(Arc::from(token.as_ref()))))
    }

    pub fn new_with_token(app: Arc<TakusuApp>, token: Arc<RwLock<Arc<str>>>) -> Self {
        Self { app, token }
    }
}
