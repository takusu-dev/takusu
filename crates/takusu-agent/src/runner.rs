use std::sync::Arc;

use crate::llm::OpenAIClient;
use crate::tools::takusu::register_tools;
use crate::{
    AgentConfig, AgentError, AgentSession, StubUserInputProvider, ToolRegistry, TurnResult,
    UserInputProvider,
};
use takusu_client::Client;

pub fn build_session(config: &AgentConfig, client: Client) -> Result<AgentSession, AgentError> {
    build_session_with_provider(config, client, Arc::new(StubUserInputProvider))
}

pub fn build_session_with_provider(
    config: &AgentConfig,
    client: Client,
    user_input_provider: Arc<dyn UserInputProvider>,
) -> Result<AgentSession, AgentError> {
    let llm = OpenAIClient::new(config.llm.clone())?;
    let mut registry = ToolRegistry::new();
    register_tools(&mut registry, client.clone(), user_input_provider);
    Ok(AgentSession::new_with_client(
        config.clone(),
        client,
        registry,
        llm,
    ))
}

pub async fn run_text(session: &AgentSession, text: &str) -> Result<TurnResult, AgentError> {
    session.run_turn(text).await
}

#[cfg(feature = "audio-device")]
pub async fn run_audio(
    session: AgentSession,
    no_tts: bool,
) -> Result<(), crate::audio::AudioError> {
    use crate::audio::AudioAdapter;
    let adapter = AudioAdapter::new(session)?;
    adapter.run(no_tts).await
}
