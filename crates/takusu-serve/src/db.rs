use sqlx::SqlitePool;
use sqlx::sqlite::SqlitePoolOptions;

pub async fn init_pool(db_url: &str) -> Result<SqlitePool, sqlx::Error> {
    let db_url = ensure_create_mode(db_url);

    if let Some(path) = extract_db_path(&db_url) {
        if let Some(parent) = std::path::Path::new(&path).parent() {
            std::fs::create_dir_all(parent).ok();
        }
    }

    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect(&db_url)
        .await?;
    Ok(pool)
}

fn ensure_create_mode(db_url: &str) -> String {
    if !db_url.contains("mode=") {
        let separator = if db_url.contains('?') { '&' } else { '?' };
        format!("{db_url}{separator}mode=rwc")
    } else {
        db_url.to_string()
    }
}

fn extract_db_path(db_url: &str) -> Option<String> {
    let path = db_url.strip_prefix("sqlite:")?;
    if path.is_empty() || path.starts_with(':') {
        return None;
    }
    let path = path.split('?').next().unwrap();
    Some(path.to_string())
}

pub async fn run_migrations(pool: &SqlitePool) -> Result<(), Box<dyn std::error::Error>> {
    let sql = include_str!("../migrations/001_init.sql");
    sqlx::raw_sql(sql).execute(pool).await?;
    let sql = include_str!("../migrations/002_google_cal.sql");
    sqlx::raw_sql(sql).execute(pool).await?;
    let sql = include_str!("../migrations/003_settings.sql");
    sqlx::raw_sql(sql).execute(pool).await?;
    Ok(())
}
