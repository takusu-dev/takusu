//! End-to-end test: spin up a mock worker (axum + in-memory SQLite) that
//! mirrors the takusu-worker HTTP API, then point WorkersStorage at it
//! and exercise the full storage trait. This is the integration test for
//! the WorkersStorage client without needing wasm32 or a deployed Worker.

use axum::Json;
use axum::Router;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{delete, get};
use sha2::{Digest, Sha256};
use sqlx::SqlitePool;
use sqlx::sqlite::SqlitePoolOptions;
use takusu_local_lib::storage_workers::WorkersStorage;
use takusu_storage::{
    CreateTask, Storage, TaskQuery, TaskRow, TokenCreateResponse, TokenRow, UpdateTask,
};
use tokio::net::TcpListener;

const ROOT_TOKEN: &str = "tsk_test_root_token_e2e_workers";

#[derive(Clone)]
struct MockState {
    pool: SqlitePool,
    root_token: String,
}

async fn setup_mock_db() -> SqlitePool {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    let sqls: &[&str] = &[
        include_str!("../migrations/001_init.sql"),
        include_str!("../migrations/002_google_cal.sql"),
        include_str!("../migrations/003_settings.sql"),
    ];
    for s in sqls {
        sqlx::raw_sql(*s).execute(&pool).await.unwrap();
    }
    pool
}

fn mock_router(state: MockState) -> Router {
    Router::new()
        .route("/api/auth/verify", get(verify))
        .route("/api/tasks", get(list_tasks).post(create_task))
        .route(
            "/api/tasks/{id}",
            get(get_task)
                .put(replace_task)
                .patch(update_task)
                .delete(delete_task),
        )
        .route("/api/tokens", get(list_tokens).post(create_token))
        .route("/api/tokens/{id}", delete(revoke_token))
        .with_state(state)
}

async fn verify(State(state): State<MockState>, headers: axum::http::HeaderMap) -> StatusCode {
    let token = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "));
    let token = match token {
        Some(t) => t,
        None => return StatusCode::UNAUTHORIZED,
    };
    if token == state.root_token {
        return StatusCode::OK;
    }
    let hash = hash_token(token);
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM tokens WHERE token_hash = ? AND revoked_at IS NULL",
    )
    .bind(&hash)
    .fetch_one(&state.pool)
    .await
    .unwrap_or(0);
    if count > 0 {
        StatusCode::OK
    } else {
        StatusCode::UNAUTHORIZED
    }
}

async fn list_tasks(
    State(state): State<MockState>,
    axum::extract::Query(q): axum::extract::Query<TaskQuery>,
) -> Json<Vec<TaskRow>> {
    let mut sql = String::from(
        "SELECT id, title, description, start_at, end_at, avg_minutes, sigma_minutes, depends, parallelizable, allows_parallel, abandonability, status, habit_id, ical_uid, created_at, updated_at FROM tasks WHERE 1=1",
    );
    if q.status.is_some() {
        sql.push_str(" AND status = ?");
    }
    if q.from.is_some() {
        sql.push_str(" AND end_at >= ?");
    }
    if q.until.is_some() {
        sql.push_str(" AND start_at <= ?");
    }
    if q.habit_id.is_some() {
        sql.push_str(" AND habit_id = ?");
    }
    sql.push_str(" ORDER BY created_at DESC");
    let mut query = sqlx::query_as::<_, TaskRow>(sqlx::AssertSqlSafe(sql.as_str()));
    if let Some(s) = &q.status {
        query = query.bind(s);
    }
    if let Some(f) = &q.from {
        query = query.bind(f);
    }
    if let Some(u) = &q.until {
        query = query.bind(u);
    }
    if let Some(h) = &q.habit_id {
        query = query.bind(h);
    }
    let rows = query.fetch_all(&state.pool).await.unwrap_or_default();
    Json(rows)
}

