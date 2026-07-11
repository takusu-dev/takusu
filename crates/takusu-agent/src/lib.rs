pub mod llm;
pub mod tool;
pub mod tools;

pub use tool::{Tool, ToolError, ToolRegistry};

use serde::Deserialize;
#[cfg(test)]
use serde_json::{Value, json};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use jiff::Unit;

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct AgentConfig {
    pub llm: llm::LlmConfig,
    pub server: ServerConfig,
    pub audio: AudioConfig,
    pub skills: SkillsConfig,
}

impl AgentConfig {
    /// Load from `$XDG_CONFIG_HOME/takusu/agent.toml` and override with
    /// `TAKUSU_AGENT__<SECTION>__<KEY>` environment variables (e.g. `TAKUSU_AGENT__LLM__BASE_URL`).
    pub fn load() -> Result<Self, config::ConfigError> {
        let mut builder = config::Config::builder();

        if let Some(dir) = config_dir() {
            let path = dir.join("takusu/agent.toml");
            if path.exists() {
                builder =
                    builder.add_source(config::File::from(path).format(config::FileFormat::Toml));
            }
        }

        let cfg = builder
            .add_source(
                config::Environment::with_prefix("TAKUSU_AGENT")
                    .separator("__")
                    .try_parsing(true),
            )
            .build()?
            .try_deserialize()?;

        Ok(cfg)
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct ServerConfig {
    #[serde(default = "default_server_url")]
    pub url: String,
    #[serde(default)]
    pub token: String,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            url: default_server_url(),
            token: String::new(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct AudioConfig {
    #[serde(default = "default_funasr_url")]
    pub funasr_url: String,
    #[serde(default = "default_tts_url")]
    pub tts_url: String,
    #[serde(default = "default_refs_dir")]
    pub refs_dir: String,
}

impl Default for AudioConfig {
    fn default() -> Self {
        Self {
            funasr_url: default_funasr_url(),
            tts_url: default_tts_url(),
            refs_dir: default_refs_dir(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct SkillsConfig {
    #[serde(default = "default_skills_dir")]
    pub dir: String,
}

impl Default for SkillsConfig {
    fn default() -> Self {
        Self {
            dir: default_skills_dir(),
        }
    }
}

fn config_dir() -> Option<PathBuf> {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME").map(|h| {
                let mut p = PathBuf::from(h);
                p.push(".config");
                p
            })
        })
}

fn data_dir() -> Option<PathBuf> {
    std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME").map(|h| {
                let mut p = PathBuf::from(h);
                p.push(".local/share");
                p
            })
        })
}

fn default_server_url() -> String {
    "http://127.0.0.1:3000".into()
}

fn default_funasr_url() -> String {
    "ws://127.0.0.1:10095".into()
}

fn default_tts_url() -> String {
    "http://127.0.0.1:8088".into()
}

fn default_refs_dir() -> String {
    "./refs".into()
}

fn default_skills_dir() -> String {
    data_dir()
        .map(|d| d.join("takusu/skills").to_string_lossy().into_owned())
        .unwrap_or_else(|| "~/.local/share/takusu/skills".into())
}

