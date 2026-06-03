use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use http_body_util::BodyExt;
use serde_json::json;
use sqlx::SqlitePool;
use takusu_serve::app::{AppState, app};
use takusu_serve::db;
use tower::ServiceExt;

const ROOT_TOKEN: &str = "tsk_test_root_token_0000000000000000000000000001";

async fn setup() -> (AppState, SqlitePool) {
    let pool = db::init_pool("sqlite::memory:").await.unwrap();
    db::run_migrations(&pool).await.unwrap();
    let state = AppState {
        db: pool.clone(),
        root_token: ROOT_TOKEN.to_string(),
        sync_lock: std::sync::Arc::new(tokio::sync::Mutex::new(())),
    };
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
    let router = app(state);
    let req = Request::builder()
        .uri("/health")
        .body(Body::empty())
        .unwrap();
    let res = router.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(body_str(res.into_body()).await, "ok");
}

#[tokio::test]
async fn unauthorized_without_token() {
    let (state, _) = setup().await;
    let router = app(state);
    let req = Request::builder()
        .uri("/api/tasks")
        .body(Body::empty())
        .unwrap();
    let res = router.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn unauthorized_with_wrong_token() {
    let (state, _) = setup().await;
    let router = app(state);
    let req = Request::builder()
        .uri("/api/tasks")
        .header("authorization", "Bearer wrong_token")
        .body(Body::empty())
        .unwrap();
    let res = router.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn authorized_with_root_token() {
    let (state, _) = setup().await;
    let router = app(state);
    let req = auth_req(Method::GET, "/api/tasks");
    let res = router.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
}

#[tokio::test]
async fn token_crud() {
    let (state, pool) = setup().await;
    let router = app(state);

    let req = auth_req_body(
        Method::POST,
        "/api/tokens",
        json!({ "label": "test-device" }),
    );
    let res = router.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);
    let body: serde_json::Value = serde_json::from_str(&body_str(res.into_body()).await).unwrap();
    let new_token = body["token"].as_str().unwrap();
    assert!(new_token.starts_with("tsk_"));

    let hash = takusu_serve::auth::hash_token(new_token);
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
    let router2 = app(state2);
    let req = Request::builder()
        .uri("/api/tasks")
        .header("authorization", format!("Bearer {new_token}"))
        .body(Body::empty())
        .unwrap();
    let res = router2.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let list_req = auth_req(Method::GET, "/api/tokens");
    let res = router2.oneshot(list_req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let list: serde_json::Value = serde_json::from_str(&body_str(res.into_body()).await).unwrap();
    assert_eq!(list.as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn task_create_and_list() {
    let (state, _) = setup().await;
    let router = app(state);

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
    let res = router.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);
    let body: serde_json::Value = serde_json::from_str(&body_str(res.into_body()).await).unwrap();
    assert_eq!(body["title"], "テストタスク");
    assert_eq!(body["status"], "pending");

    let list_req = auth_req(Method::GET, "/api/tasks");
    let res = router.oneshot(list_req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let list: serde_json::Value = serde_json::from_str(&body_str(res.into_body()).await).unwrap();
    assert_eq!(list.as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn task_get_update_delete() {
    let (state, _) = setup().await;
    let router = app(state);

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
    let res = router.clone().oneshot(req).await.unwrap();
    let body: serde_json::Value = serde_json::from_str(&body_str(res.into_body()).await).unwrap();
    let task_id = body["id"].as_str().unwrap();

    let get_req = auth_req(Method::GET, &format!("/api/tasks/{task_id}"));
    let res = router.clone().oneshot(get_req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let update_req = auth_req_body(
        Method::PATCH,
        &format!("/api/tasks/{task_id}"),
        json!({ "title": "updated" }),
    );
    let res = router.clone().oneshot(update_req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let updated: serde_json::Value =
        serde_json::from_str(&body_str(res.into_body()).await).unwrap();
    assert_eq!(updated["title"], "updated");

    let not_found_req = auth_req(Method::GET, "/api/tasks/nonexistent");
    let res = router.clone().oneshot(not_found_req).await.unwrap();
    assert_eq!(res.status(), StatusCode::NOT_FOUND);

    let delete_req = auth_req(Method::DELETE, &format!("/api/tasks/{task_id}"));
    let res = router.oneshot(delete_req).await.unwrap();
    assert_eq!(res.status(), StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn task_replace() {
    let (state, _) = setup().await;
    let router = app(state);

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
    let res = router.clone().oneshot(req).await.unwrap();
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
    let res = router.oneshot(replace_req).await.unwrap();
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
    let router = app(state);

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
        router.clone().oneshot(req).await.unwrap();
    }

    let req = auth_req(Method::GET, "/api/tasks?status=pending");
    let res = router.oneshot(req).await.unwrap();
    let list: serde_json::Value = serde_json::from_str(&body_str(res.into_body()).await).unwrap();
    assert_eq!(list.as_array().unwrap().len(), 3);
}

#[tokio::test]
async fn habit_crud() {
    let (state, _) = setup().await;
    let router = app(state);

    let req = auth_req_body(
        Method::POST,
        "/api/habits",
        json!({
            "title": "朝のランニング",
            "description": "30分走る",
            "recurrence": "daily",
            "start_time": "06:00",
            "end_time": "07:00",
            "avg_minutes": 30,
            "sigma_minutes": 5,
            "parallelizable": false,
            "allows_parallel": false,
            "abandonability": 0.1
        }),
    );
    let res = router.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);
    let body: serde_json::Value = serde_json::from_str(&body_str(res.into_body()).await).unwrap();
    let habit_id = body["id"].as_str().unwrap();

    let get_req = auth_req(Method::GET, &format!("/api/habits/{habit_id}"));
    let res = router.clone().oneshot(get_req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let habit: serde_json::Value = serde_json::from_str(&body_str(res.into_body()).await).unwrap();
    assert_eq!(habit["title"], "朝のランニング");
    assert_eq!(habit["recurrence"], "daily");

    let update_req = auth_req_body(
        Method::PATCH,
        &format!("/api/habits/{habit_id}"),
        json!({ "title": "夜のランニング", "active": false }),
    );
    let res = router.clone().oneshot(update_req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let updated: serde_json::Value =
        serde_json::from_str(&body_str(res.into_body()).await).unwrap();
    assert_eq!(updated["title"], "夜のランニング");

    let list_req = auth_req(Method::GET, "/api/habits");
    let res = router.clone().oneshot(list_req).await.unwrap();
    let list: serde_json::Value = serde_json::from_str(&body_str(res.into_body()).await).unwrap();
    assert_eq!(list.as_array().unwrap().len(), 1);

    let delete_req = auth_req(Method::DELETE, &format!("/api/habits/{habit_id}"));
    let res = router.oneshot(delete_req).await.unwrap();
    assert_eq!(res.status(), StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn ical_import() {
    let (state, _) = setup().await;
    let router = app(state);

    let ical = "BEGIN:VCALENDAR\r\nBEGIN:VEVENT\r\nDTSTART:20260605T090000Z\r\nDTEND:20260605T110000Z\r\nSUMMARY:会議\r\nUID:meeting-001@example.com\r\nEND:VEVENT\r\nBEGIN:VEVENT\r\nDTSTART:20260606T140000Z\r\nDTEND:20260606T150000Z\r\nSUMMARY:レビュー\r\nUID:review-001@example.com\r\nEND:VEVENT\r\nEND:VCALENDAR";

    let req = Request::builder()
        .method(Method::POST)
        .uri("/api/tasks/import/ical")
        .header("authorization", format!("Bearer {ROOT_TOKEN}"))
        .header("content-type", "text/calendar")
        .body(Body::from(ical.to_string()))
        .unwrap();
    let res = router.oneshot(req).await.unwrap();
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

    let router = app(state);
    let ical = "BEGIN:VCALENDAR\r\nBEGIN:VEVENT\r\nDTSTART:20260605T090000Z\r\nDTEND:20260605T110000Z\r\nSUMMARY:会議\r\nUID:meeting-001@example.com\r\nEND:VEVENT\r\nBEGIN:VEVENT\r\nDTSTART:20260606T140000Z\r\nDTEND:20260606T150000Z\r\nSUMMARY:レビュー\r\nUID:review-001@example.com\r\nEND:VEVENT\r\nEND:VCALENDAR";

    let req = Request::builder()
        .method(Method::POST)
        .uri("/api/tasks/import/ical")
        .header("authorization", format!("Bearer {ROOT_TOKEN}"))
        .header("content-type", "text/calendar")
        .body(Body::from(ical.to_string()))
        .unwrap();
    let res = router.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body: serde_json::Value = serde_json::from_str(&body_str(res.into_body()).await).unwrap();
    assert_eq!(body["imported"], 1);
}

#[tokio::test]
async fn schedule_generate_and_get() {
    let (state, _) = setup().await;
    let router = app(state);

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
    router.clone().oneshot(create_req).await.unwrap();

    let gen_req = auth_req_body(
        Method::POST,
        "/api/schedule/generate",
        json!({
            "from": "2026-06-05T00:00:00+09:00",
            "until": "2026-06-05T23:59:59+09:00",
            "sleep": "disabled"
        }),
    );
    let res = router.clone().oneshot(gen_req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body: serde_json::Value = serde_json::from_str(&body_str(res.into_body()).await).unwrap();
    assert!(body["schedule"].is_string());

    let get_req = auth_req(Method::GET, "/api/schedule");
    let res = router.clone().oneshot(get_req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let clear_req = auth_req(Method::DELETE, "/api/schedule");
    let res = router.clone().oneshot(clear_req).await.unwrap();
    assert_eq!(res.status(), StatusCode::NO_CONTENT);

    let get_req = auth_req(Method::GET, "/api/schedule");
    let res = router.oneshot(get_req).await.unwrap();
    assert_eq!(res.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn schedule_not_found_initially() {
    let (state, _) = setup().await;
    let router = app(state);
    let req = auth_req(Method::GET, "/api/schedule");
    let res = router.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn token_revoke() {
    let (state, pool) = setup().await;

    let hash = takusu_serve::auth::hash_token("tsk_test_revoke_token");
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

    let router = app(state);
    let revoke_req = auth_req(Method::DELETE, &format!("/api/tokens/{token_id}"));
    let res = router.oneshot(revoke_req).await.unwrap();
    assert_eq!(res.status(), StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn delete_nonexistent_task() {
    let (state, _) = setup().await;
    let router = app(state);
    let req = auth_req(Method::DELETE, "/api/tasks/nonexistent-id");
    let res = router.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::NOT_FOUND);
}
