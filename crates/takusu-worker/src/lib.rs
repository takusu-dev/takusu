//! # takusu-worker — Cloudflare Worker (Rust/WASM)
//!
//! Storage + auth layer for the decoupled takusu architecture. Exposes a REST
//! API that mirrors the data subset of the legacy takusu-serve: tasks, habits,
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
mod models;
mod router;

use worker::{Context, Env, Request, Response};

#[worker::event(fetch)]
pub async fn fetch(req: Request, env: Env, _ctx: Context) -> worker::Result<Response> {
    router::handle(req, env).await
}
