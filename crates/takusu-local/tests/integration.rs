use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use http_body_util::BodyExt;
use serde_json::json;
use sqlx::SqlitePool;
use std::sync::Arc;
use takusu_local::router::router as build_router;
use takusu_local::state::AppState;
use takusu_local_lib::app::TakusuApp;
use takusu_local_lib::config::LocalConfig;
use takusu_local_lib::storage_sqlite::SqliteStorage;
use takusu_local_lib::token_cache::TokenCache;
use tower::ServiceExt;

const ROOT_TOKEN: &str = "tsk_test_root_token_0000000000000000000000000001";

async fn setup() -> (AppState, SqlitePool) {
    let cfg = LocalConfig {
        db: "sqlite::memory:".into(),
        ..Default::default()
    };
    let storage = SqliteStorage::init(&cfg, ROOT_TOKEN.to_string())
        .await
        .unwrap();
    let pool = storage.pool().clone();
    let token_cache = Arc::new(TokenCache::with_default_ttl());
    let app = Arc::new(TakusuApp::new(
        Arc::new(storage),
        ROOT_TOKEN.to_string(),
        token_cache,
    ));
    let state = AppState::new(app);
    (state, pool)
}

fn auth_req(method: Method, uri: &str) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(uri)
        .header("authorization", format!("Bearer {ROOT_TOKEN}"))
        .body(Body::empty())
        .unwrap()
}

fn auth_req_body(method: Method, uri: &str, body: serde_json::Value) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(uri)
        .header("authorization", format!("Bearer {ROOT_TOKEN}"))
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap()
}

async fn body_str(body: Body) -> String {
    let bytes = body.collect().await.unwrap().to_bytes();
    String::from_utf8(bytes.to_vec()).unwrap()
}

#[tokio::test]
async fn health_check() {
    let (state, _) = setup().await;
    let app = build_router(state);
    let req = Request::builder()
        .uri("/health")
        .body(Body::empty())
        .unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(body_str(res.into_body()).await, "ok");
}

