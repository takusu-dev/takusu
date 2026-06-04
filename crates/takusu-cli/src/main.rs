mod config;
mod display_rich;
mod display_simple;
mod editor;

use clap::{CommandFactory, Parser, Subcommand};
use std::io::{self, Write};
use std::process;
use takusu_client::{
    Client, CreateTask, GenerateSchedule, MoveEntry, Reschedule, ScheduleEntry, TaskQuery,
    UpdateSyncSettings,
};
use takusu_util::{generate_root_token, parse_datetime_tz, parse_duration, parse_range_tz};

fn prompt(label: &str) -> String {
    print!("{label}: ");
    io::stdout().flush().unwrap();
    let mut buf = String::new();
    io::stdin().read_line(&mut buf).unwrap();
    buf.trim().to_string()
}

fn is_interactive() -> bool {
    atty::is(atty::Stream::Stdin) && atty::is(atty::Stream::Stdout)
}

fn parse_dt(s: &str, tz: &jiff::tz::TimeZone) -> Result<String, takusu_client::ClientError> {
    parse_datetime_tz(s, tz).map_err(|e| takusu_client::ClientError::Api { status: 0, body: e })
}

#[derive(Parser)]
#[command(name = "takusu", version, about = "CLI client for takusu scheduler")]
struct Cli {
    #[arg(long, env = "TAKUSU_URL", global = true)]
    url: Option<String>,

    #[arg(long, env = "TAKUSU_TOKEN", global = true)]
    token: Option<String>,

    #[arg(long, env = "TAKUSU_TIMEZONE", global = true)]
    tz: Option<String>,

    #[arg(long, default_value = "rich", global = true)]
    mode: DisplayMode,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Clone, clap::ValueEnum)]
enum DisplayMode {
    Rich,
    Simple,
}

#[derive(Subcommand)]
enum Commands {
    /// Check server health (no token required)
    Health,

    /// Generate a root token for takusu-serve
    GenRootToken,

    /// Task management
    Task {
        #[command(subcommand)]
        command: TaskCommands,
    },

    /// Schedule management
    Schedule {
        #[command(subcommand)]
        command: ScheduleCommands,
    },

    /// Token management
    Token {
        #[command(subcommand)]
        command: TokenCommands,
    },

    /// Generate shell completions
    Completion {
        #[arg(value_name = "SHELL")]
        shell: clap_complete::Shell,
    },

    /// Google Calendar sync
    Sync {
        #[command(subcommand)]
        command: SyncCommands,
    },

    /// Show or initialize config file
    Config {
        #[command(subcommand)]
        command: ConfigCommands,
    },
}

#[derive(Subcommand)]
enum ConfigCommands {
    /// Show config file path and contents
    Show,
    /// Initialize config file with defaults
    Init,
}

