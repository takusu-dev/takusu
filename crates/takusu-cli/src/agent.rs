use std::io::{self, BufRead, Write};
use std::sync::Arc;

use takusu_agent::{AgentConfig, AgentError, AgentSession, ApprovalRequest};
use takusu_client::Client;
use takusu_local_lib::app::TakusuApp;
use takusu_local_lib::error::AppError;

use crate::server::start_in_process;

pub async fn run(app: Arc<TakusuApp>, text: Option<String>, yes: bool) -> Result<(), AppError> {
    let local_server = start_in_process(app).await?;
    let mut config = AgentConfig::load()
        .map_err(|e| AppError::Internal(format!("failed to load agent config: {e}")))?;
    config.server.url = local_server.url;
    config.server.token = local_server.token;

    let client = Client::new(&config.server.url, &config.server.token);
    let session = takusu_agent::runner::build_session(&config, client)
        .map_err(|e| AppError::Internal(format!("failed to build agent session: {e}")))?;

    if let Some(text) = text {
        run_text(&session, &text, yes).await
    } else {
        run_repl(&session, yes).await
    }
}

async fn run_text(session: &AgentSession, text: &str, yes: bool) -> Result<(), AppError> {
    let result = session.run_turn(text).await.map_err(agent_err)?;
    println!("{}", result.text);

    let schedule_dirty = if let Some(approval) = result.approval_request.as_ref() {
        display_approval(approval);
        let approve = if yes {
            true
        } else {
            ask_approve("Approve? (y/N): ")?
        };
        let res = session
            .resolve_approval(&approval.id, approve)
            .await
            .map_err(agent_err)?;
        if res.approved {
            println!("approved {} change(s)", res.changes.len());
            for receipt in &res.changes {
                println!(
                    "  {} {}: {}",
                    receipt.operation, receipt.target_type, receipt.target_id
                );
            }
        } else {
            println!("denied");
        }
        res.schedule_dirty
    } else {
        if !result.changes.is_empty() {
            eprintln!("changes:");
            for receipt in &result.changes {
                eprintln!(
                    "  {} {}: {}",
                    receipt.operation, receipt.target_type, receipt.target_id
                );
            }
        }
        result.schedule_dirty
    };

    if schedule_dirty {
        eprintln!("schedule dirty: true");
    }

    Ok(())
}

fn display_approval(req: &ApprovalRequest) {
    if !req.why.is_empty() {
        println!("Why: {}", req.why);
    }
    if !req.inferred_fields.is_empty() {
        println!("Inferred:");
        for field in &req.inferred_fields {
            println!("  {} = {} ({})", field.field, field.value, field.reason);
        }
    }
    if !req.warnings.is_empty() {
        println!("Warnings:");
        for warning in &req.warnings {
            println!("  - {warning}");
        }
    }
    println!("Changes:");
    for change in &req.changes {
        println!(
            "  {} {}: {}",
            change.operation, change.target_label, change.description
        );
    }
    println!("expires at: {}", req.expires_at);
}

fn ask_approve(label: &str) -> Result<bool, AppError> {
    print!("{label}");
    io::stdout()
        .flush()
        .map_err(|e| AppError::Internal(e.to_string()))?;
    let stdin = io::stdin();
    let line = stdin
        .lock()
        .lines()
        .next()
        .transpose()
        .map_err(|e| AppError::Internal(e.to_string()))?;
    Ok(line
        .map(|l| {
            let l = l.trim().to_lowercase();
            l == "y" || l == "yes"
        })
        .unwrap_or(false))
}

async fn run_repl(session: &AgentSession, yes: bool) -> Result<(), AppError> {
    loop {
        print!("> ");
        io::stdout()
            .flush()
            .map_err(|e| AppError::Internal(e.to_string()))?;
        let mut line = String::new();
        let n = io::stdin()
            .read_line(&mut line)
            .map_err(|e| AppError::Internal(e.to_string()))?;
        if n == 0 {
            break;
        }
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if line == "exit" || line == "quit" {
            break;
        }
        run_text(session, line, yes).await?;
    }
    Ok(())
}

fn agent_err(e: AgentError) -> AppError {
    AppError::Internal(e.to_string())
}
