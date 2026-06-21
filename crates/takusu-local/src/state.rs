use std::sync::Arc;

use takusu_storage::Storage;

use crate::config::LocalConfig;
use crate::token_cache::TokenCache;

#[derive(Clone)]
pub struct AppState {
    pub storage: Arc<dyn Storage>,
    pub root_token: String,
    pub config: LocalConfig,
    pub token_cache: Arc<TokenCache>,
}

impl AppState {
    pub fn new(storage: Arc<dyn Storage>, root_token: String, config: LocalConfig) -> Self {
        Self {
            storage,
            root_token,
            config,
            token_cache: Arc::new(TokenCache::with_default_ttl()),
        }
    }
}