#[derive(Subcommand)]
enum TaskCommands {
    /// List tasks
    #[command(visible_alias = "ls")]
    List {
        #[arg(long)]
        status: Option<String>,
        #[arg(
            long,
            help = "Filter by start date (e.g. 2025-06-05, 2025-06-05T14:00)"
        )]
        from: Option<String>,
        #[arg(long, help = "Filter by end date (e.g. 2025-06-05, 2025-06-05T14:00)")]
        until: Option<String>,
        #[arg(long)]
        habit_id: Option<String>,
    },

    /// Show task detail
    #[command(visible_alias = "get")]
    Show { id: String },

    /// Create a task (interactive if no args in terminal)
    Create {
        #[arg(short, long, help = "Task title")]
        title: Option<String>,
        #[arg(
            short,
            long,
            help = "Deadline (e.g. 2025-06-05, 2025-06-05T23:59, 2025-06-05T23:59:00Z)"
        )]
        end_at: Option<String>,
        #[arg(long, help = "Start time (same format as end_at)")]
        start_at: Option<String>,
        #[arg(
            long,
            default_value = "30m",
            help = "Average duration (e.g. 30m, 1h30m, 6s=6slots(30min))"
        )]
        avg_time: String,
        #[arg(
            long,
            default_value = "0",
            help = "Std dev of duration (same format as avg_time)"
        )]
        sigma_time: String,
        #[arg(long, default_value_t = 0.5, help = "Abandonability 0.0-1.0")]
        abandonability: f64,
        #[arg(long)]
        description: Option<String>,
        #[arg(long)]
        depends: Option<Vec<String>>,
    },

    /// Edit a task in $EDITOR
    Edit { id: String },

    /// Partially update a task (PATCH)
    Update {
        id: String,
        #[arg(long)]
        title: Option<String>,
        #[arg(long)]
        description: Option<String>,
        #[arg(long, help = "Start time (e.g. 2025-06-05, 2025-06-05T14:00)")]
        start_at: Option<String>,
        #[arg(long, help = "Deadline (e.g. 2025-06-05, 2025-06-05T14:00)")]
        end_at: Option<String>,
        #[arg(long, help = "Average duration (e.g. 30m, 1h30m, 6s=6slots)")]
        avg_time: Option<String>,
        #[arg(long, help = "Std dev of duration (same format as avg_time)")]
        sigma_time: Option<String>,
        #[arg(long)]
        depends: Option<Vec<String>>,
        #[arg(long)]
        parallelizable: Option<bool>,
        #[arg(long)]
        allows_parallel: Option<bool>,
        #[arg(long)]
        abandonability: Option<f64>,
        #[arg(long)]
        status: Option<String>,
    },

    /// Full replace a task (PUT)
    Replace {
        id: String,
        #[arg(long)]
        title: String,
        #[arg(long, help = "Deadline (e.g. 2025-06-05, 2025-06-05T23:59Z)")]
        end_at: String,
        #[arg(long, help = "Start time (same format as end_at)")]
        start_at: Option<String>,
        #[arg(
            long,
            default_value = "30m",
            help = "Average duration (e.g. 30m, 1h30m, 6s=6slots)"
        )]
        avg_time: String,
        #[arg(
            long,
            default_value = "0",
            help = "Std dev of duration (same format as avg_time)"
        )]
        sigma_time: String,
        #[arg(long, default_value_t = 0.5)]
        abandonability: f64,
        #[arg(long)]
        description: Option<String>,
        #[arg(long)]
        depends: Option<Vec<String>>,
    },

    /// Delete a task
    #[command(visible_alias = "rm")]
    Delete { id: String },
}

#[derive(Subcommand)]
enum ScheduleCommands {
    /// Get active schedule
    Get,

    /// Generate a new schedule
    Generate {
        #[arg(long, help = "Time range (e.g. '1w', '3d', 'now to 2025-06-12')")]
        range: Option<String>,
        #[arg(long, help = "Start time (e.g. 2025-06-05, 2025-06-05T06:00Z, now)")]
        from: Option<String>,
        #[arg(long, help = "End time (e.g. 2025-06-06, 2025-06-06T06:00Z, now)")]
        until: Option<String>,
        #[arg(long)]
        task_ids: Option<Vec<String>>,
        #[arg(long, default_value = "recommended")]
        sleep: String,
    },

    /// Reschedule (partial)
    Reschedule {
        #[arg(long)]
        mode: String,
        #[arg(long, help = "Start time (e.g. 2025-06-05, 2025-06-05T06:00Z)")]
        from: Option<String>,
        #[arg(long, help = "End time (e.g. 2025-06-06, 2025-06-06T06:00Z)")]
        until: Option<String>,
        #[arg(long)]
        task_ids: Option<Vec<String>>,
        #[arg(long)]
        pinned: Option<Vec<String>>,
        #[arg(long, default_value = "recommended")]
        sleep: String,
    },

    /// Move a schedule entry
    Move {
        task_id: String,
        #[arg(
            long,
            help = "New start time (e.g. 2025-06-05T14:00, 2025-06-05T14:00:00Z)"
        )]
        start_at: String,
        #[arg(long, default_value_t = false)]
        force: bool,
    },

    /// Clear active schedule
    Clear,
}

#[derive(Subcommand)]
enum TokenCommands {
    /// Issue a new token
    Create {
        #[arg(long)]
        label: Option<String>,
    },

