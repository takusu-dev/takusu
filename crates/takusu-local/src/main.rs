use std::sync::Arc;

use takusu_local::config::LocalConfig;
use takusu_local::router::router;
use takusu_local::state::AppState;
use takusu_local::storage_sqlite::SqliteStorage;
use takusu_local::storage_workers::WorkersStorage;
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
    let root_token = LocalConfig::load_root_token();

    let storage: Arc<dyn Storage> = match cfg.storage_kind() {
        takusu_local::config::StorageKind::Sqlite => {
            tracing::info!("storage backend: sqlite ({})", cfg.db_url());
            Arc::new(SqliteStorage::init(&cfg, root_token.clone()).await?)
        }
        takusu_local::config::StorageKind::Workers => {
            // `TAKUSU_WORKERS_URL` is read directly because the config crate
            // splits env var names on `_` (so `TAKUSU_WORKERS_URL` would
            // nest as `workers.url`). Reading the env var directly keeps
            // the natural name available to users.
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

    let state = AppState::new(storage, root_token, cfg);
    let bind_addr = state.config.bind_addr().to_string();
    let app = router(state);

    let listener = tokio::net::TcpListener::bind(&bind_addr).await?;
    tracing::info!("listening on {bind_addr}");

    axum::serve(listener, app).await?;

    Ok(())
}