async fn create_task(
    State(state): State<MockState>,
    Json(body): Json<CreateTask>,
) -> Result<(StatusCode, Json<TaskRow>), StatusCode> {
    let id = uuid::Uuid::now_v7().to_string();
    let depends_json = serde_json::to_string(&body.depends.clone().unwrap_or_default())
        .unwrap_or_else(|_| "[]".to_string());
    let sigma = body.sigma_minutes.unwrap_or(0);
    let parallelizable = body.parallelizable.unwrap_or(false);
    let allows_parallel = body.allows_parallel.unwrap_or(false);
    let abandonability = body.abandonability.unwrap_or(0.5);

    sqlx::query(
        "INSERT INTO tasks (id, title, description, start_at, end_at, avg_minutes, sigma_minutes, depends, parallelizable, allows_parallel, abandonability, status, ical_uid) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 'pending', ?)"
    )
    .bind(&id)
    .bind(&body.title)
    .bind(&body.description)
    .bind(&body.start_at)
    .bind(&body.end_at)
    .bind(body.avg_minutes)
    .bind(sigma)
    .bind(&depends_json)
    .bind(parallelizable)
    .bind(allows_parallel)
    .bind(abandonability)
    .bind(&body.ical_uid)
    .execute(&state.pool)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let row: TaskRow = sqlx::query_as::<_, TaskRow>(
        "SELECT id, title, description, start_at, end_at, avg_minutes, sigma_minutes, depends, parallelizable, allows_parallel, abandonability, status, habit_id, ical_uid, created_at, updated_at FROM tasks WHERE id = ?",
    )
    .bind(&id)
    .fetch_one(&state.pool)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok((StatusCode::CREATED, Json(row)))
}

async fn get_task(
    State(state): State<MockState>,
    Path(id): Path<String>,
) -> Result<Json<TaskRow>, StatusCode> {
    let row: Option<TaskRow> = sqlx::query_as::<_, TaskRow>(
        "SELECT id, title, description, start_at, end_at, avg_minutes, sigma_minutes, depends, parallelizable, allows_parallel, abandonability, status, habit_id, ical_uid, created_at, updated_at FROM tasks WHERE id = ?",
    )
    .bind(&id)
    .fetch_optional(&state.pool)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    row.map(Json).ok_or(StatusCode::NOT_FOUND)
}

async fn update_task(
    State(state): State<MockState>,
    Path(id): Path<String>,
    Json(body): Json<UpdateTask>,
) -> Result<Json<TaskRow>, StatusCode> {
    let validated = [
        "pending",
        "scheduled",
        "in_progress",
        "completed",
        "skipped",
    ];
    if let Some(ref s) = body.status
        && !validated.contains(&s.as_str())
    {
        return Err(StatusCode::BAD_REQUEST);
    }
    let depends_json = body
        .depends
        .as_ref()
        .map(|d| serde_json::to_string(d).unwrap_or_else(|_| "[]".into()));

    let existing: TaskRow = sqlx::query_as::<_, TaskRow>(
        "SELECT id, title, description, start_at, end_at, avg_minutes, sigma_minutes, depends, parallelizable, allows_parallel, abandonability, status, habit_id, ical_uid, created_at, updated_at FROM tasks WHERE id = ?",
    )
    .bind(&id)
    .fetch_one(&state.pool)
    .await
    .map_err(|_| StatusCode::NOT_FOUND)?;
    let final_status = body.status.clone().unwrap_or(existing.status);

    sqlx::query(
        "UPDATE tasks SET title=COALESCE(?1,title), description=COALESCE(?2,description), start_at=COALESCE(?3,start_at), end_at=COALESCE(?4,end_at), avg_minutes=COALESCE(?5,avg_minutes), sigma_minutes=COALESCE(?6,sigma_minutes), depends=COALESCE(?7,depends), parallelizable=COALESCE(?8,parallelizable), allows_parallel=COALESCE(?9,allows_parallel), abandonability=COALESCE(?10,abandonability), status=?11, updated_at=datetime('now') WHERE id = ?12"
    )
    .bind(body.title.as_deref())
    .bind(body.description.as_deref())
    .bind(body.start_at.as_deref())
    .bind(body.end_at.as_deref())
    .bind(body.avg_minutes)
    .bind(body.sigma_minutes)
    .bind(depends_json.as_deref())
    .bind(body.parallelizable)
    .bind(body.allows_parallel)
    .bind(body.abandonability)
    .bind(&final_status)
    .bind(&id)
    .execute(&state.pool)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let row: TaskRow = sqlx::query_as::<_, TaskRow>(
        "SELECT id, title, description, start_at, end_at, avg_minutes, sigma_minutes, depends, parallelizable, allows_parallel, abandonability, status, habit_id, ical_uid, created_at, updated_at FROM tasks WHERE id = ?",
    )
    .bind(&id)
    .fetch_one(&state.pool)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(row))
}