    /// List tokens
    #[command(visible_alias = "ls")]
    List,

    /// Revoke a token
    Revoke { id: i64 },
}

#[derive(Subcommand)]
enum SyncCommands {
    /// Show Google Calendar sync settings
    Settings,

    /// Update Google Calendar sync settings
    Setup {
        #[arg(long)]
        enabled: Option<bool>,
        #[arg(long)]
        calendar_id: Option<String>,
        #[arg(long)]
        client_id: Option<String>,
        #[arg(long)]
        client_secret: Option<String>,
        #[arg(long)]
        refresh_token: Option<String>,
    },

    /// Generate Google OAuth2 authorization URL
    OauthUrl {
        #[arg(long)]
        redirect_uri: String,
    },

    /// Complete OAuth2 callback with authorization code
    OauthCallback {
        #[arg(long)]
        code: String,
        #[arg(long)]
        redirect_uri: String,
    },

    /// Manually trigger Google Calendar sync
    Trigger,
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    let cfg = config::load();

    let url = cli
        .url
        .or(cfg.url)
        .unwrap_or_else(|| "http://127.0.0.1:3000".into());
    let tz_str = cli.tz.or(cfg.tz).unwrap_or_else(|| "UTC".into());

    if matches!(cli.command, Commands::GenRootToken) {
        let token = generate_root_token();
        println!("{token}");
        eprintln!("\nSet this as TAKUSU_ROOT_TOKEN env var for takusu-serve.");
        return;
    }

    if matches!(cli.command, Commands::Completion { .. }) {
        let shell = match cli.command {
            Commands::Completion { shell } => shell,
            _ => unreachable!(),
        };
        let mut cmd = Cli::command();
        clap_complete::generate(shell, &mut cmd, "takusu", &mut io::stdout());
        return;
    }

    let needs_token = !matches!(cli.command, Commands::Health | Commands::Config { .. });

    let token = if needs_token {
        match cli.token.or(cfg.token) {
            Some(t) => t,
            None => {
                eprintln!("Error: token required (--token, TAKUSU_TOKEN, or config)");
                process::exit(1);
            }
        }
    } else {
        String::new()
    };

    let client = Client::new(&url, &token);

    let tz = jiff::tz::TimeZone::get(&tz_str).unwrap_or_else(|_| {
        eprintln!("Error: invalid timezone '{tz_str}' (e.g. Asia/Tokyo)");
        process::exit(1);
    });

    if let Err(e) = run(cli.mode, &client, tz, cli.command).await {
        eprintln!("Error: {e}");
        process::exit(1);
    }
}

async fn run(
    mode: DisplayMode,
    client: &Client,
    tz: jiff::tz::TimeZone,
    cmd: Commands,
) -> Result<(), takusu_client::ClientError> {
    match cmd {
        Commands::Health => {
            let resp = client.health().await?;
            println!("{resp}");
        }
        Commands::Task { command } => run_task(mode, client, &tz, command).await?,
        Commands::Schedule { command } => run_schedule(mode, client, &tz, command).await?,
        Commands::Token { command } => run_token(mode, client, command).await?,
        Commands::Sync { command } => run_sync(client, command).await?,
        Commands::GenRootToken => unreachable!(),
        Commands::Completion { .. } => unreachable!(),
        Commands::Config { command } => run_config(command)?,
    }
    Ok(())
}