#[tokio::test]
async fn unauthorized_without_token() {
    let (state, _) = setup().await;
    let app = build_router(state);
    let req = Request::builder()
        .uri("/api/tasks")
        .body(Body::empty())
        .unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn unauthorized_with_wrong_token() {
    let (state, _) = setup().await;
    let app = build_router(state);
    let req = Request::builder()
        .uri("/api/tasks")
        .header("authorization", "Bearer wrong_token")
        .body(Body::empty())
        .unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn authorized_with_root_token() {
    let (state, _) = setup().await;
    let app = build_router(state);
    let req = auth_req(Method::GET, "/api/tasks");
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
}

#[tokio::test]
async fn token_crud() {
    let (state, pool) = setup().await;
    let app = build_router(state);

    let req = auth_req_body(
        Method::POST,
        "/api/tokens",
        json!({ "label": "test-device" }),
    );
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);
    let body: serde_json::Value = serde_json::from_str(&body_str(res.into_body()).await).unwrap();
    let new_token = body["token"].as_str().unwrap();
    assert!(new_token.starts_with("tsk_"));

    let hash = takusu_local_lib::auth::hash_token(new_token);
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM tokens WHERE token_hash = ?")
        .bind(&hash)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 1);

    let (state2, pool2) = setup().await;
    sqlx::query("INSERT INTO tokens (token_hash, label, created_by) VALUES (?, ?, 'root')")
        .bind(&hash)
        .bind("test-device")
        .execute(&pool2)
        .await
        .unwrap();
    let app2 = build_router(state2);
    let req = Request::builder()
        .uri("/api/tasks")
        .header("authorization", format!("Bearer {new_token}"))
        .body(Body::empty())
        .unwrap();
    let res = app2.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let list_req = auth_req(Method::GET, "/api/tokens");
    let res = app2.oneshot(list_req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let list: serde_json::Value = serde_json::from_str(&body_str(res.into_body()).await).unwrap();
    assert_eq!(list.as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn task_create_and_list() {
    let (state, _) = setup().await;
    let app = build_router(state);

    let req = auth_req_body(
        Method::POST,
        "/api/tasks",
        json!({
            "title": "テストタスク",
            "description": "テスト用",
            "start_at": "2026-06-05T09:00:00+09:00",
            "end_at": "2026-06-05T18:00:00+09:00",
            "avg_minutes": 60,
            "sigma_minutes": 15,
            "depends": [],
            "parallelizable": false,
            "allows_parallel": false,
            "abandonability": 0.3
        }),
    );
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);
    let body: serde_json::Value = serde_json::from_str(&body_str(res.into_body()).await).unwrap();
    assert_eq!(body["title"], "テストタスク");
    assert_eq!(body["status"], "pending");

    let list_req = auth_req(Method::GET, "/api/tasks");
    let res = app.oneshot(list_req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let list: serde_json::Value = serde_json::from_str(&body_str(res.into_body()).await).unwrap();
    assert_eq!(list.as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn task_get_update_delete() {
    let (state, _) = setup().await;
    let app = build_router(state);

    let req = auth_req_body(
        Method::POST,
        "/api/tasks",
        json!({
            "title": "original",
            "end_at": "2026-06-05T18:00:00+09:00",
            "avg_minutes": 30,
            "depends": [],
            "parallelizable": false,
            "allows_parallel": false,
            "abandonability": 0.5
        }),
    );
    let res = app.clone().oneshot(req).await.unwrap();
    let body: serde_json::Value = serde_json::from_str(&body_str(res.into_body()).await).unwrap();
    let task_id = body["id"].as_str().unwrap();

    let get_req = auth_req(Method::GET, &format!("/api/tasks/{task_id}"));
    let res = app.clone().oneshot(get_req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let update_req = auth_req_body(
        Method::PATCH,
        &format!("/api/tasks/{task_id}"),
        json!({ "title": "updated" }),
    );
    let res = app.clone().oneshot(update_req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let updated: serde_json::Value =
        serde_json::from_str(&body_str(res.into_body()).await).unwrap();
    assert_eq!(updated["title"], "updated");

    let not_found_req = auth_req(Method::GET, "/api/tasks/nonexistent");
    let res = app.clone().oneshot(not_found_req).await.unwrap();
    assert_eq!(res.status(), StatusCode::NOT_FOUND);

    let delete_req = auth_req(Method::DELETE, &format!("/api/tasks/{task_id}"));
    let res = app.oneshot(delete_req).await.unwrap();
    assert_eq!(res.status(), StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn task_replace() {
    let (state, _) = setup().await;
    let app = build_router(state);

    let req = auth_req_body(
        Method::POST,
        "/api/tasks",
        json!({
            "title": "original",
            "end_at": "2026-06-05T18:00:00+09:00",
            "avg_minutes": 30,
            "depends": [],
            "parallelizable": false,
            "allows_parallel": false,
            "abandonability": 0.5
        }),
    );
    let res = app.clone().oneshot(req).await.unwrap();
    let body: serde_json::Value = serde_json::from_str(&body_str(res.into_body()).await).unwrap();
    let task_id = body["id"].as_str().unwrap();

    let replace_req = auth_req_body(
        Method::PUT,
        &format!("/api/tasks/{task_id}"),
        json!({
            "title": "replaced",
            "end_at": "2026-06-06T12:00:00+09:00",
            "avg_minutes": 45,
            "depends": [],
            "parallelizable": true,
            "allows_parallel": false,
            "abandonability": 0.8
        }),
    );
    let res = app.oneshot(replace_req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let replaced: serde_json::Value =
        serde_json::from_str(&body_str(res.into_body()).await).unwrap();
    assert_eq!(replaced["title"], "replaced");
    assert_eq!(replaced["avg_minutes"], 45);
    assert_eq!(replaced["parallelizable"], true);
}

#[tokio::test]
async fn task_list_filter_by_status() {
    let (state, _) = setup().await;
    let app = build_router(state);

    for i in 0..3 {
        let req = auth_req_body(
            Method::POST,
            "/api/tasks",
            json!({
                "title": format!("task-{i}"),
                "end_at": "2026-06-05T18:00:00+09:00",
                "avg_minutes": 30,
                "depends": [],
                "parallelizable": false,
                "allows_parallel": false,
                "abandonability": 0.5
            }),
        );
        app.clone().oneshot(req).await.unwrap();
    }

    let req = auth_req(Method::GET, "/api/tasks?status=pending");
    let res = app.oneshot(req).await.unwrap();
    let list: serde_json::Value = serde_json::from_str(&body_str(res.into_body()).await).unwrap();
    assert_eq!(list.as_array().unwrap().len(), 3);
}

#[tokio::test]
async fn habit_crud() {
    let (state, _) = setup().await;
    let app = build_router(state);

    let req = auth_req_body(
        Method::POST,
        "/api/habits",
        json!({
            "title": "朝のランニング",
            "description": "30分走る",
            "recurrence": r#"{"freq":"daily","interval":1,"by_day":[],"by_month":[],"by_month_day":[],"count":null,"exdates":[]}"#,
            "start_time": "06:00",
            "end_time": "07:00",
            "avg_minutes": 30,
            "sigma_minutes": 5,
            "parallelizable": false,
            "allows_parallel": false,
            "abandonability": 0.1
        }),
    );
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);
    let body: serde_json::Value = serde_json::from_str(&body_str(res.into_body()).await).unwrap();
    let habit_id = body["id"].as_str().unwrap();
    // create_habit assigns a monotonic display_id (#305).
    let display_id = body["display_id"].as_i64().unwrap();
    assert!(display_id >= 1);

    let get_req = auth_req(Method::GET, &format!("/api/habits/{habit_id}"));
    let res = app.clone().oneshot(get_req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let habit: serde_json::Value = serde_json::from_str(&body_str(res.into_body()).await).unwrap();
    assert_eq!(habit["title"], "朝のランニング");
    assert_eq!(habit["display_id"], display_id);
    assert_eq!(
        habit["recurrence"],
        r#"{"freq":"daily","interval":1,"by_day":[],"by_month":[],"by_month_day":[],"count":null,"exdates":[]}"#
    );

    // Habit can be fetched by `h{display_id}` (#305).
    let h_req = auth_req(Method::GET, &format!("/api/habits/h{display_id}"));
    let res = app.clone().oneshot(h_req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let h_habit: serde_json::Value =
        serde_json::from_str(&body_str(res.into_body()).await).unwrap();
    assert_eq!(h_habit["id"], habit_id);

    let update_req = auth_req_body(
        Method::PATCH,
        &format!("/api/habits/{habit_id}"),
        json!({ "title": "夜のランニング", "active": false }),
    );
    let res = app.clone().oneshot(update_req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let updated: serde_json::Value =
        serde_json::from_str(&body_str(res.into_body()).await).unwrap();
    assert_eq!(updated["title"], "夜のランニング");

    let list_req = auth_req(Method::GET, "/api/habits");
    let res = app.clone().oneshot(list_req).await.unwrap();
    let list: serde_json::Value = serde_json::from_str(&body_str(res.into_body()).await).unwrap();
    assert_eq!(list.as_array().unwrap().len(), 1);

    let delete_req = auth_req(Method::DELETE, &format!("/api/habits/{habit_id}"));
    let res = app.oneshot(delete_req).await.unwrap();
    assert_eq!(res.status(), StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn habit_display_id_is_monotonic() {
    // #305: habit display_id is assigned from a monotonic sequence.
    let (state, _) = setup().await;
    let app = build_router(state);

    let mk = || {
        auth_req_body(
            Method::POST,
            "/api/habits",
            json!({
                "title": "h",
                "recurrence": r#"{"freq":"daily","interval":1,"by_day":[],"by_month":[],"by_month_day":[],"count":null,"exdates":[]}"#,
                "start_time": "06:00",
                "end_time": "07:00",
                "avg_minutes": 30,
            }),
        )
    };

    let r1 = app.clone().oneshot(mk()).await.unwrap();
    let b1: serde_json::Value = serde_json::from_str(&body_str(r1.into_body()).await).unwrap();
    let r2 = app.clone().oneshot(mk()).await.unwrap();
    let b2: serde_json::Value = serde_json::from_str(&body_str(r2.into_body()).await).unwrap();
    let d1 = b1["display_id"].as_i64().unwrap();
    let d2 = b2["display_id"].as_i64().unwrap();
    assert_eq!(d2, d1 + 1, "habit display_id must be monotonic");

    // Fetch the second habit by h{d2}.
    let h_req = auth_req(Method::GET, &format!("/api/habits/h{d2}"));
    let res = app.oneshot(h_req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
}

#[tokio::test]
async fn habit_delete_cascades_to_tasks() {
    // #240: deleting a habit that has already generated tasks must
    // succeed (cascade-delete the associated tasks) instead of failing
    // on the foreign-key constraint.
    let (state, _) = setup().await;
    let app = build_router(state);

    // Create a habit.
    let habit_req = auth_req_body(
        Method::POST,
        "/api/habits",
        json!({
            "title": "朝のランニング",
            "description": "30分走る",
            "recurrence": r#"{"freq":"daily","interval":1,"by_day":[],"by_month":[],"by_month_day":[],"count":null,"exdates":[]}"#,
            "start_time": "06:00",
            "end_time": "07:00",
            "avg_minutes": 30,
            "sigma_minutes": 5,
            "parallelizable": false,
            "allows_parallel": false,
            "abandonability": 0.1
        }),
    );
    let res = app.clone().oneshot(habit_req).await.unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);
    let body: serde_json::Value = serde_json::from_str(&body_str(res.into_body()).await).unwrap();
    let habit_id = body["id"].as_str().unwrap();

    // Create a task referencing the habit.
    let task_req = auth_req_body(
        Method::POST,
        "/api/tasks",
        json!({
            "title": "ランニングタスク",
            "end_at": "2026-07-06T07:00:00Z",
            "avg_minutes": 30,
            "sigma_minutes": 5,
            "depends": [],
            "parallelizable": false,
            "allows_parallel": false,
            "abandonability": 0.1,
            "habit_id": habit_id,
        }),
    );
    let res = app.clone().oneshot(task_req).await.unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);
    let task_body: serde_json::Value =
        serde_json::from_str(&body_str(res.into_body()).await).unwrap();
    let task_id = task_body["id"].as_str().unwrap();

    // Delete the habit — must succeed and cascade-delete the task.
    let delete_req = auth_req(Method::DELETE, &format!("/api/habits/{habit_id}"));
    let res = app.clone().oneshot(delete_req).await.unwrap();
    assert_eq!(res.status(), StatusCode::NO_CONTENT);

    // Habit is gone.
    let get_req = auth_req(Method::GET, &format!("/api/habits/{habit_id}"));
    let res = app.clone().oneshot(get_req).await.unwrap();
    assert_eq!(res.status(), StatusCode::NOT_FOUND);

    // Associated task is also gone.
    let task_get = auth_req(Method::GET, &format!("/api/tasks/{task_id}"));
    let res = app.oneshot(task_get).await.unwrap();
    assert_eq!(res.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn ical_import() {
    let (state, _) = setup().await;
    let app = build_router(state);

    let ical = "BEGIN:VCALENDAR\r\nBEGIN:VEVENT\r\nDTSTART:20260605T090000Z\r\nDTEND:20260605T110000Z\r\nSUMMARY:会議\r\nUID:meeting-001@example.com\r\nEND:VEVENT\r\nBEGIN:VEVENT\r\nDTSTART:20260606T140000Z\r\nDTEND:20260606T150000Z\r\nSUMMARY:レビュー\r\nUID:review-001@example.com\r\nEND:VEVENT\r\nEND:VCALENDAR";

    let req = Request::builder()
        .method(Method::POST)
        .uri("/api/tasks/import/ical")
        .header("authorization", format!("Bearer {ROOT_TOKEN}"))
        .header("content-type", "text/calendar")
        .body(Body::from(ical.to_string()))
        .unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body: serde_json::Value = serde_json::from_str(&body_str(res.into_body()).await).unwrap();
    assert_eq!(body["imported"], 2);
    assert_eq!(body["task_ids"].as_array().unwrap().len(), 2);
}

#[tokio::test]
async fn ical_import_skips_duplicate() {
    let (state, pool) = setup().await;
    sqlx::query("INSERT INTO tasks (id, title, end_at, avg_minutes, sigma_minutes, depends, parallelizable, allows_parallel, abandonability, status, ical_uid) VALUES ('existing', '会議', '2026-06-05T11:00:00Z', 120, 0, '[]', 0, 0, 0.5, 'pending', 'meeting-001@example.com')")
        .execute(&pool)
        .await.unwrap();

    let app = build_router(state);
    let ical = "BEGIN:VCALENDAR\r\nBEGIN:VEVENT\r\nDTSTART:20260605T090000Z\r\nDTEND:20260605T110000Z\r\nSUMMARY:会議\r\nUID:meeting-001@example.com\r\nEND:VEVENT\r\nBEGIN:VEVENT\r\nDTSTART:20260606T140000Z\r\nDTEND:20260606T150000Z\r\nSUMMARY:レビュー\r\nUID:review-001@example.com\r\nEND:VEVENT\r\nEND:VCALENDAR";

    let req = Request::builder()
        .method(Method::POST)
        .uri("/api/tasks/import/ical")
        .header("authorization", format!("Bearer {ROOT_TOKEN}"))
        .header("content-type", "text/calendar")
        .body(Body::from(ical.to_string()))
        .unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body: serde_json::Value = serde_json::from_str(&body_str(res.into_body()).await).unwrap();
    assert_eq!(body["imported"], 1);
}

#[tokio::test]
async fn schedule_generate_and_get() {
    let (state, _) = setup().await;
    let app = build_router(state);

    let create_req = auth_req_body(
        Method::POST,
        "/api/tasks",
        json!({
            "title": "作業A",
            "start_at": "2026-06-05T09:00:00+09:00",
            "end_at": "2026-06-05T18:00:00+09:00",
            "avg_minutes": 60,
            "sigma_minutes": 10,
            "depends": [],
            "parallelizable": false,
            "allows_parallel": false,
            "abandonability": 0.5
        }),
    );
    app.clone().oneshot(create_req).await.unwrap();

    let gen_req = auth_req_body(
        Method::POST,
        "/api/schedule/generate",
        json!({
            "sleep": "disabled"
        }),
    );
    let res = app.clone().oneshot(gen_req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body: serde_json::Value = serde_json::from_str(&body_str(res.into_body()).await).unwrap();
    assert!(body["schedule"].is_string());

    let get_req = auth_req(Method::GET, "/api/schedule");
    let res = app.clone().oneshot(get_req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let clear_req = auth_req(Method::DELETE, "/api/schedule");
    let res = app.clone().oneshot(clear_req).await.unwrap();
    assert_eq!(res.status(), StatusCode::NO_CONTENT);

    let get_req = auth_req(Method::GET, "/api/schedule");
    let res = app.oneshot(get_req).await.unwrap();
    assert_eq!(res.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn schedule_not_found_initially() {
    let (state, _) = setup().await;
    let app = build_router(state);
    let req = auth_req(Method::GET, "/api/schedule");
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn token_revoke() {
    let (state, pool) = setup().await;

    let hash = takusu_local_lib::auth::hash_token("tsk_test_revoke_token");
    sqlx::query(
        "INSERT INTO tokens (token_hash, label, created_by) VALUES (?, 'to-revoke', 'root')",
    )
    .bind(&hash)
    .execute(&pool)
    .await
    .unwrap();

    let token_id: i64 = sqlx::query_scalar("SELECT id FROM tokens WHERE label = 'to-revoke'")
        .fetch_one(&pool)
        .await
        .unwrap();

    let app = build_router(state);
    let revoke_req = auth_req(Method::DELETE, &format!("/api/tokens/{token_id}"));
    let res = app.oneshot(revoke_req).await.unwrap();
    assert_eq!(res.status(), StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn delete_nonexistent_task() {
    let (state, _) = setup().await;
    let app = build_router(state);
    let req = auth_req(Method::DELETE, "/api/tasks/nonexistent-id");
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn task_prefix_lookup() {
    let (state, _) = setup().await;
    let app = build_router(state);

    let req = auth_req_body(
        Method::POST,
        "/api/tasks",
        json!({
            "title": "prefix-test",
            "end_at": "2026-06-05T18:00:00+09:00",
            "avg_minutes": 30,
            "depends": [],
            "parallelizable": false,
            "allows_parallel": false,
            "abandonability": 0.5
        }),
    );
    let res = app.clone().oneshot(req).await.unwrap();
    let body: serde_json::Value = serde_json::from_str(&body_str(res.into_body()).await).unwrap();
    let full_id = body["id"].as_str().unwrap();
    assert!(full_id.contains('-'));
    let short_id = &full_id[..8];

    let get_req = auth_req(Method::GET, &format!("/api/tasks/{short_id}"));
    let res = app.clone().oneshot(get_req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let task: serde_json::Value = serde_json::from_str(&body_str(res.into_body()).await).unwrap();
    assert_eq!(task["id"], full_id);
    assert_eq!(task["title"], "prefix-test");

    let update_req = auth_req_body(
        Method::PATCH,
        &format!("/api/tasks/{short_id}"),
        json!({ "title": "prefix-updated" }),
    );
    let res = app.clone().oneshot(update_req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let replace_req = auth_req_body(
        Method::PUT,
        &format!("/api/tasks/{short_id}"),
        json!({
            "title": "prefix-replaced",
            "end_at": "2026-06-06T12:00:00+09:00",
            "avg_minutes": 45,
            "depends": [],
            "parallelizable": true,
            "allows_parallel": false,
            "abandonability": 0.8
        }),
    );
    let res = app.clone().oneshot(replace_req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let not_found_req = auth_req(Method::GET, "/api/tasks/00000000");
    let res = app.clone().oneshot(not_found_req).await.unwrap();
    assert_eq!(res.status(), StatusCode::NOT_FOUND);

    let delete_req = auth_req(Method::DELETE, &format!("/api/tasks/{short_id}"));
    let res = app.oneshot(delete_req).await.unwrap();
    assert_eq!(res.status(), StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn task_update_status() {
    let (state, _) = setup().await;
    let app = build_router(state);

    let req = auth_req_body(
        Method::POST,
        "/api/tasks",
        json!({
            "title": "status-test",
            "end_at": "2026-06-05T18:00:00+09:00",
            "avg_minutes": 30,
            "depends": [],
            "parallelizable": false,
            "allows_parallel": false,
            "abandonability": 0.5
        }),
    );
    let res = app.clone().oneshot(req).await.unwrap();
    let body: serde_json::Value = serde_json::from_str(&body_str(res.into_body()).await).unwrap();
    let task_id = body["id"].as_str().unwrap();

    for status in &["in_progress", "completed", "skipped"] {
        let update_req = auth_req_body(
            Method::PATCH,
            &format!("/api/tasks/{task_id}"),
            json!({ "status": status }),
        );
        let res = app.clone().oneshot(update_req).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let updated: serde_json::Value =
            serde_json::from_str(&body_str(res.into_body()).await).unwrap();
        assert_eq!(updated["status"], *status);
    }

    let bad_req = auth_req_body(
        Method::PATCH,
        &format!("/api/tasks/{task_id}"),
        json!({ "status": "invalid" }),
    );
    let res = app.oneshot(bad_req).await.unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn generate_excludes_in_progress() {
    let (state, pool) = setup().await;

    sqlx::query(
        "INSERT INTO tasks (id, title, end_at, avg_minutes, sigma_minutes, depends, parallelizable, allows_parallel, abandonability, status) VALUES ('task1', 'pending-task', '2026-06-05T18:00:00+09:00', 60, 0, '[]', 0, 0, 0.5, 'pending')"
    ).execute(&pool).await.unwrap();

    sqlx::query(
        "INSERT INTO tasks (id, title, end_at, avg_minutes, sigma_minutes, depends, parallelizable, allows_parallel, abandonability, status) VALUES ('task2', 'in-progress-task', '2026-06-05T18:00:00+09:00', 30, 0, '[]', 0, 0, 0.5, 'in_progress')"
    ).execute(&pool).await.unwrap();

    let app = build_router(state);

    let gen_req = auth_req_body(
        Method::POST,
        "/api/schedule/generate",
        json!({
            "sleep": "disabled"
        }),
    );
    let res = app.oneshot(gen_req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body: serde_json::Value = serde_json::from_str(&body_str(res.into_body()).await).unwrap();
    let schedule: Vec<serde_json::Value> =
        serde_json::from_str(body["schedule"].as_str().unwrap()).unwrap();
    assert_eq!(
        schedule.len(),
        1,
        "should include pending but exclude in_progress tasks"
    );
    assert_eq!(schedule[0]["task_id"], "task1");
}

#[tokio::test]
async fn generate_excludes_completed_and_skipped() {
    let (state, pool) = setup().await;

    sqlx::query(
        "INSERT INTO tasks (id, title, end_at, avg_minutes, sigma_minutes, depends, parallelizable, allows_parallel, abandonability, status) VALUES ('task1', 'pending-task', '2026-06-05T18:00:00+09:00', 60, 0, '[]', 0, 0, 0.5, 'pending')"
    ).execute(&pool).await.unwrap();

    sqlx::query(
        "INSERT INTO tasks (id, title, end_at, avg_minutes, sigma_minutes, depends, parallelizable, allows_parallel, abandonability, status) VALUES ('task2', 'completed-task', '2020-01-01T12:00:00Z', 30, 0, '[]', 0, 0, 0.5, 'completed')"
    ).execute(&pool).await.unwrap();

    sqlx::query(
        "INSERT INTO tasks (id, title, end_at, avg_minutes, sigma_minutes, depends, parallelizable, allows_parallel, abandonability, status) VALUES ('task3', 'skipped-task', '2026-06-05T18:00:00+09:00', 30, 0, '[]', 0, 0, 0.5, 'skipped')"
    ).execute(&pool).await.unwrap();

    sqlx::query(
        "INSERT INTO tasks (id, title, end_at, avg_minutes, sigma_minutes, depends, parallelizable, allows_parallel, abandonability, status) VALUES ('task4', 'in-progress-task', '2026-06-05T18:00:00+09:00', 30, 0, '[]', 0, 0, 0.5, 'in_progress')"
    ).execute(&pool).await.unwrap();

    let app = build_router(state);

    let gen_req = auth_req_body(
        Method::POST,
        "/api/schedule/generate",
        json!({
            "sleep": "disabled"
        }),
    );
    let res = app.oneshot(gen_req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body: serde_json::Value = serde_json::from_str(&body_str(res.into_body()).await).unwrap();
    let schedule: Vec<serde_json::Value> =
        serde_json::from_str(body["schedule"].as_str().unwrap()).unwrap();
    let task_ids: Vec<&str> = schedule
        .iter()
        .map(|e| e["task_id"].as_str().unwrap())
        .collect();
    assert!(task_ids.contains(&"task1"), "should include pending task");
    assert!(
        !task_ids.contains(&"task2"),
        "should exclude completed task"
    );
    assert!(!task_ids.contains(&"task3"), "should exclude skipped task");
    assert!(
        !task_ids.contains(&"task4"),
        "should exclude in_progress task"
    );
}

#[tokio::test]
async fn settings_get_default() {
    let (state, _) = setup().await;
    let app = build_router(state);

    let req = auth_req(Method::GET, "/api/settings");
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body: serde_json::Value = serde_json::from_str(&body_str(res.into_body()).await).unwrap();
    assert_eq!(body["tz"], "UTC");
    assert_eq!(body["sleep_start"], "22:00");
    assert_eq!(body["sleep_end"], "06:00");
}

#[tokio::test]
async fn settings_update() {
    let (state, _) = setup().await;
    let app = build_router(state);

    let req = auth_req_body(
        Method::PUT,
        "/api/settings",
        json!({
            "tz": "Asia/Tokyo",
            "sleep_start": "23:00",
            "sleep_end": "07:00"
        }),
    );
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body: serde_json::Value = serde_json::from_str(&body_str(res.into_body()).await).unwrap();
    assert_eq!(body["tz"], "Asia/Tokyo");
    assert_eq!(body["sleep_start"], "23:00");
    assert_eq!(body["sleep_end"], "07:00");

    let get_req = auth_req(Method::GET, "/api/settings");
    let res = app.oneshot(get_req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body: serde_json::Value = serde_json::from_str(&body_str(res.into_body()).await).unwrap();
    assert_eq!(body["tz"], "Asia/Tokyo");
    assert_eq!(body["sleep_start"], "23:00");
}

#[tokio::test]
async fn settings_update_partial() {
    let (state, _) = setup().await;
    let app = build_router(state);

    let req = auth_req_body(
        Method::PUT,
        "/api/settings",
        json!({ "tz": "Europe/Berlin" }),
    );
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body: serde_json::Value = serde_json::from_str(&body_str(res.into_body()).await).unwrap();
    assert_eq!(body["tz"], "Europe/Berlin");
    assert_eq!(body["sleep_start"], "22:00");
    assert_eq!(body["sleep_end"], "06:00");
}

#[tokio::test]
async fn schedule_generate_with_custom_sleep() {
    let (state, pool) = setup().await;

    sqlx::query(
        "INSERT INTO tasks (id, title, end_at, avg_minutes, sigma_minutes, depends, parallelizable, allows_parallel, abandonability, status) VALUES ('task1', 'test-task', '2026-06-05T18:00:00+09:00', 60, 0, '[]', 0, 0, 0.5, 'pending')",
    )
    .execute(&pool)
    .await
    .unwrap();

    let app = build_router(state);

    let set_req = auth_req_body(
        Method::PUT,
        "/api/settings",
        json!({ "tz": "Asia/Tokyo", "sleep_start": "23:00", "sleep_end": "07:00" }),
    );
    let res = app.clone().oneshot(set_req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let gen_req = auth_req_body(
        Method::POST,
        "/api/schedule/generate",
        json!({
            "sleep": "recommended"
        }),
    );
    let res = app.oneshot(gen_req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
}

#[tokio::test]
async fn move_entry_with_force_overrides_warnings() {
    let (state, pool) = setup().await;

    sqlx::query(
        "INSERT INTO tasks (id, title, end_at, avg_minutes, sigma_minutes, depends, parallelizable, allows_parallel, abandonability, status) VALUES ('t1', 'Task1', '2026-06-10T12:00:00Z', 60, 0, '[]', 0, 0, 0.5, 'scheduled')",
    )
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO schedules (id, schedule, created_at, updated_at) VALUES ('active', '[{\"task_id\":\"t1\",\"start_at\":\"2026-06-10T10:00:00Z\",\"end_at\":\"2026-06-10T11:00:00Z\"}]', datetime('now'), datetime('now'))",
    )
    .execute(&pool)
    .await
    .unwrap();

    let app = build_router(state);

    let req = auth_req_body(
        Method::PATCH,
        "/api/schedule/entries/t1",
        json!({
            "start_at": "2026-06-10T13:00:00Z",
            "force": true
        }),
    );
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body: serde_json::Value = serde_json::from_str(&body_str(res.into_body()).await).unwrap();
    assert_eq!(body["task_id"], "t1");
    assert!(
        body["warnings"]
            .as_array()
            .unwrap()
            .contains(&json!("deadline_violation"))
    );
}

#[tokio::test]
async fn move_entry_without_force_rejects_violations() {
    let (state, pool) = setup().await;

    sqlx::query(
        "INSERT INTO tasks (id, title, end_at, avg_minutes, sigma_minutes, depends, parallelizable, allows_parallel, abandonability, status) VALUES ('t1', 'Task1', '2026-06-10T12:00:00Z', 60, 0, '[]', 0, 0, 0.5, 'scheduled')",
    )
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO schedules (id, schedule, created_at, updated_at) VALUES ('active', '[{\"task_id\":\"t1\",\"start_at\":\"2026-06-10T10:00:00Z\",\"end_at\":\"2026-06-10T11:00:00Z\"}]', datetime('now'), datetime('now'))",
    )
    .execute(&pool)
    .await
    .unwrap();

    let app = build_router(state);

    let req = auth_req_body(
        Method::PATCH,
        "/api/schedule/entries/t1",
        json!({
            "start_at": "2026-06-10T13:00:00Z",
            "force": false
        }),
    );
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::CONFLICT);
}

#[tokio::test]
async fn move_entry_no_violation_succeeds() {
    let (state, pool) = setup().await;

    sqlx::query(
        "INSERT INTO tasks (id, title, end_at, avg_minutes, sigma_minutes, depends, parallelizable, allows_parallel, abandonability, status) VALUES ('t1', 'Task1', '2026-06-10T14:00:00Z', 60, 0, '[]', 0, 0, 0.5, 'scheduled')",
    )
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO schedules (id, schedule, created_at, updated_at) VALUES ('active', '[{\"task_id\":\"t1\",\"start_at\":\"2026-06-10T10:00:00Z\",\"end_at\":\"2026-06-10T11:00:00Z\"}]', datetime('now'), datetime('now'))",
    )
    .execute(&pool)
    .await
    .unwrap();

    let app = build_router(state);

    let req = auth_req_body(
        Method::PATCH,
        "/api/schedule/entries/t1",
        json!({
            "start_at": "2026-06-10T12:00:00Z",
            "force": false
        }),
    );
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body: serde_json::Value = serde_json::from_str(&body_str(res.into_body()).await).unwrap();
    assert_eq!(body["task_id"], "t1");
}

#[tokio::test]
async fn move_entry_task_not_in_schedule_errors() {
    let (state, pool) = setup().await;

    sqlx::query(
        "INSERT INTO tasks (id, title, end_at, avg_minutes, sigma_minutes, depends, parallelizable, allows_parallel, abandonability, status) VALUES ('t1', 'Task1', '2026-06-10T14:00:00Z', 60, 0, '[]', 0, 0, 0.5, 'scheduled')",
    )
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO schedules (id, schedule, created_at, updated_at) VALUES ('active', '[]', datetime('now'), datetime('now'))",
    )
    .execute(&pool)
    .await
    .unwrap();

    let app = build_router(state);

    let req = auth_req_body(
        Method::PATCH,
        "/api/schedule/entries/t1",
        json!({ "start_at": "2026-06-10T12:00:00Z" }),
    );
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn clear_schedule_when_empty_is_noop() {
    let (state, _) = setup().await;
    let app = build_router(state);

    let req = auth_req(Method::DELETE, "/api/schedule");
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn reschedule_range_mode() {
    let (state, pool) = setup().await;

    sqlx::query(
        "INSERT INTO tasks (id, title, end_at, avg_minutes, sigma_minutes, depends, parallelizable, allows_parallel, abandonability, status) VALUES ('t1', 'Task1', '2026-06-10T14:00:00Z', 60, 0, '[]', 0, 0, 0.5, 'scheduled')",
    )
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO schedules (id, schedule, created_at, updated_at) VALUES ('active', '[{\"task_id\":\"t1\",\"start_at\":\"2026-06-10T10:00:00Z\",\"end_at\":\"2026-06-10T11:00:00Z\"}]', datetime('now'), datetime('now'))",
    )
    .execute(&pool)
    .await
    .unwrap();

    let app = build_router(state);

    let req = auth_req_body(
        Method::POST,
        "/api/schedule/reschedule",
        json!({
            "mode": "range",
            "from": "2026-06-10T08:00:00Z",
            "until": "2026-06-10T18:00:00Z",
            "sleep": "disabled"
        }),
    );
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
}

#[tokio::test]
async fn sync_settings_flow() {
    let (state, _) = setup().await;
    let app = build_router(state);

    let get_req = auth_req(Method::GET, "/api/sync/settings");
    let res = app.clone().oneshot(get_req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body: serde_json::Value = serde_json::from_str(&body_str(res.into_body()).await).unwrap();
    assert_eq!(body["enabled"], false);

    let put_req = auth_req_body(
        Method::PUT,
        "/api/sync/settings",
        json!({
            "enabled": true,
            "calendar_id": "test@calendar.com",
            "client_id": "fake-client-id",
            "client_secret": "fake-secret"
        }),
    );
    let res = app.clone().oneshot(put_req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body: serde_json::Value = serde_json::from_str(&body_str(res.into_body()).await).unwrap();
    assert_eq!(body["enabled"], true);
    assert_eq!(body["has_client_secret"], true);
}

#[tokio::test]
async fn generate_schedule_excludes_completed_in_progress_skipped() {
    let (state, pool) = setup().await;

    sqlx::query(
        "INSERT INTO tasks (id, title, end_at, avg_minutes, sigma_minutes, depends, parallelizable, allows_parallel, abandonability, status) VALUES ('p1', 'Pending', '2026-06-10T18:00:00Z', 30, 0, '[]', 0, 0, 0.5, 'pending')",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO tasks (id, title, end_at, avg_minutes, sigma_minutes, depends, parallelizable, allows_parallel, abandonability, status) VALUES ('s1', 'Scheduled', '2026-06-10T18:00:00Z', 30, 0, '[]', 0, 0, 0.5, 'scheduled')",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO tasks (id, title, end_at, avg_minutes, sigma_minutes, depends, parallelizable, allows_parallel, abandonability, status) VALUES ('c1', 'Completed', '2026-06-10T18:00:00Z', 30, 0, '[]', 0, 0, 0.5, 'completed')",
    )
    .execute(&pool)
    .await
    .unwrap();

    let app = build_router(state);

    let gen_req = auth_req_body(
        Method::POST,
        "/api/schedule/generate",
        json!({
            "sleep": "disabled"
        }),
    );
    let res = app.clone().oneshot(gen_req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body: serde_json::Value = serde_json::from_str(&body_str(res.into_body()).await).unwrap();
    let schedule: Vec<serde_json::Value> =
        serde_json::from_str(body["schedule"].as_str().unwrap()).unwrap();
    let task_ids: Vec<&str> = schedule
        .iter()
        .map(|e| e["task_id"].as_str().unwrap())
        .collect();
    assert!(task_ids.contains(&"p1"));
    assert!(task_ids.contains(&"s1"));
    assert!(!task_ids.contains(&"c1"));
}

#[tokio::test]
async fn habit_sync_marks_generated_task_unedited() {
    let (state, _) = setup().await;
    let app = build_router(state);

    let create_req = auth_req_body(
        Method::POST,
        "/api/habits",
        json!({
            "title": "朝のランニング",
            "recurrence": r#"{"freq":"daily","interval":1,"by_day":[],"by_month":[],"by_month_day":[],"count":null,"exdates":[]}"#,
            "start_time": "06:00",
            "end_time": "07:00",
            "avg_minutes": 30,
            "sigma_minutes": 5,
            "parallelizable": false,
            "allows_parallel": false,
            "abandonability": 0.1
        }),
    );
    let res = app.clone().oneshot(create_req).await.unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);
    let body: serde_json::Value = serde_json::from_str(&body_str(res.into_body()).await).unwrap();
    let habit_id = body["id"].as_str().unwrap();

    let gen_req = auth_req_body(
        Method::POST,
        "/api/schedule/generate",
        json!({
            "sleep": "disabled"
        }),
    );
    let res = app.clone().oneshot(gen_req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let list_req = auth_req(Method::GET, &format!("/api/tasks?habit_id={habit_id}"));
    let res = app.clone().oneshot(list_req).await.unwrap();
    let tasks: Vec<serde_json::Value> =
        serde_json::from_str(&body_str(res.into_body()).await).unwrap();
    assert!(!tasks.is_empty());
    for t in &tasks {
        assert_eq!(t["user_edited"], false);
    }
}

#[tokio::test]
async fn task_edit_marks_habit_task_user_edited() {
    let (state, _) = setup().await;
    let app = build_router(state);

    let create_req = auth_req_body(
        Method::POST,
        "/api/habits",
        json!({
            "title": "朝のランニング",
            "recurrence": r#"{"freq":"daily","interval":1,"by_day":[],"by_month":[],"by_month_day":[],"count":null,"exdates":[]}"#,
            "start_time": "06:00",
            "end_time": "07:00",
            "avg_minutes": 30,
            "sigma_minutes": 5,
            "parallelizable": false,
            "allows_parallel": false,
            "abandonability": 0.1
        }),
    );
    let res = app.clone().oneshot(create_req).await.unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);
    let body: serde_json::Value = serde_json::from_str(&body_str(res.into_body()).await).unwrap();
    let habit_id = body["id"].as_str().unwrap();

    let gen_req = auth_req_body(
        Method::POST,
        "/api/schedule/generate",
        json!({
            "sleep": "disabled"
        }),
    );
    let res = app.clone().oneshot(gen_req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let list_req = auth_req(Method::GET, &format!("/api/tasks?habit_id={habit_id}"));
    let res = app.clone().oneshot(list_req).await.unwrap();
    let tasks: Vec<serde_json::Value> =
        serde_json::from_str(&body_str(res.into_body()).await).unwrap();
    let task_id = tasks[0]["id"].as_str().unwrap();
    assert_eq!(tasks[0]["user_edited"], false);

    let edit_req = auth_req_body(
        Method::PATCH,
        &format!("/api/tasks/{task_id}"),
        json!({ "title": " edited" }),
    );
    let res = app.clone().oneshot(edit_req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let updated: serde_json::Value =
        serde_json::from_str(&body_str(res.into_body()).await).unwrap();
    assert_eq!(updated["user_edited"], true);
}

#[tokio::test]
async fn task_status_update_keeps_user_edited_false() {
    let (state, _) = setup().await;
    let app = build_router(state);

    let create_req = auth_req_body(
        Method::POST,
        "/api/habits",
        json!({
            "title": "朝のランニング",
            "recurrence": r#"{"freq":"daily","interval":1,"by_day":[],"by_month":[],"by_month_day":[],"count":null,"exdates":[]}"#,
            "start_time": "06:00",
            "end_time": "07:00",
            "avg_minutes": 30,
            "sigma_minutes": 5,
            "parallelizable": false,
            "allows_parallel": false,
            "abandonability": 0.1
        }),
    );
    let res = app.clone().oneshot(create_req).await.unwrap();
    let body: serde_json::Value = serde_json::from_str(&body_str(res.into_body()).await).unwrap();
    let habit_id = body["id"].as_str().unwrap();

    let gen_req = auth_req_body(
        Method::POST,
        "/api/schedule/generate",
        json!({
            "sleep": "disabled"
        }),
    );
    let res = app.clone().oneshot(gen_req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let list_req = auth_req(Method::GET, &format!("/api/tasks?habit_id={habit_id}"));
    let res = app.clone().oneshot(list_req).await.unwrap();
    let tasks: Vec<serde_json::Value> =
        serde_json::from_str(&body_str(res.into_body()).await).unwrap();
    let task_id = tasks[0]["id"].as_str().unwrap();

    let status_req = auth_req_body(
        Method::PATCH,
        &format!("/api/tasks/{task_id}"),
        json!({ "status": "completed" }),
    );
    let res = app.clone().oneshot(status_req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let updated: serde_json::Value =
        serde_json::from_str(&body_str(res.into_body()).await).unwrap();
    assert_eq!(updated["user_edited"], false);
}

#[tokio::test]
async fn habit_change_respects_user_edited_flag() {
    let (state, _) = setup().await;
    let app = build_router(state);

    let create_req = auth_req_body(
        Method::POST,
        "/api/habits",
        json!({
            "title": "朝のランニング",
            "recurrence": r#"{"freq":"daily","interval":1,"by_day":[],"by_month":[],"by_month_day":[],"count":null,"exdates":[]}"#,
            "start_time": "06:00",
            "end_time": "07:00",
            "avg_minutes": 30,
            "sigma_minutes": 5,
            "parallelizable": false,
            "allows_parallel": false,
            "abandonability": 0.1
        }),
    );
    let res = app.clone().oneshot(create_req).await.unwrap();
    let body: serde_json::Value = serde_json::from_str(&body_str(res.into_body()).await).unwrap();
    let habit_id = body["id"].as_str().unwrap();

    let gen_req = auth_req_body(
        Method::POST,
        "/api/schedule/generate",
        json!({
            "sleep": "disabled"
        }),
    );
    let res = app.clone().oneshot(gen_req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let list_req = auth_req(Method::GET, &format!("/api/tasks?habit_id={habit_id}"));
    let res = app.clone().oneshot(list_req).await.unwrap();
    let tasks: Vec<serde_json::Value> =
        serde_json::from_str(&body_str(res.into_body()).await).unwrap();
    assert!(tasks.len() >= 3, "expected at least 3 generated tasks");
    // Pick tasks near the middle of the generated range so they are included
    // in both sync runs even if the exact window shifts slightly.
    let task_id = tasks[tasks.len() / 3]["id"].as_str().unwrap();
    let other_id = tasks[tasks.len() * 2 / 3]["id"].as_str().unwrap();

    // After generation tasks are scheduled; sync only mutates pending tasks.
    // Set both targets back to pending so the next sync can update them.
    let pend_req = auth_req_body(
        Method::PATCH,
        &format!("/api/tasks/{task_id}"),
        json!({ "status": "pending" }),
    );
    let res = app.clone().oneshot(pend_req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let pend_req = auth_req_body(
        Method::PATCH,
        &format!("/api/tasks/{other_id}"),
        json!({ "status": "pending" }),
    );
    let res = app.clone().oneshot(pend_req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let edit_req = auth_req_body(
        Method::PATCH,
        &format!("/api/tasks/{task_id}"),
        json!({ "avg_minutes": 99 }),
    );
    let res = app.clone().oneshot(edit_req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let habit_update = auth_req_body(
        Method::PATCH,
        &format!("/api/habits/{habit_id}"),
        json!({ "avg_minutes": 45 }),
    );
    let res = app.clone().oneshot(habit_update).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let gen_req = auth_req_body(
        Method::POST,
        "/api/schedule/generate",
        json!({
            "sleep": "disabled"
        }),
    );
    let res = app.clone().oneshot(gen_req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let get_req = auth_req(Method::GET, &format!("/api/tasks/{task_id}"));
    let res = app.clone().oneshot(get_req).await.unwrap();
    let edited: serde_json::Value = serde_json::from_str(&body_str(res.into_body()).await).unwrap();
    assert_eq!(edited["avg_minutes"], 99);
    assert_eq!(edited["user_edited"], true);

    let get_req = auth_req(Method::GET, &format!("/api/tasks/{other_id}"));
    let res = app.clone().oneshot(get_req).await.unwrap();
    let other: serde_json::Value = serde_json::from_str(&body_str(res.into_body()).await).unwrap();
    assert_eq!(other["avg_minutes"], 45);
    assert_eq!(other["user_edited"], false);
}

#[tokio::test]
async fn revert_to_habit_clears_user_edited_flag() {
    let (state, _) = setup().await;
    let app = build_router(state);

    let create_req = auth_req_body(
        Method::POST,
        "/api/habits",
        json!({
            "title": "朝のランニング",
            "recurrence": r#"{"freq":"daily","interval":1,"by_day":[],"by_month":[],"by_month_day":[],"count":null,"exdates":[]}"#,
            "start_time": "06:00",
            "end_time": "07:00",
            "avg_minutes": 30,
            "sigma_minutes": 5,
            "parallelizable": false,
            "allows_parallel": false,
            "abandonability": 0.1
        }),
    );
    let res = app.clone().oneshot(create_req).await.unwrap();
    let body: serde_json::Value = serde_json::from_str(&body_str(res.into_body()).await).unwrap();
    let habit_id = body["id"].as_str().unwrap();

    let gen_req = auth_req_body(
        Method::POST,
        "/api/schedule/generate",
        json!({
            "sleep": "disabled"
        }),
    );
    let res = app.clone().oneshot(gen_req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let list_req = auth_req(Method::GET, &format!("/api/tasks?habit_id={habit_id}"));
    let res = app.clone().oneshot(list_req).await.unwrap();
    let tasks: Vec<serde_json::Value> =
        serde_json::from_str(&body_str(res.into_body()).await).unwrap();
    assert!(tasks.len() >= 3, "expected at least 3 generated tasks");
    // Pick a task near the middle of the generated range to survive both syncs.
    let task_id = tasks[tasks.len() / 2]["id"].as_str().unwrap();

    // sync only mutates pending tasks; set target back to pending before the next sync.
    let pend_req = auth_req_body(
        Method::PATCH,
        &format!("/api/tasks/{task_id}"),
        json!({ "status": "pending" }),
    );
    let res = app.clone().oneshot(pend_req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let edit_req = auth_req_body(
        Method::PATCH,
        &format!("/api/tasks/{task_id}"),
        json!({ "avg_minutes": 99 }),
    );
    let res = app.clone().oneshot(edit_req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let revert_req = auth_req_body(
        Method::PATCH,
        &format!("/api/tasks/{task_id}"),
        json!({ "user_edited": false }),
    );
    let res = app.clone().oneshot(revert_req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let reverted: serde_json::Value =
        serde_json::from_str(&body_str(res.into_body()).await).unwrap();
    assert_eq!(reverted["user_edited"], false);

    let habit_update = auth_req_body(
        Method::PATCH,
        &format!("/api/habits/{habit_id}"),
        json!({ "avg_minutes": 45 }),
    );
    let res = app.clone().oneshot(habit_update).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let gen_req = auth_req_body(
        Method::POST,
        "/api/schedule/generate",
        json!({
            "sleep": "disabled"
        }),
    );
    let res = app.clone().oneshot(gen_req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let get_req = auth_req(Method::GET, &format!("/api/tasks/{task_id}"));
    let res = app.clone().oneshot(get_req).await.unwrap();
    let task: serde_json::Value = serde_json::from_str(&body_str(res.into_body()).await).unwrap();
    assert_eq!(task["avg_minutes"], 45);
}

#[tokio::test]
async fn stale_user_edited_task_is_not_deleted() {
    let (state, pool) = setup().await;
    let app = build_router(state);

    let create_req = auth_req_body(
        Method::POST,
        "/api/habits",
        json!({
            "title": "朝のランニング",
            "recurrence": r#"{"freq":"daily","interval":1,"by_day":[],"by_month":[],"by_month_day":[],"count":null,"exdates":[]}"#,
            "start_time": "06:00",
            "end_time": "07:00",
            "avg_minutes": 30,
            "sigma_minutes": 5,
            "parallelizable": false,
            "allows_parallel": false,
            "abandonability": 0.1
        }),
    );
    let res = app.clone().oneshot(create_req).await.unwrap();
    let body: serde_json::Value = serde_json::from_str(&body_str(res.into_body()).await).unwrap();
    let habit_id = body["id"].as_str().unwrap();

    sqlx::query(
        "INSERT INTO tasks (id, title, end_at, avg_minutes, sigma_minutes, depends, parallelizable, allows_parallel, abandonability, status, habit_id, user_edited, start_at) VALUES ('stale1', 'Stale edited', '2030-01-01T18:00:00Z', 30, 0, '[]', 0, 0, 0.5, 'pending', ?, 1, '2030-01-01T09:00:00Z')",
    )
    .bind(habit_id)
    .execute(&pool)
    .await
    .unwrap();

    // Use a different date so the two stale tasks do not collide in the
    // (habit_id, date) key used by sync_habit_tasks.
    sqlx::query(
        "INSERT INTO tasks (id, title, end_at, avg_minutes, sigma_minutes, depends, parallelizable, allows_parallel, abandonability, status, habit_id, user_edited, start_at) VALUES ('stale2', 'Stale unedited', '2030-01-02T18:00:00Z', 30, 0, '[]', 0, 0, 0.5, 'pending', ?, 0, '2030-01-02T09:00:00Z')",
    )
    .bind(habit_id)
    .execute(&pool)
    .await
    .unwrap();

    let gen_req = auth_req_body(
        Method::POST,
        "/api/schedule/generate",
        json!({
            "sleep": "disabled"
        }),
    );
    let res = app.clone().oneshot(gen_req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let get_req = auth_req(Method::GET, "/api/tasks/stale1");
    let res = app.clone().oneshot(get_req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let get_req = auth_req(Method::GET, "/api/tasks/stale2");
    let res = app.clone().oneshot(get_req).await.unwrap();
    assert_eq!(res.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn habit_sync_uses_local_date_in_non_utc_timezone() {
    // Regression test: sync_habit_tasks used to compute the (habit_id, date)
    // key and the title date from the UTC date of start_at. In Asia/Tokyo
    // (UTC+9), a habit starting at 08:40 JST maps to 23:40 UTC on the
    // *previous* day, so a Mon-Fri habit generated a task titled
    // "habit (2026-07-05)" (Sunday, UTC date) instead of
    // "habit (2026-07-06)" (Monday, local date). This also caused the
    // weekday filter to appear broken and tasks to be skipped/duplicated
    // across day boundaries.
    let (state, pool) = setup().await;
    let app = build_router(state);

    // Set timezone to Asia/Tokyo.
    let req = auth_req_body(Method::PUT, "/api/settings", json!({ "tz": "Asia/Tokyo" }));
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    // Insert a habit with by_day=[Mon-Fri] and start_time=08:40 JST.
    // 08:40 JST = 23:40 UTC (previous day), which is the exact case that
    // exposed the bug.
    let habit_id = sqlx::query_scalar::<_, String>(
        "INSERT INTO habits (id, title, description, recurrence, start_time, end_time, avg_minutes, sigma_minutes, parallelizable, allows_parallel, abandonability, active, fixed) VALUES ('habit-tz', '平日朝', '', '{\"freq\":\"daily\",\"interval\":1,\"by_day\":[{\"n\":null,\"weekday\":\"mon\"},{\"n\":null,\"weekday\":\"tue\"},{\"n\":null,\"weekday\":\"wed\"},{\"n\":null,\"weekday\":\"thu\"},{\"n\":null,\"weekday\":\"fri\"}],\"by_month\":[],\"by_month_day\":[],\"count\":null,\"exdates\":[]}', '08:40', '16:30', 470, 15, 0, 0, 0.2, 1, 0) RETURNING id",
    )
    .fetch_one(&pool)
    .await
    .unwrap();

    // Trigger habit sync via schedule/generate.
    let gen_req = auth_req_body(
        Method::POST,
        "/api/schedule/generate",
        json!({ "sleep": "disabled" }),
    );
    let res = app.clone().oneshot(gen_req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    // List generated tasks for this habit.
    let list_req = auth_req(Method::GET, &format!("/api/tasks?habit_id={habit_id}"));
    let res = app.clone().oneshot(list_req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let tasks: Vec<serde_json::Value> =
        serde_json::from_str(&body_str(res.into_body()).await).unwrap();
    assert!(!tasks.is_empty(), "habit should generate at least one task");

    // The title includes the date in parentheses. Before the fix, the date
    // in the title was the UTC date (1 day behind the JST date of start_at).
    // Verify that the title date matches the JST date of start_at.
    let tz = jiff::tz::TimeZone::get("Asia/Tokyo").unwrap();
    for t in &tasks {
        let title = t["title"].as_str().unwrap();
        let start_at = t["start_at"].as_str().unwrap();
        let ts: jiff::Timestamp = start_at.parse().unwrap();
        let jst_date = ts.to_zoned(tz.clone()).date().to_string();
        assert!(
            title.contains(&jst_date),
            "title '{}' should contain JST date '{}' (start_at={})",
            title,
            jst_date,
            start_at
        );
    }
}

#[tokio::test]
async fn task_create_rejects_negative_avg_minutes() {
    let (state, _) = setup().await;
    let app = build_router(state);
    let req = auth_req_body(
        Method::POST,
        "/api/tasks",
        json!({
            "title": "bad",
            "end_at": "2026-07-06T18:00:00Z",
            "avg_minutes": -10,
        }),
    );
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn task_create_rejects_negative_sigma_minutes() {
    let (state, _) = setup().await;
    let app = build_router(state);
    let req = auth_req_body(
        Method::POST,
        "/api/tasks",
        json!({
            "title": "bad",
            "end_at": "2026-07-06T18:00:00Z",
            "avg_minutes": 30,
            "sigma_minutes": -5,
        }),
    );
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn habit_create_rejects_invalid_recurrence() {
    let (state, _) = setup().await;
    let app = build_router(state);
    let req = auth_req_body(
        Method::POST,
        "/api/habits",
        json!({
            "title": "bad",
            "recurrence": "not json",
            "start_time": "06:00",
            "end_time": "07:00",
            "avg_minutes": 30,
        }),
    );
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn settings_update_rejects_invalid_timezone() {
    let (state, _) = setup().await;
    let app = build_router(state);
    let req = auth_req_body(Method::PUT, "/api/settings", json!({ "tz": "Asia/Tokyoo" }));
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn task_replace_rejects_negative_avg_minutes() {
    let (state, _) = setup().await;
    let app = build_router(state);
    // First create a valid task to replace.
    let create_req = auth_req_body(
        Method::POST,
        "/api/tasks",
        json!({
            "title": "good",
            "end_at": "2026-07-06T18:00:00Z",
            "avg_minutes": 30,
        }),
    );
    let res = app.clone().oneshot(create_req).await.unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);
    let body: serde_json::Value = serde_json::from_str(&body_str(res.into_body()).await).unwrap();
    let task_id = body["id"].as_str().unwrap();

    let replace_req = auth_req_body(
        Method::PUT,
        &format!("/api/tasks/{task_id}"),
        json!({
            "title": "bad",
            "end_at": "2026-07-06T18:00:00Z",
            "avg_minutes": -10,
        }),
    );
    let res = app.oneshot(replace_req).await.unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
}

// ── Habit pauses (#303) ────────────────────────────────────────────────

/// Create a daily habit and return its id.
async fn create_daily_habit(app: &axum::Router, title: &str) -> String {
    let req = auth_req_body(
        Method::POST,
        "/api/habits",
        json!({
            "title": title,
            "recurrence": r#"{"freq":"daily","interval":1,"by_day":[],"by_month":[],"by_month_day":[],"count":null,"exdates":[]}"#,
            "start_time": "06:00",
            "end_time": "07:00",
            "avg_minutes": 30,
            "sigma_minutes": 5,
            "abandonability": 0.1
        }),
    );
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);
    let body: serde_json::Value = serde_json::from_str(&body_str(res.into_body()).await).unwrap();
    body["id"].as_str().unwrap().to_string()
}

/// Trigger habit sync via schedule/generate and return the habit's tasks.
async fn sync_habit_tasks(app: &axum::Router, habit_id: &str) -> Vec<serde_json::Value> {
    let gen_req = auth_req_body(
        Method::POST,
        "/api/schedule/generate",
        json!({ "sleep": "disabled" }),
    );
    let res = app.clone().oneshot(gen_req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let list_req = auth_req(Method::GET, &format!("/api/tasks?habit_id={habit_id}"));
    let res = app.clone().oneshot(list_req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    serde_json::from_str(&body_str(res.into_body()).await).unwrap()
}

#[tokio::test]
async fn habit_pause_skips_occurrences_in_range() {
    let (state, _) = setup().await;
    let app = build_router(state);
    let habit_id = create_daily_habit(&app, "朝のランニング").await;

    // Pick a date 3 days from today as the pause start, 5 days as the end.
    let today = jiff::Zoned::now().date();
    let pause_start = today
        .checked_add(jiff::Span::new().days(3))
        .unwrap()
        .to_string();
    let pause_end = today
        .checked_add(jiff::Span::new().days(5))
        .unwrap()
        .to_string();

    // Add the pause before generating tasks.
    let req = auth_req_body(
        Method::POST,
        &format!("/api/habits/{habit_id}/pauses"),
        json!({ "start_date": pause_start, "end_date": pause_end, "reason": "休暇" }),
    );
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);

    let tasks = sync_habit_tasks(&app, &habit_id).await;
    assert!(
        !tasks.is_empty(),
        "habit should still generate tasks outside the pause"
    );

    // No task title should contain a date within the pause range.
    for t in &tasks {
        let title = t["title"].as_str().unwrap();
        for d in 3..=5 {
            let date = today
                .checked_add(jiff::Span::new().days(d))
                .unwrap()
                .to_string();
            assert!(
                !title.contains(&date),
                "task title '{title}' should not contain paused date {date}"
            );
        }
    }
}

#[tokio::test]
async fn habit_pause_deletes_existing_pending_unedited_tasks() {
    let (state, pool) = setup().await;
    let app = build_router(state);
    let habit_id = create_daily_habit(&app, "ジム").await;

    // Generate tasks first (no pause yet). generate_schedule marks tasks as
    // 'scheduled', so reset the target task to 'pending' + unedited
    // afterwards to make it eligible for the sync cleanup loop.
    let tasks_before = sync_habit_tasks(&app, &habit_id).await;
    assert!(!tasks_before.is_empty());

    // Find a generated task's date to pause. Use the first task's date.
    let first_title = tasks_before[0]["title"].as_str().unwrap();
    // Title format: "ジム (YYYY-MM-DD)"
    let pause_date = first_title
        .split('(')
        .nth(1)
        .map(|s| s.trim_end_matches(')').trim())
        .unwrap();
    let pause_date = pause_date.to_string();
    let first_id = tasks_before[0]["id"].as_str().unwrap().to_string();

    // Reset to pending + unedited so the cleanup loop can delete it.
    sqlx::query("UPDATE tasks SET status = 'pending', user_edited = 0 WHERE id = ?")
        .bind(&first_id)
        .execute(&pool)
        .await
        .unwrap();

    // Add a pause covering that single date.
    let req = auth_req_body(
        Method::POST,
        &format!("/api/habits/{habit_id}/pauses"),
        json!({ "start_date": pause_date, "end_date": pause_date }),
    );
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);

    // Re-sync; the paused date's pending unedited task should be deleted.
    let tasks_after = sync_habit_tasks(&app, &habit_id).await;
    for t in &tasks_after {
        let title = t["title"].as_str().unwrap();
        assert!(
            !title.contains(&pause_date),
            "task for paused date {pause_date} should have been deleted"
        );
    }
}

#[tokio::test]
async fn habit_pause_protects_edited_and_nonpending_tasks() {
    let (state, pool) = setup().await;
    let app = build_router(state);
    let habit_id = create_daily_habit(&app, "読書").await;

    // Generate tasks.
    let tasks = sync_habit_tasks(&app, &habit_id).await;
    assert!(!tasks.is_empty());

    // Pick the first task and mark it user_edited + completed via direct SQL
    // so the cleanup loop must protect it.
    let first_id = tasks[0]["id"].as_str().unwrap().to_string();
    sqlx::query("UPDATE tasks SET user_edited = 1, status = 'completed' WHERE id = ?")
        .bind(&first_id)
        .execute(&pool)
        .await
        .unwrap();

    // Derive the date from the title to build a pause covering it.
    let first_title = tasks[0]["title"].as_str().unwrap();
    let pause_date = first_title
        .split('(')
        .nth(1)
        .map(|s| s.trim_end_matches(')').trim())
        .unwrap()
        .to_string();

    let req = auth_req_body(
        Method::POST,
        &format!("/api/habits/{habit_id}/pauses"),
        json!({ "start_date": pause_date, "end_date": pause_date }),
    );
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);

    // Re-sync; the protected task must still exist.
    let _ = sync_habit_tasks(&app, &habit_id).await;
    let get_req = auth_req(Method::GET, &format!("/api/tasks/{first_id}"));
    let res = app.clone().oneshot(get_req).await.unwrap();
    assert_eq!(
        res.status(),
        StatusCode::OK,
        "completed/edited task should be protected from pause cleanup"
    );
}

#[tokio::test]
async fn habit_pause_rejects_reversed_dates() {
    let (state, _) = setup().await;
    let app = build_router(state);
    let habit_id = create_daily_habit(&app, "散歩").await;
    let req = auth_req_body(
        Method::POST,
        &format!("/api/habits/{habit_id}/pauses"),
        json!({ "start_date": "2026-08-07", "end_date": "2026-08-01" }),
    );
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn habit_pause_rejects_bad_date_format() {
    let (state, _) = setup().await;
    let app = build_router(state);
    let habit_id = create_daily_habit(&app, "瞑想").await;
    let req = auth_req_body(
        Method::POST,
        &format!("/api/habits/{habit_id}/pauses"),
        json!({ "start_date": "2026/08/01", "end_date": "2026-08-07" }),
    );
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn habit_pause_rejects_non_zero_padded_dates() {
    // Non-zero-padded dates like "2026-8-1" would pass numeric parsing
    // but break the lexicographic pause-matching comparison against
    // jiff's zero-padded Date::to_string, so they must be rejected (#303).
    let (state, _) = setup().await;
    let app = build_router(state);
    let habit_id = create_daily_habit(&app, "ストレッチ").await;
    let req = auth_req_body(
        Method::POST,
        &format!("/api/habits/{habit_id}/pauses"),
        json!({ "start_date": "2026-8-1", "end_date": "2026-08-07" }),
    );
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);

    // End date non-zero-padded should also fail.
    let req = auth_req_body(
        Method::POST,
        &format!("/api/habits/{habit_id}/pauses"),
        json!({ "start_date": "2026-08-01", "end_date": "2026-8-7" }),
    );
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn habit_pause_list_and_delete() {
    let (state, _) = setup().await;
    let app = build_router(state);
    let habit_id = create_daily_habit(&app, "日記").await;

    // Add a pause.
    let req = auth_req_body(
        Method::POST,
        &format!("/api/habits/{habit_id}/pauses"),
        json!({ "start_date": "2026-08-01", "end_date": "2026-08-07", "reason": "夏休み" }),
    );
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);
    let pause_body: serde_json::Value =
        serde_json::from_str(&body_str(res.into_body()).await).unwrap();
    let pause_id = pause_body["id"].as_str().unwrap().to_string();

    // List pauses.
    let req = auth_req(Method::GET, &format!("/api/habits/{habit_id}/pauses"));
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let pauses: Vec<serde_json::Value> =
        serde_json::from_str(&body_str(res.into_body()).await).unwrap();
    assert_eq!(pauses.len(), 1);
    assert_eq!(pauses[0]["id"].as_str().unwrap(), pause_id);
    assert_eq!(pauses[0]["reason"].as_str().unwrap(), "夏休み");

    // Delete the pause.
    let req = auth_req(
        Method::DELETE,
        &format!("/api/habits/{habit_id}/pauses/{pause_id}"),
    );
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::NO_CONTENT);

    // List should now be empty.
    let req = auth_req(Method::GET, &format!("/api/habits/{habit_id}/pauses"));
    let res = app.clone().oneshot(req).await.unwrap();
    let pauses: Vec<serde_json::Value> =
        serde_json::from_str(&body_str(res.into_body()).await).unwrap();
    assert!(pauses.is_empty());
}

#[tokio::test]
async fn habit_pause_list_all_endpoint() {
    let (state, _) = setup().await;
    let app = build_router(state);
    let h1 = create_daily_habit(&app, "習慣A").await;
    let h2 = create_daily_habit(&app, "習慣B").await;

    // Add a pause to each.
    for (hid, start) in [(&h1, "2026-09-01"), (&h2, "2026-10-01")] {
        let req = auth_req_body(
            Method::POST,
            &format!("/api/habits/{hid}/pauses"),
            json!({ "start_date": start, "end_date": start }),
        );
        let res = app.clone().oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::CREATED);
    }

    // GET /api/habits/pauses must return both (and not be shadowed by
    // the /api/habits/{id} route).
    let req = auth_req(Method::GET, "/api/habits/pauses");
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let pauses: Vec<serde_json::Value> =
        serde_json::from_str(&body_str(res.into_body()).await).unwrap();
    assert_eq!(pauses.len(), 2);
}

#[tokio::test]
async fn habit_delete_removes_its_pauses() {
    // Regression: deleting a habit must also delete its pause rows so
    // they don't accumulate as orphans in list_all_habit_pauses (#303).
    // SQLite does not enable PRAGMA foreign_keys, so the ON DELETE
    // CASCADE in the schema does not fire — the cleanup must be explicit.
    let (state, _) = setup().await;
    let app = build_router(state);
    let habit_id = create_daily_habit(&app, "一時停止削除対象").await;

    // Add a pause.
    let req = auth_req_body(
        Method::POST,
        &format!("/api/habits/{habit_id}/pauses"),
        json!({ "start_date": "2026-08-01", "end_date": "2026-08-07" }),
    );
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);

    // Confirm it shows up in list-all.
    let req = auth_req(Method::GET, "/api/habits/pauses");
    let res = app.clone().oneshot(req).await.unwrap();
    let pauses: Vec<serde_json::Value> =
        serde_json::from_str(&body_str(res.into_body()).await).unwrap();
    assert_eq!(pauses.len(), 1);

    // Delete the habit.
    let req = auth_req(Method::DELETE, &format!("/api/habits/{habit_id}"));
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::NO_CONTENT);

    // list-all must now be empty — no orphaned pause rows.
    let req = auth_req(Method::GET, "/api/habits/pauses");
    let res = app.clone().oneshot(req).await.unwrap();
    let pauses: Vec<serde_json::Value> =
        serde_json::from_str(&body_str(res.into_body()).await).unwrap();
    assert!(
        pauses.is_empty(),
        "deleting a habit should remove its pause rows, but found {pauses:?}"
    );
}
