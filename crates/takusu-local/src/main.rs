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

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let _guard = takusu_local_lib::sentry::init("takusu_local=info", sentry::release_name!());

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;

    runtime.block_on(async {
        let mut cfg = LocalConfig::default();
        if let Ok(v) = std::env::var("TAKUSU_DB") && !v.is_empty() {
            cfg.db = v;
        }
        if let Ok(v) = std::env::var("TAKUSU_BIND") && !v.is_empty() {
            cfg.bind = v;
        }
        if let Ok(v) = std::env::var("TAKUSU_STORAGE") && !v.is_empty() {
            cfg.storage = v;
        }
        if let Ok(v) = std::env::var("TAKUSU_WORKERS_URL") && !v.is_empty() {
            cfg.worker_url = v;
        } else if let Ok(v) = std::env::var("TAKUSU_WORKER_URL") && !v.is_empty() {
            cfg.worker_url = v;
        }

        let env_root = std::env::var("TAKUSU_ROOT_TOKEN")
            .ok()
            .filter(|s| !s.is_empty());

        let storage: Arc<dyn Storage> = match cfg.storage_kind() {
            #[cfg(feature = "sqlite")]
            StorageKind::Sqlite => {
                let root_token = env_root.clone().ok_or("TAKUSU_ROOT_TOKEN is required for the sqlite backend")?;
                tracing::info!("storage backend: sqlite ({})", cfg.db_url());
                Arc::new(SqliteStorage::init(&cfg, root_token).await?)
            }
            #[allow(unreachable_patterns)]
            _ => {
                let url = std::env::var("TAKUSU_WORKERS_URL")
                    .ok()
                    .filter(|s| !s.is_empty())
                    .or_else(|| cfg.workers_url().split('|').next().map(|s| s.to_string()))
                    .unwrap_or_default();
                let token = std::env::var("TAKUSU_WORKERS_TOKEN")
                    .ok()
                    .filter(|s| !s.is_empty())
                    .or_else(|| env_root.clone())
                    .unwrap_or_default();
                if url.is_empty() {
                    return Err("TAKUSU_WORKERS_URL is required for the workers backend".into());
                }
                if token.is_empty() {
                    return Err("TAKUSU_WORKERS_TOKEN (or TAKUSU_ROOT_TOKEN) is required for the workers backend".into());
                }
                tracing::info!("storage backend: workers ({url})");
                Arc::new(WorkersStorage::new_with(url, token))
            }
        };

        let root_token = env_root.unwrap_or_default();
        let token_cache = Arc::new(TokenCache::with_default_ttl());
        let app = Arc::new(TakusuApp::new(storage, root_token, token_cache));
        let state = AppState::new(app);
        let bind_addr = cfg.bind_addr().to_string();
        let app_router = router(state);

        let listener = tokio::net::TcpListener::bind(&bind_addr).await?;
        tracing::info!("listening on {bind_addr}");

        axum::serve(listener, app_router).await?;

        Ok(())
    })
}
