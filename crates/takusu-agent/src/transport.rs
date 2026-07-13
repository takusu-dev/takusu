//! Versioned HTTP transport for an in-process Agent.
//!
//! The Android host mounts this router next to the planner API. Keeping the
//! transport in the Agent crate lets the CLI, Android, and a future server
//! adapter share the exact same session and approval contract.

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};

use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{AgentError, AgentSession, ApprovalRequest, ApprovalResult, TurnResult};

pub const API_VERSION: u8 = 1;

const MAX_SESSIONS: usize = 64;
const MAX_TURN_RESULTS: usize = 256;
const MAX_APPROVAL_RESULTS: usize = 256;

/// A simple FIFO-bounded `HashMap` to prevent unbounded growth of in-memory
/// caches.
struct BoundedMap<K, V> {
    capacity: usize,
    map: HashMap<K, V>,
    order: VecDeque<K>,
}

impl<K: Clone + Eq + std::hash::Hash, V> BoundedMap<K, V> {
    fn new(capacity: usize) -> Self {
        Self {
            capacity,
            map: HashMap::new(),
            order: VecDeque::new(),
        }
    }

    fn get<Q>(&self, key: &Q) -> Option<&V>
    where
        K: std::borrow::Borrow<Q>,
        Q: std::hash::Hash + Eq + ?Sized,
    {
        self.map.get(key)
    }

    fn insert(&mut self, key: K, value: V) -> Option<V> {
        let new_key = !self.map.contains_key(&key);
        if new_key
            && self.capacity > 0
            && self.map.len() >= self.capacity
            && let Some(oldest) = self.order.pop_front()
        {
            self.map.remove(&oldest);
        }
        match self.map.entry(key) {
            std::collections::hash_map::Entry::Occupied(mut entry) => Some(entry.insert(value)),
            std::collections::hash_map::Entry::Vacant(entry) => {
                self.order.push_back(entry.key().clone());
                entry.insert(value);
                None
            }
        }
    }

    fn remove<Q>(&mut self, key: &Q) -> Option<V>
    where
        K: std::borrow::Borrow<Q>,
        Q: std::hash::Hash + Eq + ?Sized,
    {
        let value = self.map.remove(key)?;
        if let Some(pos) = self.order.iter().position(|k| key == (*k).borrow()) {
            self.order.remove(pos);
        }
        Some(value)
    }
}

/// Creates an AgentSession for an authenticated local user.
///
/// The host owns configuration and secrets. They never arrive in a session
/// creation request from JavaScript.
pub trait SessionFactory: Send + Sync {
    fn create(&self) -> Result<AgentSession, AgentError>;
}

impl<F> SessionFactory for F
where
    F: Fn() -> Result<AgentSession, AgentError> + Send + Sync,
{
    fn create(&self) -> Result<AgentSession, AgentError> {
        self()
    }
}

pub struct AgentApiState {
    pub bearer_token: String,
    pub factory: Arc<dyn SessionFactory>,
    sessions: Mutex<BoundedMap<String, Arc<AgentSession>>>,
    turn_results: Mutex<BoundedMap<(String, String), TurnResultDto>>,
    approval_results: Mutex<BoundedMap<(String, String), ApprovalResultDto>>,
}

impl AgentApiState {
    pub fn new(bearer_token: impl Into<String>, factory: Arc<dyn SessionFactory>) -> Self {
        Self {
            bearer_token: bearer_token.into(),
            factory,
            sessions: Mutex::new(BoundedMap::new(MAX_SESSIONS)),
            turn_results: Mutex::new(BoundedMap::new(MAX_TURN_RESULTS)),
            approval_results: Mutex::new(BoundedMap::new(MAX_APPROVAL_RESULTS)),
        }
    }

