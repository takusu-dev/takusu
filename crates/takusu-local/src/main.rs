use std::sync::Arc;

use takusu_local::router::router;
use takusu_local::state::AppState;
use takusu_local_lib::app::TakusuApp;
use takusu_local_lib::config::LocalConfig;
use takusu_local_lib::config::StorageKind;
#[cfg(feature = "sqlite")]
use takusu_local_lib::storage_sqlite::SqliteStorage;
use takusu_local_lib::storage_workers::WorkersStorage;
use takusu_local_lib::token_cache::TokenCache;
use takusu_storage::Storage;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("takusu_local=info".parse()?),
        )
        .init();

    let cfg = LocalConfig::load()?;
    let root_token = LocalConfig::load_root_token()?;

    let storage: Arc<dyn Storage> = match cfg.storage_kind() {
        #[cfg(feature = "sqlite")]
        StorageKind::Sqlite => {
            tracing::info!("storage backend: sqlite ({})", cfg.db_url());
            Arc::new(SqliteStorage::init(&cfg, root_token.clone()).await?)
        }
        #[allow(unreachable_patterns)]
        _ => {
            let url = std::env::var("TAKUSU_WORKERS_URL")
                .ok()
                .filter(|s| !s.is_empty())
                .or_else(|| cfg.workers_url().split('|').next().map(|s| s.to_string()))
                .unwrap_or_default();
            let token = std::env::var("TAKUSU_WORKERS_TOKEN")
                .or_else(|_| std::env::var("TAKUSU_ROOT_TOKEN"))?;
            if url.is_empty() {
                return Err("TAKUSU_WORKERS_URL is required for the workers backend".into());
            }
            tracing::info!("storage backend: workers ({url})");
            Arc::new(WorkersStorage::new_with(url, token))
        }
    };

    let token_cache = Arc::new(TokenCache::with_default_ttl());
    let app = Arc::new(TakusuApp::new(storage, root_token, token_cache));
    let state = AppState::new(app);
    let bind_addr = cfg.bind_addr().to_string();
    let app_router = router(state);

    let listener = tokio::net::TcpListener::bind(&bind_addr).await?;
    tracing::info!("listening on {bind_addr}");

    axum::serve(listener, app_router).await?;

    Ok(())
}
