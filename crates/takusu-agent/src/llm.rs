use std::time::Duration;

use async_trait::async_trait;
use rand::random;
use reqwest;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LlmProviderKind {
    Openai,
    Openrouter,
    Custom,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmProviderConfig {
    pub id: String,
    pub name: String,
    pub provider: LlmProviderKind,
    pub base_url: String,
    pub selected_model: String,
    #[serde(default)]
    pub cached_models: Vec<String>,
    pub models_fetched_at: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct LlmConfig {
    #[serde(default = "default_llm_base_url")]
    pub base_url: String,
    #[serde(default = "default_llm_model")]
    pub model: String,
    #[serde(default = "default_llm_api_key_env")]
    pub api_key_env: String,
    #[serde(default)]
    pub api_key: String,
    #[serde(default = "default_max_history")]
    pub max_history: usize,
    #[serde(default = "default_max_context_tokens")]
    pub max_context_tokens: usize,
    #[serde(default = "default_max_tool_calls")]
    pub max_tool_calls: usize,
    #[serde(default = "default_request_timeout_seconds")]
    pub request_timeout_seconds: u64,
}

impl Default for LlmConfig {
    fn default() -> Self {
        Self {
            base_url: default_llm_base_url(),
            model: default_llm_model(),
            api_key_env: default_llm_api_key_env(),
            api_key: String::new(),
            max_history: default_max_history(),
            max_context_tokens: default_max_context_tokens(),
            max_tool_calls: default_max_tool_calls(),
            request_timeout_seconds: default_request_timeout_seconds(),
        }
    }
}

fn default_llm_base_url() -> String {
    "https://api.openai.com/v1".into()
}

fn default_llm_model() -> String {
    "gpt-4.1-mini".into()
}

fn default_llm_api_key_env() -> String {
    "TAKUSU_LLM_API_KEY".into()
}

fn default_max_history() -> usize {
    64
}

fn default_max_context_tokens() -> usize {
    32000
}

fn default_max_tool_calls() -> usize {
    16
}

fn default_request_timeout_seconds() -> u64 {
    60
}

#[derive(Debug, Error)]
pub enum LlmError {
    #[error("http error {status}: {message}")]
    Http { status: u16, message: String },
    #[error("rate limited")]
    RateLimited,
    #[error("request timed out")]
    Timeout,
    #[error("response parse error: {0}")]
    Parse(String),
    #[error("request failed: {0}")]
    Request(String),
    #[error(transparent)]
    Other(#[from] Box<dyn std::error::Error + Send + Sync + 'static>),
}

impl LlmError {
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            LlmError::RateLimited
                | LlmError::Timeout
                | LlmError::Http {
                    status: 429 | 502 | 503 | 504 | 524,
                    ..
                }
                | LlmError::Request(_)
        )
    }
}

#[derive(Debug, Clone)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: Value,
}