async fn run_task(
    mode: DisplayMode,
    client: &Client,
    tz: &jiff::tz::TimeZone,
    cmd: TaskCommands,
) -> Result<(), takusu_client::ClientError> {
    match cmd {
        TaskCommands::List {
            status,
            from,
            until,
            habit_id,
        } => {
            let query = TaskQuery {
                status,
                from: from.map(|s| parse_dt(&s, tz)).transpose()?,
                until: until.map(|s| parse_dt(&s, tz)).transpose()?,
                habit_id,
            };
            let tasks = client.list_tasks(&query).await?;
            match mode {
                DisplayMode::Rich => display_rich::display_tasks(&tasks, tz),
                DisplayMode::Simple => display_simple::display_tasks(&tasks, tz),
            }
        }
        TaskCommands::Show { id } => {
            let task = client.get_task(&id).await?;
            let entry = match client.get_schedule().await {
                Ok(schedule) => {
                    let entries: Vec<ScheduleEntry> =
                        serde_json::from_str(&schedule.schedule).unwrap_or_default();
                    entries.into_iter().find(|e| e.task_id == task.id)
                }
                Err(_) => None,
            };
            match mode {
                DisplayMode::Rich => display_rich::display_task_detail(&task, entry.as_ref(), tz),
                DisplayMode::Simple => {
                    display_simple::display_task_detail(&task, entry.as_ref(), tz)
                }
            }
        }
        TaskCommands::Create {
            title,
            end_at,
            start_at,
            avg_time,
            sigma_time,
            abandonability,
            description,
            depends,
        } => {
            let (title, end_at) = if is_interactive() && title.is_none() && end_at.is_none() {
                let t = prompt("Title");
                let e = prompt("End at (e.g. 2025-06-05 or 2025-06-05T23:59)");
                (Some(t), Some(e))
            } else {
                (title, end_at)
            };
            let avg_minutes = parse_duration(&avg_time)
                .map_err(|e| takusu_client::ClientError::Api { status: 0, body: e })?;
            let sigma_minutes: i64 = parse_duration(&sigma_time)
                .map_err(|e| takusu_client::ClientError::Api { status: 0, body: e })?;
            let body = CreateTask {
                title: title.unwrap_or_default(),
                end_at: parse_dt(&end_at.unwrap_or_default(), tz)?,
                start_at: start_at.map(|s| parse_dt(&s, tz)).transpose()?,
                avg_minutes,
                sigma_minutes: if sigma_minutes > 0 {
                    Some(sigma_minutes)
                } else {
                    None
                },
                depends,
                parallelizable: None,
                allows_parallel: None,
                abandonability: Some(abandonability),
                description,
            };
            let task = client.create_task(&body).await?;
            match mode {
                DisplayMode::Rich => display_rich::display_tasks(&[task], tz),
                DisplayMode::Simple => display_simple::display_tasks(&[task], tz),
            }
        }
        TaskCommands::Edit { id } => {
            let task = client.get_task(&id).await?;
            let original = editor::format_task_for_editing(&task);
            let edited = editor::open_editor(&original, &task.id).map_err(|e| {
                takusu_client::ClientError::Api {
                    status: 0,
                    body: e.to_string(),
                }
            })?;
            let update = editor::parse_edited_task(&edited).ok_or_else(|| {
                takusu_client::ClientError::Api {
                    status: 0,
                    body: "failed to parse edited task".to_string(),
                }
            })?;
            let updated = client.update_task(&id, &update).await?;
            match mode {
                DisplayMode::Rich => display_rich::display_tasks(&[updated], tz),
                DisplayMode::Simple => display_simple::display_tasks(&[updated], tz),
            }
        }
        TaskCommands::Update {
            id,
            title,
            description,
            start_at,
            end_at,
            avg_time,
            sigma_time,
            depends,
            parallelizable,
            allows_parallel,
            abandonability,
            status,
        } => {
            let avg_minutes = avg_time
                .as_ref()
                .map(|s| parse_duration(s))
                .transpose()
                .map_err(|e| takusu_client::ClientError::Api { status: 0, body: e })?;
            let sigma_minutes = sigma_time
                .as_ref()
                .map(|s| parse_duration(s))
                .transpose()
                .map_err(|e| takusu_client::ClientError::Api { status: 0, body: e })?;
            let body = takusu_client::UpdateTask {
                title,
                description,
                start_at: start_at.map(|s| parse_dt(&s, tz)).transpose()?,
                end_at: end_at.map(|s| parse_dt(&s, tz)).transpose()?,
                avg_minutes,
                sigma_minutes,
                depends,
                parallelizable,
                allows_parallel,
                abandonability,
                status,
            };
            let task = client.update_task(&id, &body).await?;
            match mode {
                DisplayMode::Rich => display_rich::display_tasks(&[task], tz),
                DisplayMode::Simple => display_simple::display_tasks(&[task], tz),
            }
        }
        TaskCommands::Replace {
            id,
            title,
            end_at,
            start_at,
            avg_time,
            sigma_time,
            abandonability,
            description,
            depends,
        } => {
            let avg_minutes = parse_duration(&avg_time)
                .map_err(|e| takusu_client::ClientError::Api { status: 0, body: e })?;
            let sigma_minutes: i64 = parse_duration(&sigma_time)
                .map_err(|e| takusu_client::ClientError::Api { status: 0, body: e })?;
            let body = CreateTask {
                title,
                end_at: parse_dt(&end_at, tz)?,
                start_at: start_at.map(|s| parse_dt(&s, tz)).transpose()?,
                avg_minutes,
                sigma_minutes: if sigma_minutes > 0 {
                    Some(sigma_minutes)
                } else {
                    None
                },
                depends,
                parallelizable: None,
                allows_parallel: None,
                abandonability: Some(abandonability),
                description,
            };
            let task = client.replace_task(&id, &body).await?;
            match mode {
                DisplayMode::Rich => display_rich::display_tasks(&[task], tz),
                DisplayMode::Simple => display_simple::display_tasks(&[task], tz),
            }
        }
        TaskCommands::Delete { id } => {
            client.delete_task(&id).await?;
            println!("Task {id} deleted.");
        }
    }
    Ok(())
}

