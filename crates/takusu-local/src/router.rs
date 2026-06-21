use axum::Router;
use axum::middleware;
use axum::routing::{delete, get, patch, post, put};

use crate::auth;
use crate::handlers;
use crate::state::AppState;

pub fn router(state: AppState) -> Router {
    let api = Router::new()
        .route("/tasks", post(handlers::task::create_task))
        .route("/tasks", get(handlers::task::list_tasks))
        .route("/tasks/import/ical", post(handlers::task::import_ical))
        .route("/tasks/{id}", get(handlers::task::get_task))
        .route("/tasks/{id}", put(handlers::task::replace_task))
        .route("/tasks/{id}", patch(handlers::task::update_task))
        .route("/tasks/{id}", delete(handlers::task::delete_task))
        .route("/habits", post(handlers::habit::create_habit))
        .route("/habits", get(handlers::habit::list_habits))
        .route("/habits/{id}", get(handlers::habit::get_habit))
        .route("/habits/{id}", put(handlers::habit::replace_habit))
        .route("/habits/{id}", patch(handlers::habit::update_habit))
        .route("/habits/{id}", delete(handlers::habit::delete_habit))
        .route("/schedule", get(handlers::schedule::get_schedule))
        .route(
            "/schedule/generate",
            post(handlers::schedule::generate_schedule),
        )
        .route("/schedule/reschedule", post(handlers::schedule::reschedule))
        .route(
            "/schedule/entries/{task_id}",
            patch(handlers::schedule::move_entry),
        )
        .route("/schedule", delete(handlers::schedule::clear_schedule))
        .route("/tokens", post(handlers::token::create_token))
        .route("/tokens", get(handlers::token::list_tokens))
        .route("/tokens/{id}", delete(handlers::token::revoke_token))
        .route("/sync/settings", get(handlers::sync::get_settings))
        .route("/sync/settings", put(handlers::sync::update_settings))
        .route("/sync/oauth/url", post(handlers::sync::oauth_url))
        .route("/sync/oauth/callback", post(handlers::sync::oauth_callback))
        .route("/sync/trigger", post(handlers::sync::trigger_sync))
        .route("/sync/mappings", get(handlers::sync::list_mappings))
        .route("/settings", get(handlers::settings::get_settings))
        .route("/settings", put(handlers::settings::update_settings))
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
