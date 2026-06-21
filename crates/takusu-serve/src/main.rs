use takusu_serve::app::AppState;
use takusu_serve::config;
use takusu_serve::db;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("takusu_serve=info".parse()?),
        )
        .init();

    let serve_config = config::load_config()?;
    let root_token = config::load_root_token();

    let pool = db::init_pool(&serve_config.db_url).await?;
    db::run_migrations(&pool).await?;

    tracing::info!("database initialized: {}", serve_config.db_url);

    let state = AppState {
        db: pool,
        root_token,
    };

    let app = takusu_serve::app::app(state);

    let listener = tokio::net::TcpListener::bind(&serve_config.bind_addr).await?;
    tracing::info!("listening on {}", serve_config.bind_addr);

    axum::serve(listener, app).await?;

    Ok(())
}