async fn replace_task(
    State(state): State<MockState>,
    Path(id): Path<String>,
    Json(body): Json<CreateTask>,
) -> Result<Json<TaskRow>, StatusCode> {
    let depends_json = serde_json::to_string(&body.depends.clone().unwrap_or_default())
        .unwrap_or_else(|_| "[]".to_string());
    let sigma = body.sigma_minutes.unwrap_or(0);
    let parallelizable = body.parallelizable.unwrap_or(false);
    let allows_parallel = body.allows_parallel.unwrap_or(false);
    let abandonability = body.abandonability.unwrap_or(0.5);

    sqlx::query(
        "UPDATE tasks SET title=?, description=?, start_at=?, end_at=?, avg_minutes=?, sigma_minutes=?, depends=?, parallelizable=?, allows_parallel=?, abandonability=?, updated_at=datetime('now') WHERE id = ?"
    )
    .bind(&body.title)
    .bind(&body.description)
    .bind(&body.start_at)
    .bind(&body.end_at)
    .bind(body.avg_minutes)
    .bind(sigma)
    .bind(&depends_json)
    .bind(parallelizable)
    .bind(allows_parallel)
    .bind(abandonability)
    .bind(&id)
    .execute(&state.pool)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let row: TaskRow = sqlx::query_as::<_, TaskRow>(
        "SELECT id, title, description, start_at, end_at, avg_minutes, sigma_minutes, depends, parallelizable, allows_parallel, abandonability, status, habit_id, ical_uid, created_at, updated_at FROM tasks WHERE id = ?",
    )
    .bind(&id)
    .fetch_one(&state.pool)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(row))
}

async fn delete_task(State(state): State<MockState>, Path(id): Path<String>) -> StatusCode {
    let r = sqlx::query("DELETE FROM tasks WHERE id = ?")
        .bind(&id)
        .execute(&state.pool)
        .await
        .unwrap();
    if r.rows_affected() == 0 {
        StatusCode::NOT_FOUND
    } else {
        StatusCode::NO_CONTENT
    }
}

async fn list_tokens(State(state): State<MockState>) -> Json<Vec<TokenRow>> {
    let rows: Vec<TokenRow> = sqlx::query_as::<_, TokenRow>(
        "SELECT id, token_hash, label, created_by, created_at, revoked_at FROM tokens ORDER BY created_at DESC",
    )
    .fetch_all(&state.pool)
    .await
    .unwrap_or_default();
    Json(rows)
}

async fn create_token(
    State(state): State<MockState>,
    axum::Json(body): axum::Json<serde_json::Value>,
) -> Result<(StatusCode, Json<TokenCreateResponse>), StatusCode> {
    let new_token = format!("tsk_{}", uuid::Uuid::now_v7());
    let hash = hash_token(&new_token);
    let label = body
        .get("label")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    sqlx::query(
        "INSERT INTO tokens (token_hash, label, created_by) VALUES (?, ?, 'authenticated')",
    )
    .bind(&hash)
    .bind(label.as_deref())
    .execute(&state.pool)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let row: TokenRow = sqlx::query_as::<_, TokenRow>(
        "SELECT id, token_hash, label, created_by, created_at, revoked_at FROM tokens WHERE token_hash = ?",
    )
    .bind(&hash)
    .fetch_one(&state.pool)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok((
        StatusCode::CREATED,
        Json(TokenCreateResponse {
            id: row.id,
            token: new_token,
            label: row.label,
            created_at: row.created_at,
        }),
    ))
}

