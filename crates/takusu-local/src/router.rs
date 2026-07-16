use axum::Router;
use axum::body::Body;
use axum::http::Request;
use axum::middleware;
use axum::routing::{delete, get, patch, post, put};
use sentry::integrations::tower::{NewSentryLayer, SentryHttpLayer};

use crate::auth;
use crate::handlers;
use crate::state::AppState;

pub fn router(state: AppState) -> Router {
    let api = Router::new()
        .route("/tasks", post(handlers::task::create_task))
        .route("/tasks", get(handlers::task::list_tasks))
        .route("/tasks/import/ical", post(handlers::task::import_ical))
        .route(
            "/tasks/dependency-analysis",
            get(handlers::task::dependency_analysis),
        )
        .route("/tasks/{id}", get(handlers::task::get_task))
        .route("/tasks/{id}", put(handlers::task::replace_task))
        .route("/tasks/{id}", patch(handlers::task::update_task))
        .route("/tasks/{id}", delete(handlers::task::delete_task))
        .route("/habits", post(handlers::habit::create_habit))
        .route("/habits", get(handlers::habit::list_habits))
        // `/habits/pauses` and `/habits/steps` must be declared before
        // `/habits/{id}` so axum matches the literal segment instead of
        // treating "pauses" / "steps" as an id (#303 / #95).
        .route(
            "/habits/pauses",
            get(handlers::habit::list_all_habit_pauses),
        )
        .route("/habits/steps", get(handlers::habit::list_all_habit_steps))
        .route("/habits/{id}", get(handlers::habit::get_habit))
        .route("/habits/{id}", put(handlers::habit::replace_habit))
        .route("/habits/{id}", patch(handlers::habit::update_habit))
        .route("/habits/{id}", delete(handlers::habit::delete_habit))
        .route(
            "/habits/{id}/pauses",
            get(handlers::habit::list_habit_pauses),
        )
        .route(
            "/habits/{id}/pauses",
            post(handlers::habit::create_habit_pause),
        )
        .route(
            "/habits/{id}/pauses/{pause_id}",
            delete(handlers::habit::delete_habit_pause),
        )
        .route("/habits/{id}/steps", get(handlers::habit::list_habit_steps))
        .route(
            "/habits/{id}/steps",
            put(handlers::habit::replace_habit_steps),
        )
        .route(
            "/habits/{id}/steps/dependency-analysis",
            get(handlers::habit::step_dependency_analysis),
        )
        .route("/schedule", get(handlers::schedule::get_schedule))
        .route(
            "/schedule/generate",
            post(handlers::schedule::generate_schedule),
        )
        .route(
            "/schedule/preview",
            post(handlers::schedule::preview_schedule),
        )
        .route(
            "/schedule/replace",
            post(handlers::schedule::replace_schedule),
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
        .route(
            "/sync/delete-all",
            post(handlers::sync::delete_all_gcal_events),
        )
        .route("/sync/mappings", get(handlers::sync::list_mappings))
        .route("/settings", get(handlers::settings::get_settings))
        .route("/settings", put(handlers::settings::update_settings))
        .route("/skills", get(handlers::skills::list_skills))
        .route("/skills", post(handlers::skills::create_skill))
        .route("/skills/{slug}", get(handlers::skills::get_skill))
        .route("/skills/{slug}", patch(handlers::skills::update_skill))
        .route("/skills/{slug}", delete(handlers::skills::delete_skill))
        .route("/workers/health", get(handlers::settings::workers_health))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            auth::auth_middleware,
        ));

    Router::new()
        .route("/health", get(health))
        .nest("/api", api)
        .with_state(state)
        .layer(SentryHttpLayer::new().enable_transaction())
        .layer(NewSentryLayer::<Request<Body>>::new_from_top())
}

async fn health() -> &'static str {
    "ok"
}