impl ToolCall {
    pub fn to_openai(&self) -> Value {
        json!({
            "id": self.id,
            "type": "function",
            "function": {
                "name": self.name,
                "arguments": self.arguments.to_string(),
            }
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FinishReason {
    Stop,
    Length,
    ContentFilter,
    ToolCalls,
    Other(String),
}

impl From<&str> for FinishReason {
    fn from(s: &str) -> Self {
        match s {
            "stop" => FinishReason::Stop,
            "length" => FinishReason::Length,
            "content_filter" => FinishReason::ContentFilter,
            "tool_calls" | "function_call" => FinishReason::ToolCalls,
            other => FinishReason::Other(other.to_string()),
        }
    }
}

#[derive(Debug, Clone)]
pub struct LlmResponse {
    pub content: LlmResponseContent,
    pub prompt_tokens: Option<usize>,
    pub finish_reason: Option<FinishReason>,
}

#[derive(Debug, Clone)]
pub enum LlmResponseContent {
    Text(String),
    ToolCalls(Vec<ToolCall>),
}

#[derive(Debug, Clone)]
pub enum AssistantContent {
    Text(String),
    ToolCalls(Vec<ToolCall>),
}

#[derive(Debug, Clone)]
pub enum Message {
    System(String),
    User(String),
    Assistant(AssistantContent),
    ToolResult {
        call_id: String,
        content: String,
        is_error: bool,
    },
}

impl Message {
    pub fn to_openai(&self) -> Value {
        match self {
            Message::System(c) => json!({"role": "system", "content": c}),
            Message::User(c) => json!({"role": "user", "content": c}),
            Message::Assistant(AssistantContent::Text(c)) => {
                json!({"role": "assistant", "content": c})
            }
            Message::Assistant(AssistantContent::ToolCalls(calls)) => json!({
                "role": "assistant",
                "content": Value::Null,
                "tool_calls": calls.iter().map(ToolCall::to_openai).collect::<Vec<_>>(),
            }),
            Message::ToolResult {
                call_id,
                content,
                is_error,
            } => {
                let mut obj = json!({
                    "role": "tool",
                    "tool_call_id": call_id,
                    "content": content,
                });
                if *is_error {
                    obj["is_error"] = json!(true);
                }
                obj
            }
        }
    }

    pub fn role(&self) -> &'static str {
        match self {
            Message::System(_) => "system",
            Message::User(_) => "user",
            Message::Assistant(_) => "assistant",
            Message::ToolResult { .. } => "tool",
        }
    }

    /// Very rough token estimate for history trimming. Treats ~4 characters as one token
    /// plus a small per-message overhead, which is enough to preserve context limits.
    pub fn estimate_tokens(&self) -> usize {
        const OVERHEAD: usize = 4;
        const CHARS_PER_TOKEN: usize = 4;
        let content_len = match self {
            Message::System(c) | Message::User(c) => c.chars().count(),
            Message::Assistant(AssistantContent::Text(c)) => c.chars().count(),
            Message::Assistant(AssistantContent::ToolCalls(calls)) => calls
                .iter()
                .map(|c| {
                    c.name.chars().count()
                        + c.arguments.to_string().chars().count()
                        + c.id.chars().count()
                })
                .sum(),
            Message::ToolResult {
                call_id, content, ..
            } => call_id.chars().count() + content.chars().count(),
        };
        content_len.div_ceil(CHARS_PER_TOKEN) + OVERHEAD
    }
}

#[async_trait]
pub trait LlmClient: Send + Sync {
    async fn chat(&self, messages: &[Message], tools: &[Value]) -> Result<LlmResponse, LlmError>;
}

#[derive(Debug, Clone)]
pub struct OpenAIClient {
    client: reqwest::Client,
    base_url: String,
    api_key: String,
    model: String,
    request_timeout: Duration,
    max_retries: usize,
    initial_backoff: Duration,
}

impl OpenAIClient {
    pub fn new(config: LlmConfig) -> Result<Self, LlmError> {
        let client = takusu_client::default_http_client(Some(config.request_timeout_seconds))
            .map_err(|e| LlmError::Request(e.to_string()))?;

        let api_key = if config.api_key.is_empty() {
            std::env::var(&config.api_key_env).unwrap_or_default()
        } else {
            config.api_key
        };

        Ok(Self {
            client,
            base_url: config.base_url,
            api_key,
            model: config.model,
            request_timeout: Duration::from_secs(config.request_timeout_seconds),
            max_retries: 3,
            initial_backoff: Duration::from_millis(500),
        })
    }

    pub fn with_url(mut self, base_url: impl Into<String>) -> Self {
        self.base_url = base_url.into();
        self
    }

    /// Fetch model IDs from an OpenAI-compatible `/models` endpoint.
    ///
    /// Providers may expose additional fields, but only the stable `id` is
    /// surfaced to the UI so the dropdown stays provider-neutral.
    pub async fn list_models(&self) -> Result<Vec<String>, LlmError> {
        let url = format!("{}/models", self.base_url.trim_end_matches('/'));
        let response = tokio::time::timeout(
            self.request_timeout,
            self.client.get(url).bearer_auth(&self.api_key).send(),
        )
        .await
        .map_err(|_| LlmError::Timeout)?
        .map_err(|e| {
            if e.is_timeout() {
                LlmError::Timeout
            } else if e.is_request() {
                LlmError::Request(e.to_string())
            } else {
                LlmError::Other(e.into())
            }
        })?;
        let status = response.status();
        if !status.is_success() {
            let text = response.text().await.unwrap_or_default();
            return Err(if status == 429 {
                LlmError::RateLimited
            } else {
                LlmError::Http {
                    status: status.as_u16(),
                    message: extract_error_message(&text),
                }
            });
        }
        let body = response
            .json::<ModelsResponse>()
            .await
            .map_err(|e| LlmError::Parse(e.to_string()))?;
        let mut models: Vec<String> = body
            .data
            .into_iter()
            .map(|model| model.id)
            .filter(|id| !id.trim().is_empty())
            .collect();
        models.sort_unstable();
        models.dedup();
        Ok(models)
    }

    async fn chat_once(
        &self,
        messages: &[Message],
        tools: &[Value],
    ) -> Result<LlmResponse, LlmError> {
        let request = ChatCompletionRequest {
            model: self.model.clone(),
            messages: messages.iter().map(Message::to_openai).collect(),
            tools: tools.to_vec(),
        };
        let url = format!("{}/chat/completions", self.base_url.trim_end_matches('/'));

        let response = tokio::time::timeout(
            self.request_timeout,
            self.client
                .post(&url)
                .bearer_auth(&self.api_key)
                .json(&request)
                .send(),
        )
        .await
        .map_err(|_| LlmError::Timeout)?
        .map_err(|e| {
            if e.is_timeout() {
                LlmError::Timeout
            } else if e.is_request() {
                LlmError::Request(e.to_string())
            } else {
                LlmError::Other(e.into())
            }
        })?;

        self.parse_response(response).await
    }

    async fn parse_response(&self, response: reqwest::Response) -> Result<LlmResponse, LlmError> {
        let status = response.status();
        if !status.is_success() {
            let text = response.text().await.unwrap_or_default();
            let message = extract_error_message(&text);
            if status == 429 {
                return Err(LlmError::RateLimited);
            }
            return Err(LlmError::Http {
                status: status.as_u16(),
                message,
            });
        }

        let body = response
            .json::<ChatCompletionResponse>()
            .await
            .map_err(|e| LlmError::Parse(e.to_string()))?;

        let choice = body
            .choices
            .into_iter()
            .next()
            .ok_or_else(|| LlmError::Parse("no choices in response".into()))?;

        let prompt_tokens = body.usage.as_ref().map(|u| u.prompt_tokens as usize);
        let finish_reason = choice.finish_reason.as_deref().map(FinishReason::from);

        let content = if let Some(tool_calls) = choice.message.tool_calls {
            let calls = tool_calls
                .into_iter()
                .map(|tc| ToolCall {
                    id: tc.id,
                    name: tc.function.name,
                    arguments: serde_json::from_str(&tc.function.arguments).unwrap_or(Value::Null),
                })
                .collect();
            LlmResponseContent::ToolCalls(calls)
        } else {
            LlmResponseContent::Text(choice.message.content.unwrap_or_default())
        };

        Ok(LlmResponse {
            content,
            prompt_tokens,
            finish_reason,
        })
    }

    fn backoff(&self, attempt: usize) -> Duration {
        let base = self.initial_backoff.as_millis() as f64 * 2f64.powi(attempt as i32);
        let jitter = base * random::<f64>();
        Duration::from_millis((base + jitter).clamp(0.0, u64::MAX as f64) as u64)
    }
}

#[async_trait]
impl LlmClient for OpenAIClient {
    async fn chat(&self, messages: &[Message], tools: &[Value]) -> Result<LlmResponse, LlmError> {
        let mut attempt = 0;
        loop {
            let result = self.chat_once(messages, tools).await;
            match result {
                Ok(resp) => return Ok(resp),
                Err(e) if e.is_retryable() && attempt < self.max_retries => {
                    tokio::time::sleep(self.backoff(attempt)).await;
                    attempt += 1;
                }
                Err(e) => return Err(e),
            }
        }
    }
}

#[derive(Deserialize, Debug)]
struct ModelsResponse {
    #[serde(default)]
    data: Vec<ModelResponse>,
}

#[derive(Deserialize, Debug)]
struct ModelResponse {
    id: String,
}

#[derive(Serialize, Debug)]
struct ChatCompletionRequest {
    model: String,
    messages: Vec<Value>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tools: Vec<Value>,
}

#[derive(Deserialize, Debug)]
struct ChatCompletionResponse {
    choices: Vec<Choice>,
    usage: Option<Usage>,
}

#[derive(Deserialize, Debug)]
struct Usage {
    #[allow(dead_code)]
    prompt_tokens: u32,
    #[allow(dead_code)]
    completion_tokens: u32,
    #[allow(dead_code)]
    total_tokens: u32,
}

#[derive(Deserialize, Debug)]
struct Choice {
    #[allow(dead_code)]
    index: u32,
    message: ResponseMessage,
    #[allow(dead_code)]
    finish_reason: Option<String>,
}

#[derive(Deserialize, Debug)]
struct ResponseMessage {
    #[allow(dead_code)]
    role: String,
    content: Option<String>,
    #[serde(default)]
    tool_calls: Option<Vec<ToolCallResponse>>,
}

#[derive(Deserialize, Debug)]
struct ToolCallResponse {
    id: String,
    #[serde(rename = "type")]
    #[allow(dead_code)]
    type_: String,
    function: ToolCallFunction,
}

#[derive(Deserialize, Debug)]
struct ToolCallFunction {
    name: String,
    arguments: String,
}

#[derive(Deserialize, Debug)]
struct ErrorResponse {
    error: Option<ErrorDetail>,
}

#[derive(Deserialize, Debug)]
struct ErrorDetail {
    message: String,
    #[serde(rename = "type")]
    #[allow(dead_code)]
    type_: Option<String>,
    #[allow(dead_code)]
    code: Option<String>,
}

fn extract_error_message(text: &str) -> String {
    serde_json::from_str::<ErrorResponse>(text)
        .ok()
        .and_then(|r| r.error.map(|e| e.message))
        .unwrap_or_else(|| text.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn llm_error_retryable_detection() {
        assert!(LlmError::RateLimited.is_retryable());
        assert!(LlmError::Timeout.is_retryable());
        assert!(LlmError::Request("connection refused".into()).is_retryable());
        assert!(
            LlmError::Http {
                status: 429,
                message: "rate limited".into(),
            }
            .is_retryable()
        );
        assert!(
            LlmError::Http {
                status: 503,
                message: "unavailable".into(),
            }
            .is_retryable()
        );
        assert!(
            !LlmError::Http {
                status: 400,
                message: "bad request".into(),
            }
            .is_retryable()
        );
        assert!(!LlmError::Parse("broken json".into()).is_retryable());
    }

    #[test]
    fn message_serializes_to_openai_format() {
        let msg = Message::User("hello".into());
        let value = msg.to_openai();
        assert_eq!(value["role"], "user");
        assert_eq!(value["content"], "hello");

        let tool_call = ToolCall {
            id: "call_1".into(),
            name: "echo".into(),
            arguments: json!({"message": "hi"}),
        };
        let msg = Message::Assistant(AssistantContent::ToolCalls(vec![tool_call]));
        let value = msg.to_openai();
        assert_eq!(value["role"], "assistant");
        assert!(value["content"].is_null());
        assert_eq!(value["tool_calls"][0]["id"], "call_1");
        assert_eq!(value["tool_calls"][0]["function"]["name"], "echo");
    }

    #[test]
    fn fixture_text_response_deserializes() {
        let text = include_str!("fixtures/text_response.json");
        let body: ChatCompletionResponse = serde_json::from_str(text).unwrap();
        let choice = body.choices.into_iter().next().unwrap();
        assert_eq!(
            choice.message.content.as_deref(),
            Some("今日は会議が2つあります")
        );
        assert!(choice.message.tool_calls.is_none());
    }

    #[test]
    fn fixture_tool_calls_response_deserializes() {
        let text = include_str!("fixtures/tool_calls_response.json");
        let body: ChatCompletionResponse = serde_json::from_str(text).unwrap();
        let choice = body.choices.into_iter().next().unwrap();
        assert!(choice.message.content.is_none());
        let calls = choice.message.tool_calls.unwrap();
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0].function.name, "list_tasks");
        assert_eq!(calls[1].function.name, "get_schedule");
    }

    #[test]
    fn fixture_error_response_parses_message() {
        let text = include_str!("fixtures/error_response.json");
        let msg = extract_error_message(text);
        assert!(msg.contains("quota"));
    }

    #[test]
    fn response_with_null_content_parses_as_empty_text() {
        let json = json!({
            "choices": [{
                "index": 0,
                "message": { "role": "assistant", "content": null },
                "finish_reason": "stop"
            }]
        });
        let body: ChatCompletionResponse = serde_json::from_value(json).unwrap();
        let choice = body.choices.into_iter().next().unwrap();
        assert_eq!(choice.message.content.unwrap_or_default(), "");
    }

    #[tokio::test]
    async fn openai_client_parses_text_response_from_mock_server() {
        use axum::Router;
        use axum::extract::Json;
        use axum::http::StatusCode;
        use axum::response::IntoResponse;
        use axum::routing::post;

        async fn handler(Json(_): Json<Value>) -> impl IntoResponse {
            let fixture = include_str!("fixtures/text_response.json");
            let body: Value = serde_json::from_str(fixture).unwrap();
            (StatusCode::OK, axum::Json(body))
        }

        let app = Router::new().route("/chat/completions", post(handler));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

        let cfg = LlmConfig {
            base_url: format!("http://{addr}/"),
            request_timeout_seconds: 5,
            api_key_env: "UNUSED".into(),
            ..Default::default()
        };
        let client = OpenAIClient::new(cfg).unwrap();

        let response = client
            .chat(&[Message::User("hello".into())], &[])
            .await
            .unwrap();

        assert!(
            matches!(response.content, LlmResponseContent::Text(text) if text == "今日は会議が2つあります")
        );
    }

    #[tokio::test]
    async fn openai_client_parses_multiple_tool_calls_from_mock_server() {
        use axum::Router;
        use axum::extract::Json;
        use axum::http::StatusCode;
        use axum::response::IntoResponse;
        use axum::routing::post;

        async fn handler(Json(_): Json<Value>) -> impl IntoResponse {
            let fixture = include_str!("fixtures/tool_calls_response.json");
            let body: Value = serde_json::from_str(fixture).unwrap();
            (StatusCode::OK, axum::Json(body))
        }

        let app = Router::new().route("/chat/completions", post(handler));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

        let cfg = LlmConfig {
            base_url: format!("http://{addr}/"),
            request_timeout_seconds: 5,
            api_key_env: "UNUSED".into(),
            ..Default::default()
        };
        let client = OpenAIClient::new(cfg).unwrap();

        let response = client
            .chat(&[Message::User("予定を教えて".into())], &[])
            .await
            .unwrap();

        if let LlmResponseContent::ToolCalls(calls) = response.content {
            assert_eq!(calls.len(), 2);
            assert_eq!(calls[0].name, "list_tasks");
            assert_eq!(calls[1].name, "get_schedule");
        } else {
            panic!("expected tool calls, got {response:?}");
        }
    }

    #[tokio::test]
    async fn openai_client_retries_429_and_then_succeeds() {
        use axum::Router;
        use axum::extract::{Json, State};
        use axum::http::StatusCode;
        use axum::routing::post;
        use std::sync::Arc;
        use std::sync::atomic::{AtomicUsize, Ordering};

        #[derive(Clone)]
        struct AppState {
            count: Arc<AtomicUsize>,
        }

        async fn handler(
            State(state): State<AppState>,
            Json(_): Json<Value>,
        ) -> Result<axum::Json<Value>, StatusCode> {
            let count = state.count.fetch_add(1, Ordering::SeqCst);
            if count < 2 {
                return Err(StatusCode::TOO_MANY_REQUESTS);
            }
            let fixture = include_str!("fixtures/text_response.json");
            let body: Value = serde_json::from_str(fixture).unwrap();
            Ok(axum::Json(body))
        }

        let state = AppState {
            count: Arc::new(AtomicUsize::new(0)),
        };
        let app = Router::new()
            .route("/chat/completions", post(handler))
            .with_state(state.clone());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

        let cfg = LlmConfig {
            base_url: format!("http://{addr}/"),
            request_timeout_seconds: 5,
            api_key_env: "UNUSED".into(),
            ..Default::default()
        };
        let client = OpenAIClient::new(cfg).unwrap();

        let response = client
            .chat(&[Message::User("hello".into())], &[])
            .await
            .unwrap();

        assert!(matches!(response.content, LlmResponseContent::Text(_)));
        assert_eq!(state.count.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn openai_client_times_out_against_slow_server() {
        use axum::Router;
        use axum::extract::Json;
        use axum::http::StatusCode;
        use axum::response::IntoResponse;
        use axum::routing::post;

        async fn handler(Json(_): Json<Value>) -> impl IntoResponse {
            tokio::time::sleep(Duration::from_secs(2)).await;
            let fixture = include_str!("fixtures/text_response.json");
            let body: Value = serde_json::from_str(fixture).unwrap();
            (StatusCode::OK, axum::Json(body))
        }

        let app = Router::new().route("/chat/completions", post(handler));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

        let cfg = LlmConfig {
            base_url: format!("http://{addr}/"),
            request_timeout_seconds: 1,
            api_key_env: "UNUSED".into(),
            ..Default::default()
        };
        let client = OpenAIClient::new(cfg).unwrap();

        let response = client.chat(&[Message::User("hello".into())], &[]).await;

        assert!(matches!(response, Err(LlmError::Timeout)));
    }

    #[tokio::test]
    #[ignore = "requires a real OpenAI-compatible API key"]
    async fn real_endpoint_smoke_test() {
        let cfg = LlmConfig {
            api_key_env: "TAKUSU_LLM_API_KEY".into(),
            request_timeout_seconds: 30,
            ..Default::default()
        };
        let client = OpenAIClient::new(cfg).unwrap();
        let response = client.chat(&[Message::User("hello".into())], &[]).await;
        assert!(response.is_ok(), "{response:?}");
    }
}