async fn run_schedule(
    mode: DisplayMode,
    client: &Client,
    tz: &jiff::tz::TimeZone,
    cmd: ScheduleCommands,
) -> Result<(), takusu_client::ClientError> {
    match cmd {
        ScheduleCommands::Get => {
            let schedule = client.get_schedule().await?;
            let entries: Vec<ScheduleEntry> =
                serde_json::from_str(&schedule.schedule).unwrap_or_default();
            let tasks = client
                .list_tasks(&TaskQuery::default())
                .await
                .unwrap_or_default();
            match mode {
                DisplayMode::Rich => display_rich::display_schedule(&entries, &tasks, tz),
                DisplayMode::Simple => display_simple::display_schedule(&entries, &tasks, tz),
            }
        }
        ScheduleCommands::Generate {
            range,
            from,
            until,
            task_ids,
            sleep,
        } => {
            let (from, until) = if let Some(range) = range {
                parse_range_tz(&range, tz)
                    .map_err(|e| takusu_client::ClientError::Api { status: 0, body: e })?
            } else {
                let from = match from {
                    Some(s) => parse_dt(&s, tz)?,
                    None => jiff::Timestamp::now().to_string(),
                };
                let until = match until {
                    Some(s) => parse_dt(&s, tz)?,
                    None => {
                        let now = jiff::Timestamp::now();
                        let until_secs = now.as_second().saturating_add(7 * 86400);
                        jiff::Timestamp::from_second(until_secs)
                            .unwrap_or(now)
                            .to_string()
                    }
                };
                (from, until)
            };
            let body = GenerateSchedule {
                from: parse_dt(&from, tz)?,
                until: parse_dt(&until, tz)?,
                task_ids,
                sleep,
            };
            let schedule = client.generate_schedule(&body).await?;
            let entries: Vec<ScheduleEntry> =
                serde_json::from_str(&schedule.schedule).unwrap_or_default();
            let tasks = client
                .list_tasks(&TaskQuery::default())
                .await
                .unwrap_or_default();
            match mode {
                DisplayMode::Rich => display_rich::display_schedule(&entries, &tasks, tz),
                DisplayMode::Simple => display_simple::display_schedule(&entries, &tasks, tz),
            }
        }
        ScheduleCommands::Reschedule {
            mode: rmode,
            from,
            until,
            task_ids,
            pinned,
            sleep,
        } => {
            let body = Reschedule {
                mode: rmode,
                from: from.map(|s| parse_dt(&s, tz)).transpose()?,
                until: until.map(|s| parse_dt(&s, tz)).transpose()?,
                task_ids,
                pinned: pinned.unwrap_or_default(),
                sleep,
            };
            let schedule = client.reschedule(&body).await?;
            let entries: Vec<ScheduleEntry> =
                serde_json::from_str(&schedule.schedule).unwrap_or_default();
            let tasks = client
                .list_tasks(&TaskQuery::default())
                .await
                .unwrap_or_default();
            match mode {
                DisplayMode::Rich => display_rich::display_schedule(&entries, &tasks, tz),
                DisplayMode::Simple => display_simple::display_schedule(&entries, &tasks, tz),
            }
        }
        ScheduleCommands::Move {
            task_id,
            start_at,
            force,
        } => {
            let body = MoveEntry {
                start_at: parse_dt(&start_at, tz)?,
                force,
            };
            let result = client.move_entry(&task_id, &body).await?;
            println!("{}", serde_json::to_string_pretty(&result).unwrap());
        }
        ScheduleCommands::Clear => {
            client.clear_schedule().await?;
            println!("Schedule cleared.");
        }
    }
    Ok(())
}

