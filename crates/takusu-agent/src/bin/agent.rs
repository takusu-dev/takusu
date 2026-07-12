use clap::Parser;
use std::process;
use takusu_agent::llm::OpenAIClient;
use takusu_agent::{AgentConfig, AgentSession, ToolRegistry};

#[derive(Parser)]
#[command(name = "takusu-agent", about = "takusu voice assistant agent")]
struct Cli {
    /// Text input for a single agent turn.
    #[arg(long, short = 't')]
    text: String,
}

#[tokio::main]
async fn main() {
    if let Err(e) = run().await {
        eprintln!("error: {e}");
        process::exit(1);
    }
}

async fn run() -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>> {
    let cli = Cli::parse();
    let config = AgentConfig::load()?;
    let llm = OpenAIClient::new(config.llm.clone())?;
    let registry = ToolRegistry::new();
    let session = AgentSession::new(config, registry, llm);
    let result = session.run_turn(&cli.text).await?;

    println!("{}", result.text);
    if !result.changes.is_empty() {
        let changes = serde_json::to_string_pretty(&result.changes)?;
        eprintln!("{changes}");
    }
    if result.schedule_dirty {
        eprintln!("schedule dirty: true");
    }

    Ok(())
}
