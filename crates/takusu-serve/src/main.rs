use takusu_serve::app::AppState;
use takusu_serve::db;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("takusu_serve=info".parse()?),
        )
        .init();

    let db_url = std::env::var("TAKUSU_DB").unwrap_or_else(|_| "sqlite:./takusu.db".to_string());
    let root_token = std::env::var("TAKUSU_ROOT_TOKEN").expect("TAKUSU_ROOT_TOKEN is required");
    let bind_addr = std::env::var("TAKUSU_BIND").unwrap_or_else(|_| "127.0.0.1:3000".to_string());

    let pool = db::init_pool(&db_url).await?;
    db::run_migrations(&pool).await?;

    tracing::info!("database initialized: {db_url}");

    let state = AppState {
        db: pool,
        root_token,
    };

    let app = takusu_serve::app::app(state);

    let listener = tokio::net::TcpListener::bind(&bind_addr).await?;
    tracing::info!("listening on {bind_addr}");

    axum::serve(listener, app).await?;

    Ok(())
}