async fn run_token(
    mode: DisplayMode,
    client: &Client,
    cmd: TokenCommands,
) -> Result<(), takusu_client::ClientError> {
    match cmd {
        TokenCommands::Create { label } => {
            let resp = client.create_token(label.as_deref()).await?;
            println!("Token issued:");
            println!("  ID:    {}", resp.id);
            println!("  Token: {}", resp.token);
            println!("  Label: {}", resp.label.as_deref().unwrap_or("—"));
            println!("  Created: {}", resp.created_at);
            eprintln!("\n⚠ Save the token value; it won't be shown again.");
        }
        TokenCommands::List => {
            let tokens = client.list_tokens().await?;
            match mode {
                DisplayMode::Rich => display_rich::display_tokens(&tokens),
                DisplayMode::Simple => display_simple::display_tokens(&tokens),
            }
        }
        TokenCommands::Revoke { id } => {
            client.revoke_token(id).await?;
            println!("Token {id} revoked.");
        }
    }
    Ok(())
}

async fn run_sync(client: &Client, cmd: SyncCommands) -> Result<(), takusu_client::ClientError> {
    match cmd {
        SyncCommands::Settings => {
            let settings = client.get_sync_settings().await?;
            println!("Google Calendar sync settings:");
            println!("  enabled:          {}", settings.enabled);
            println!("  calendar_id:      {}", settings.calendar_id);
            println!("  client_id:        {}", settings.client_id);
            println!("  has_client_secret: {}", settings.has_client_secret);
            println!("  has_refresh_token:  {}", settings.has_refresh_token);
        }
        SyncCommands::Setup {
            enabled,
            calendar_id,
            client_id,
            client_secret,
            refresh_token,
        } => {
            let body = UpdateSyncSettings {
                enabled,
                calendar_id,
                client_id,
                client_secret,
                refresh_token,
            };
            let settings = client.update_sync_settings(&body).await?;
            println!("Sync settings updated:");
            println!("  enabled:           {}", settings.enabled);
            println!("  calendar_id:      {}", settings.calendar_id);
            println!("  has_client_secret: {}", settings.has_client_secret);
            println!("  has_refresh_token:  {}", settings.has_refresh_token);
        }
        SyncCommands::OauthUrl { redirect_uri } => {
            let result = client.get_oauth_url(&redirect_uri).await?;
            println!("{}", serde_json::to_string_pretty(&result).unwrap());
        }
        SyncCommands::OauthCallback { code, redirect_uri } => {
            let result = client.oauth_callback(&code, &redirect_uri).await?;
            println!("{}", serde_json::to_string_pretty(&result).unwrap());
        }
        SyncCommands::Trigger => {
            let result = client.trigger_sync().await?;
            println!("{}", serde_json::to_string_pretty(&result).unwrap());
        }
    }
    Ok(())
}

fn run_config(cmd: ConfigCommands) -> Result<(), takusu_client::ClientError> {
    match cmd {
        ConfigCommands::Show => config::show(),
        ConfigCommands::Init => config::init(),
    }
    Ok(())
}
