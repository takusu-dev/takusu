use std::sync::Arc;

use takusu_local_lib::app::TakusuApp;

#[derive(Clone)]
pub struct AppState {
    pub app: Arc<TakusuApp>,
}

impl AppState {
    pub fn new(app: Arc<TakusuApp>) -> Self {
        Self { app }
    }
}