#[derive(Debug, thiserror::Error)]
pub enum AgentError {
    #[error("llm error: {0}")]
    Llm(#[from] llm::LlmError),
    #[error("tool error: {0}")]
    Tool(#[from] ToolError),
    #[error("too many tool calls")]
    TooManyToolCalls,
}

pub struct Agent {
    config: AgentConfig,
    registry: ToolRegistry,
    llm: Arc<dyn llm::LlmClient + Send + Sync>,
    history: Mutex<Vec<llm::Message>>,
}

impl Agent {
    pub fn new(
        config: AgentConfig,
        registry: ToolRegistry,
        llm: impl llm::LlmClient + 'static,
    ) -> Self {
        Self {
            config,
            registry,
            llm: Arc::new(llm),
            history: Mutex::new(Vec::new()),
        }
    }

    pub async fn run_turn(&self, user_text: &str) -> Result<String, AgentError> {
        let tools = self.registry.definitions();
        let system = llm::Message::System(self.build_system_prompt(""));

        let mut local = self.history.lock().unwrap().clone();
        local.push(llm::Message::User(user_text.to_string()));

        for _ in 0..self.config.llm.max_tool_calls {
            let mut messages = vec![system.clone()];
            messages.extend(local.clone());

            match self
                .llm
                .chat(&messages, &tools)
                .await
                .map_err(AgentError::Llm)?
            {
                llm::LlmResponse::Text(text) => {
                    local.push(llm::Message::Assistant(llm::AssistantContent::Text(
                        text.clone(),
                    )));
                    self.replace_history(local);
                    return Ok(text);
                }
                llm::LlmResponse::ToolCalls(calls) => {
                    local.push(llm::Message::Assistant(llm::AssistantContent::ToolCalls(
                        calls.clone(),
                    )));
                    for call in &calls {
                        let output = self
                            .registry
                            .call(&call.name, call.arguments.clone())
                            .await
                            .map_err(AgentError::Tool)?;
                        local.push(llm::Message::ToolResult {
                            call_id: call.id.clone(),
                            content: output,
                        });
                    }
                }
            }
        }

        Err(AgentError::TooManyToolCalls)
    }

    fn build_system_prompt(&self, skills_index: &str) -> String {
        let now = jiff::Zoned::now()
            .round(Unit::Second)
            .unwrap_or_else(|_| jiff::Zoned::now());
        let tz = now.time_zone().iana_name().unwrap_or("unknown");
        let skills = if skills_index.is_empty() {
            "（スキルはまだ登録されていません）"
        } else {
            skills_index
        };
        format!(
            "あなたは「takusu（タスク）」の音声アシスタントです。ユーザーのスケジュールとタスクを管理し、日本語で応答してください。\n\
            タスクや習慣を参照・作成・更新する際は、必ず `display_id` を使用してください。\n\n\
            現在日時: {now}\n\
            タイムゾーン: {tz}\n\n\
            ## 使用可能なスキル\n\
            {skills}\n\n\
            【指示】\n\
            - 推定値が明示されていない場合は `create_task` を呼ぶ前に `similar_tasks` を呼んで見積もりを調整してください。\n\
            - ユーザーの入力に含まれる不明な固有名詞は `memory_save` で保存してください。\n\
            - タスク参照には `display_id` を使い、UUID は使わないでください。"
        )
    }

    fn replace_history(&self, mut local: Vec<llm::Message>) {
        let max = self.config.llm.max_history;
        if local.len() > max {
            let target = local.len() - max;
            // Trim from the front, but never split a tool-call/tool-result group.
            // Safe boundaries are messages that start a new turn (user/system).
            let start = local
                .iter()
                .enumerate()
                .skip(target)
                .find(|(_, m)| matches!(m, llm::Message::User(_) | llm::Message::System(_)))
                .map(|(i, _)| i)
                .or_else(|| {
                    local
                        .iter()
                        .enumerate()
                        .take(target)
                        .rfind(|(_, m)| {
                            matches!(m, llm::Message::User(_) | llm::Message::System(_))
                        })
                        .map(|(i, _)| i)
                })
                .unwrap_or_default();
            local.drain(0..start);
        }
        let mut guard = self.history.lock().unwrap();
        *guard = local;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct EchoTool {
        calls: Arc<Mutex<usize>>,
    }

    #[async_trait::async_trait]
    impl Tool for EchoTool {
        fn name(&self) -> &'static str {
            "echo"
        }

        fn description(&self) -> &'static str {
            "echoes back the input message"
        }

        fn parameters_schema(&self) -> Value {
            json!({
                "type": "object",
                "properties": {
                    "message": {"type": "string"}
                },
                "required": ["message"]
            })
        }

        async fn call(&self, args: Value) -> Result<String, ToolError> {
            let msg = args
                .get("message")
                .and_then(|v| v.as_str())
                .ok_or_else(|| ToolError::InvalidArgs("missing message".to_string()))?;
            *self.calls.lock().unwrap() += 1;
            Ok(msg.to_string())
        }
    }

    struct MockLlm {
        calls: Mutex<Vec<(Vec<llm::Message>, Vec<Value>)>>,
        responses: Mutex<Vec<llm::LlmResponse>>,
    }

    #[async_trait::async_trait]
    impl llm::LlmClient for MockLlm {
        async fn chat(
            &self,
            messages: &[llm::Message],
            tools: &[Value],
        ) -> Result<llm::LlmResponse, llm::LlmError> {
            self.calls
                .lock()
                .unwrap()
                .push((messages.to_vec(), tools.to_vec()));
            let resp = self.responses.lock().unwrap().remove(0).clone();
            Ok(resp)
        }
    }

    #[tokio::test]
    async fn run_turn_calls_tool_and_returns_final_text() {
        let calls = Arc::new(Mutex::new(0));
        let mut registry = ToolRegistry::new();
        registry.register(Box::new(EchoTool {
            calls: calls.clone(),
        }));

        let mock = MockLlm {
            calls: Mutex::new(Vec::new()),
            responses: Mutex::new(vec![
                llm::LlmResponse::ToolCalls(vec![llm::ToolCall {
                    id: "call_1".to_string(),
                    name: "echo".to_string(),
                    arguments: json!({"message": "hello"}),
                }]),
                llm::LlmResponse::Text("done".to_string()),
            ]),
        };

        let agent = Agent::new(AgentConfig::default(), registry, mock);
        let answer = agent.run_turn("call echo").await.unwrap();

        assert_eq!(answer, "done");
        assert_eq!(*calls.lock().unwrap(), 1);
    }

    #[tokio::test]
    async fn history_is_trimmed_to_max_history() {
        let registry = ToolRegistry::new();
        let mut mock_responses = Vec::new();
        for i in 0..100 {
            mock_responses.push(llm::LlmResponse::Text(format!("reply {i}")));
        }
        let mock = MockLlm {
            calls: Mutex::new(Vec::new()),
            responses: Mutex::new(mock_responses),
        };
        let mut cfg = AgentConfig::default();
        cfg.llm.max_history = 4;
        let agent = Agent::new(cfg, registry, mock);
        for i in 0..100 {
            let _ = agent.run_turn(&format!("turn {i}")).await.unwrap();
        }
        let history = agent.history.lock().unwrap();
        assert_eq!(history.len(), 4);
        assert!(matches!(
            history.last(),
            Some(llm::Message::Assistant(llm::AssistantContent::Text(t))) if t == "reply 99"
        ));
    }

    #[tokio::test]
    async fn history_trim_keeps_tool_call_pairs_together() {
        let mut registry = ToolRegistry::new();
        registry.register(Box::new(EchoTool {
            calls: Arc::new(Mutex::new(0)),
        }));

        let mut responses = Vec::new();
        for i in 0..5 {
            responses.push(llm::LlmResponse::ToolCalls(vec![llm::ToolCall {
                id: format!("call_{i}"),
                name: "echo".to_string(),
                arguments: json!({"message": "hello"}),
            }]));
            responses.push(llm::LlmResponse::Text(format!("done {i}")));
        }

        let mock = MockLlm {
            calls: Mutex::new(Vec::new()),
            responses: Mutex::new(responses),
        };
        let mut cfg = AgentConfig::default();
        cfg.llm.max_history = 5;
        let agent = Agent::new(cfg, registry, mock);

        for i in 0..5 {
            let _ = agent.run_turn(&format!("turn {i}")).await.unwrap();
        }

        let history = agent.history.lock().unwrap();
        assert!(history.len() <= 5);
        assert!(matches!(&history[0], llm::Message::User(_)));
        if let llm::Message::Assistant(llm::AssistantContent::ToolCalls(calls)) = &history[1] {
            assert_eq!(calls.len(), 1);
            assert!(matches!(
                &history[2],
                llm::Message::ToolResult { call_id, .. } if call_id == &calls[0].id
            ));
        } else {
            panic!("expected assistant tool-calls message at index 1");
        }
    }
}
