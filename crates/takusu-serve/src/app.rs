use axum::Router;
use axum::middleware;
use axum::routing::{delete, get, patch, post, put};
use sqlx::SqlitePool;

use crate::auth;
use crate::handler;

#[derive(Clone)]
pub struct AppState {
    pub db: SqlitePool,
    pub root_token: String,
}

pub fn app(state: AppState) -> Router {
    let api = Router::new()
        .route("/tasks", post(handler::task::create_task))
        .route("/tasks", get(handler::task::list_tasks))
        .route("/tasks/import/ical", post(handler::task::import_ical))
        .route("/tasks/{id}", get(handler::task::get_task))
        .route("/tasks/{id}", put(handler::task::replace_task))
        .route("/tasks/{id}", patch(handler::task::update_task))
        .route("/tasks/{id}", delete(handler::task::delete_task))
        .route("/habits", post(handler::habit::create_habit))
        .route("/habits", get(handler::habit::list_habits))
        .route("/habits/{id}", get(handler::habit::get_habit))
        .route("/habits/{id}", put(handler::habit::replace_habit))
        .route("/habits/{id}", patch(handler::habit::update_habit))
        .route("/habits/{id}", delete(handler::habit::delete_habit))
        .route("/schedule", get(handler::schedule::get_schedule))
        .route(
            "/schedule/generate",
            post(handler::schedule::generate_schedule),
        )
        .route("/schedule/reschedule", post(handler::schedule::reschedule))
        .route(
            "/schedule/entries/{task_id}",
            patch(handler::schedule::move_entry),
        )
        .route("/schedule", delete(handler::schedule::clear_schedule))
        .route("/tokens", post(handler::token::create_token))
        .route("/tokens", get(handler::token::list_tokens))
        .route("/tokens/{id}", delete(handler::token::revoke_token))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            auth::auth_middleware,
        ));

    Router::new()
        .route("/health", get(health))
        .nest("/api", api)
        .with_state(state)
}

async fn health() -> &'static str {
    "ok"
}