    fn session(&self, id: &str) -> Option<Arc<AgentSession>> {
        self.sessions.lock().ok()?.get(id).cloned()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Versioned<T> {
    pub version: u8,
    #[serde(flatten)]
    pub value: T,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateSessionResponse {
    pub session_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TurnRequest {
    pub text: String,
    pub idempotency_key: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TurnResultDto {
    pub text: String,
    pub changes: Vec<crate::ChangeReceipt>,
    pub schedule_dirty: bool,
    pub approval_request: Option<ApprovalRequest>,
}

impl From<TurnResult> for TurnResultDto {
    fn from(result: TurnResult) -> Self {
        Self {
            text: result.text,
            changes: result.changes,
            schedule_dirty: result.schedule_dirty,
            approval_request: result.approval_request,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalDecisionRequest {
    pub approve: bool,
    pub idempotency_key: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalResultDto {
    pub id: String,
    pub approved: bool,
    pub changes: Vec<crate::ChangeReceipt>,
    pub schedule_dirty: bool,
}

impl From<ApprovalResult> for ApprovalResultDto {
    fn from(result: ApprovalResult) -> Self {
        Self {
            id: result.id,
            approved: result.approved,
            changes: result.changes,
            schedule_dirty: result.schedule_dirty,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct CapabilitiesResponse {
    pub audio_input: bool,
    pub tts: bool,
    pub approvals: bool,
}

pub fn router(state: Arc<AgentApiState>) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/capabilities", get(capabilities))
        .route("/sessions", post(create_session))
        .route("/sessions/{id}/turns", post(run_turn))
        .route("/sessions/{id}/approval", get(get_approval))
        .route(
            "/sessions/{id}/approvals/{approval_id}",
            post(resolve_approval),
        )
        .route("/sessions/{id}", delete(delete_session))
        .with_state(state)
}

async fn health(State(state): State<Arc<AgentApiState>>, headers: HeaderMap) -> Response {
    if !authorized(&headers, &state.bearer_token) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    Json(Versioned {
        version: API_VERSION,
        value: serde_json::json!({ "ok": true }),
    })
    .into_response()
}

async fn capabilities(State(state): State<Arc<AgentApiState>>, headers: HeaderMap) -> Response {
    if !authorized(&headers, &state.bearer_token) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    Json(Versioned {
        version: API_VERSION,
        value: CapabilitiesResponse {
            audio_input: true,
            tts: true,
            approvals: true,
        },
    })
    .into_response()
}

async fn create_session(State(state): State<Arc<AgentApiState>>, headers: HeaderMap) -> Response {
    if !authorized(&headers, &state.bearer_token) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let session = match state.factory.create() {
        Ok(session) => Arc::new(session),
        Err(error) => return agent_error(error),
    };
    let id = format!("session-{}", Uuid::now_v7());
    let mut sessions = match state.sessions.lock() {
        Ok(guard) => guard,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };
    sessions.insert(id.clone(), session);
    Json(Versioned {
        version: API_VERSION,
        value: CreateSessionResponse { session_id: id },
    })
    .into_response()
}

async fn run_turn(
    State(state): State<Arc<AgentApiState>>,
    Path(id): Path<String>,
    headers: HeaderMap,
    Json(body): Json<Versioned<TurnRequest>>,
) -> Response {
    if !authorized(&headers, &state.bearer_token) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    if body.version != API_VERSION || body.value.text.trim().is_empty() {
        return StatusCode::BAD_REQUEST.into_response();
    }
    let session = match state.session(&id) {
        Some(session) => session,
        None => return StatusCode::NOT_FOUND.into_response(),
    };
    let key = body.value.idempotency_key.filter(|key| !key.is_empty());
    if let Some(key) = &key
        && let Ok(results) = state.turn_results.lock()
        && let Some(result) = results.get(&(id.clone(), key.clone()))
    {
        return Json(Versioned {
            version: API_VERSION,
            value: result.clone(),
        })
        .into_response();
    }
    let result = match session.run_turn(&body.value.text).await {
        Ok(result) => TurnResultDto::from(result),
        Err(error) => return agent_error(error),
    };
    if let Some(key) = key
        && let Ok(mut results) = state.turn_results.lock()
    {
        results.insert((id, key), result.clone());
    }
    Json(Versioned {
        version: API_VERSION,
        value: result,
    })
    .into_response()
}

async fn get_approval(
    State(state): State<Arc<AgentApiState>>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> Response {
    if !authorized(&headers, &state.bearer_token) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let Some(session) = state.session(&id) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    match session.pending_approval() {
        Some(approval) => Json(Versioned {
            version: API_VERSION,
            value: Some(approval),
        })
        .into_response(),
        None => Json(Versioned {
            version: API_VERSION,
            value: Option::<ApprovalRequest>::None,
        })
        .into_response(),
    }
}

async fn resolve_approval(
    State(state): State<Arc<AgentApiState>>,
    Path((id, approval_id)): Path<(String, String)>,
    headers: HeaderMap,
    Json(body): Json<Versioned<ApprovalDecisionRequest>>,
) -> Response {
    if !authorized(&headers, &state.bearer_token) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    if body.version != API_VERSION {
        return StatusCode::BAD_REQUEST.into_response();
    }
    let key = body
        .value
        .idempotency_key
        .clone()
        .unwrap_or_else(|| approval_id.clone());
    if let Ok(results) = state.approval_results.lock()
        && let Some(result) = results.get(&(id.clone(), key.clone()))
    {
        return Json(Versioned {
            version: API_VERSION,
            value: result.clone(),
        })
        .into_response();
    }
    let Some(session) = state.session(&id) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let result = match session
        .resolve_approval(&approval_id, body.value.approve)
        .await
    {
        Ok(result) => ApprovalResultDto::from(result),
        Err(error) => return agent_error(error),
    };
    if let Ok(mut results) = state.approval_results.lock() {
        results.insert((id, key), result.clone());
    }
    Json(Versioned {
        version: API_VERSION,
        value: result,
    })
    .into_response()
}

async fn delete_session(
    State(state): State<Arc<AgentApiState>>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> Response {
    if !authorized(&headers, &state.bearer_token) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    match state.sessions.lock() {
        Ok(mut sessions) => {
            if sessions.remove(&id).is_some() {
                StatusCode::NO_CONTENT.into_response()
            } else {
                StatusCode::NOT_FOUND.into_response()
            }
        }
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

fn authorized(headers: &HeaderMap, token: &str) -> bool {
    headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value == format!("Bearer {token}"))
}

fn agent_error(error: AgentError) -> Response {
    let status = match &error {
        AgentError::Tool(crate::ToolError::InvalidArgs(_))
        | AgentError::Tool(crate::ToolError::NotFound(_)) => StatusCode::BAD_REQUEST,
        AgentError::Tool(crate::ToolError::Conflict(_)) => StatusCode::CONFLICT,
        AgentError::Tool(crate::ToolError::Cancelled) => StatusCode::GONE,
        AgentError::TooManyToolCalls => StatusCode::UNPROCESSABLE_ENTITY,
        _ => StatusCode::BAD_GATEWAY,
    };
    (
        status,
        Json(serde_json::json!({
            "version": API_VERSION,
            "error": error.to_string(),
        })),
    )
        .into_response()
}
