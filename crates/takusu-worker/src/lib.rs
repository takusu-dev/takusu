//! # takusu-worker — Cloudflare Worker (Rust/WASM)
//!
//! Storage + auth layer for the decoupled takusu architecture. Exposes a REST
//! API that mirrors the data subset of the takusu-local API: tasks, habits,
//! schedules, tokens, settings, Google Calendar settings/mappings. The local
//! server (`takusu-local`) is the only intended client.
//!
//! What lives here: D1 CRUD, SHA-256 token hashing, UUID v7 issuance.
//! What does NOT live here: scheduling (takusu-core), Google Calendar I/O
//! (google-cal), iCal parsing (takusu-ical) — those run in the native local
//! server.

mod auth;
mod error;
mod handlers;
mod memory;
mod models;
mod router;
mod validate;

use std::sync::Once;
use worker::{Context, Env, Request, Response};

static INIT: Once = Once::new();

fn init_logging(env: &Env) {
    INIT.call_once(|| {
        console_error_panic_hook::set_once();
        let level = env
            .var("TAKUSU_LOG")
            .ok()
            .and_then(|v| v.to_string().parse::<log::LevelFilter>().ok())
            .and_then(|f| f.to_level())
            .unwrap_or(log::Level::Info);
        wasm_logger::init(wasm_logger::Config::new(level));
    });
}

#[worker::event(fetch)]
pub async fn fetch(req: Request, env: Env, _ctx: Context) -> worker::Result<Response> {
    init_logging(&env);
    router::handle(req, env).await
}