async fn revoke_token(
    State(state): State<MockState>,
    Path(id): Path<String>,
) -> Result<StatusCode, StatusCode> {
    let id_num: i64 = id.parse().map_err(|_| StatusCode::BAD_REQUEST)?;
    let result = sqlx::query(
        "UPDATE tokens SET revoked_at = datetime('now') WHERE id = ? AND revoked_at IS NULL",
    )
    .bind(id_num)
    .execute(&state.pool)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if result.rows_affected() == 0 {
        return Err(StatusCode::NOT_FOUND);
    }
    Ok(StatusCode::NO_CONTENT)
}

fn hash_token(token: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(token.as_bytes());
    let result = hasher.finalize();
    result.iter().map(|b| format!("{b:02x}")).collect()
}

async fn spawn_mock_worker() -> String {
    let pool = setup_mock_db().await;
    let state = MockState {
        pool,
        root_token: ROOT_TOKEN.to_string(),
    };
    let app = mock_router(state);
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.ok();
    });
    format!("http://{addr}")
}

#[tokio::test]
async fn workers_storage_e2e() {
    let base_url = spawn_mock_worker().await;
    let storage = WorkersStorage::new_with(base_url.clone(), ROOT_TOKEN.to_string());

    assert!(storage.verify_token(ROOT_TOKEN).await.unwrap());
    assert!(!storage.verify_token("tsk_bogus").await.unwrap());

    let create_body = CreateTask {
        title: "e2e task".to_string(),
        description: Some("integration test".to_string()),
        start_at: Some("2026-06-05T09:00:00+09:00".to_string()),
        end_at: "2026-06-05T18:00:00+09:00".to_string(),
        avg_minutes: 60,
        sigma_minutes: Some(15),
        depends: Some(vec![]),
        parallelizable: Some(false),
        allows_parallel: Some(false),
        abandonability: Some(0.3),
        ical_uid: None,
    };
    let task = storage.create_task(&create_body).await.unwrap();
    assert_eq!(task.title, "e2e task");
    assert_eq!(task.status, "pending");
    let id = task.id.clone();

    let tasks = storage
        .list_tasks(&TaskQuery {
            status: Some("pending".to_string()),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(tasks.len(), 1);
    assert_eq!(tasks[0].id, id);

    let fetched = storage.get_task(&id).await.unwrap();
    assert_eq!(fetched.id, id);

    let err = storage
        .get_task("00000000-0000-0000-0000-000000000000")
        .await;
    assert!(matches!(
        err,
        Err(takusu_storage::StorageError::NotFound(_))
    ));

    let update_body = UpdateTask {
        title: Some("e2e task updated".to_string()),
        ..Default::default()
    };
    let updated = storage.update_task(&id, &update_body).await.unwrap();
    assert_eq!(updated.title, "e2e task updated");

    storage.delete_task(&id).await.unwrap();
    let after = storage.get_task(&id).await;
    assert!(matches!(
        after,
        Err(takusu_storage::StorageError::NotFound(_))
    ));

    let resp = storage.create_token(Some("e2e")).await.unwrap();
    assert!(resp.token.starts_with("tsk_"));
    let tokens = storage.list_tokens().await.unwrap();
    assert_eq!(tokens.len(), 1);
    storage.revoke_token(resp.id).await.unwrap();
    let tokens_after = storage.list_tokens().await.unwrap();
    assert!(tokens_after[0].revoked_at.is_some());
}
