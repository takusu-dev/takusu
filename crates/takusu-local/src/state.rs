use std::sync::Arc;

use takusu_local_lib::app::TakusuApp;

#[derive(Clone)]
pub struct AppState {
    pub app: Arc<TakusuApp>,
    pub root_token: String,
}

impl AppState {
    pub fn new(app: Arc<TakusuApp>, root_token: impl Into<String>) -> Self {
        Self {
            app,
            root_token: root_token.into(),
        }
    }
}
