use clap::Parser;
use std::process;
use takusu_agent::audio::AudioAdapter;
use takusu_agent::llm::OpenAIClient;
use takusu_agent::tools::takusu::register_read_tools;
use takusu_agent::{AgentConfig, AgentSession, ToolRegistry};
use takusu_client::Client;

#[derive(Parser)]
#[command(name = "takusu-agent", about = "takusu voice assistant agent")]
struct Cli {
    /// Text input for a single agent turn. If omitted, run push-to-talk audio mode.
    #[arg(long, short = 't')]
    text: Option<String>,

    /// Disable TTS in audio mode.
    #[arg(long)]
    no_tts: bool,
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
    let mut registry = ToolRegistry::new();
    let client = Client::new(&config.server.url, &config.server.token);
    register_read_tools(&mut registry, client);
    let session = AgentSession::new(config, registry, llm);

    if let Some(text) = cli.text {
        let result = session.run_turn(&text).await?;

        println!("{}", result.text);
        if !result.changes.is_empty() {
            let changes = serde_json::to_string_pretty(&result.changes)?;
            eprintln!("{changes}");
        }
        if result.schedule_dirty {
            eprintln!("schedule dirty: true");
        }
    } else {
        let adapter = AudioAdapter::new(session)?;
        adapter.run(cli.no_tts).await?;
    }

    Ok(())
}
