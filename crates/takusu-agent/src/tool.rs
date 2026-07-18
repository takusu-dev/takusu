use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Debug, thiserror::Error)]
pub enum ToolError {
    #[error("invalid arguments: {0}")]
    InvalidArgs(String),
    #[error("not found: {0}")]
    NotFound(String),
    #[error("optimistic conflict: {0}")]
    Conflict(String),
    #[error("operation cancelled by user")]
    Cancelled,
    #[error(transparent)]
    Other(#[from] Box<dyn std::error::Error + Send + Sync + 'static>),
}

impl ToolError {
    /// Errors that the LLM can correct by adjusting its request.
    pub fn is_recoverable(&self) -> bool {
        matches!(
            self,
            ToolError::InvalidArgs(_)
                | ToolError::NotFound(_)
                | ToolError::Conflict(_)
                | ToolError::Cancelled
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProposedChange {
    pub operation: String,
    pub target_label: String,
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub before: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub after: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub arguments: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub observed_updated_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InferredField {
    pub field: String,
    pub value: Value,
    pub reason: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ChangeReceipt {
    pub operation: String,
    pub target_type: String,
    pub target_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub before: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub after: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_revision: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inferred_fields: Option<Value>,
}

#[derive(Debug, Clone, Default)]
pub struct ToolOutput {
    /// JSON or text returned to the LLM.
    pub content: String,
    /// Short user-facing explanation supplied by the model for an approval request.
    pub why: Option<String>,
    pub warnings: Vec<String>,
    /// Planner writes proposed for application-level approval.
    pub proposed_changes: Vec<ProposedChange>,
    pub inferred_fields: Vec<InferredField>,
    /// Change receipts collected for the application UI.
    pub changes: Vec<ChangeReceipt>,
    pub schedule_dirty: bool,
    /// Whether this result represents an error the LLM should correct.
    pub is_error: bool,
}

#[async_trait::async_trait]
pub trait Tool: Send + Sync {
    fn name(&self) -> &'static str;
    fn description(&self) -> &'static str;
    /// JSON Schema for the arguments object (OpenAI function-calling format).
    fn parameters_schema(&self) -> Value;
    async fn call(&self, args: Value) -> Result<ToolOutput, ToolError>;

    /// Call with the tool-call id from the LLM provider.
    ///
    /// Tools that need to correlate with host UI events (e.g. `correct_asr`)
    /// should override this. The default delegates to `call`.
    async fn call_with_id(&self, _id: &str, args: Value) -> Result<ToolOutput, ToolError> {
        self.call(args).await
    }

    /// Returns the tool name in the OpenAI function-calling format.
    fn to_openai_definition(&self) -> Value {
        json!({
            "type": "function",
            "function": {
                "name": self.name(),
                "description": self.description(),
                "parameters": self.parameters_schema(),
            }
        })
    }
}

#[derive(Default)]
pub struct ToolRegistry {
    tools: HashMap<String, Box<dyn Tool>>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, tool: Box<dyn Tool>) {
        self.tools.insert(tool.name().to_string(), tool);
    }

    pub fn schemas(&self) -> Vec<Value> {
        self.tools.values().map(|t| t.parameters_schema()).collect()
    }

    /// Tool definitions in OpenAI function-calling format.
    pub fn definitions(&self) -> Vec<Value> {
        self.tools
            .values()
            .map(|t| {
                json!({
                    "type": "function",
                    "function": {
                        "name": t.name(),
                        "description": t.description(),
                        "parameters": t.parameters_schema(),
                    }
                })
            })
            .collect()
    }

    pub async fn call(&self, name: &str, args: Value) -> Result<ToolOutput, ToolError> {
        let tool = self
            .tools
            .get(name)
            .ok_or_else(|| ToolError::InvalidArgs(format!("unknown tool: {name}")))?;
        tool.call(args).await
    }

    pub async fn call_with_id(
        &self,
        name: &str,
        call_id: &str,
        args: Value,
    ) -> Result<ToolOutput, ToolError> {
        let tool = self
            .tools
            .get(name)
            .ok_or_else(|| ToolError::InvalidArgs(format!("unknown tool: {name}")))?;
        tool.call_with_id(call_id, args).await
    }
}
