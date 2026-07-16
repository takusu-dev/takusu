mod config;
mod display_rich;
mod display_simple;
mod editor;

use clap::{CommandFactory, Parser, Subcommand};
use config::CliConfig;
use std::io::{self, Read, Write};
use std::process;
use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum::extract::{Query, State};
use axum::response::Html;
use axum::routing::get;

use takusu_local_lib::{
    app::TakusuApp,
    config::{LocalConfig, StorageKind},
    error::AppError,
    storage_sqlite::SqliteStorage,
    storage_workers::WorkersStorage,
    token_cache::TokenCache,
};
use takusu_storage::{
    CreateHabit, CreateHabitPause, CreateSkill, CreateTask, ScheduleEntry, TaskQuery, UpdateHabit,
    UpdateSettings,
};
use takusu_util::{generate_root_token, parse_datetime_tz, parse_duration};

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

fn parse_dt(s: &str, tz: &jiff::tz::TimeZone) -> Result<String, AppError> {
    parse_datetime_tz(s, tz).map_err(AppError::BadRequest)
}

#[derive(Parser)]
#[command(name = "takusu", version, about = "CLI client for takusu scheduler")]
struct Cli {
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

    /// Generate a root token
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

    /// Habit management
    Habit {
        #[command(subcommand)]
        command: HabitCommands,
    },

    /// Skill management
    Skill {
        #[command(subcommand)]
        command: SkillCommands,
    },
}

#[derive(Subcommand)]
enum ConfigCommands {
    /// Show config file path and contents
    Show,
    /// Initialize config file with defaults
    Init,
    /// Set a local config value
    Set {
        #[arg(long)]
        storage: Option<String>,
        #[arg(long)]
        db: Option<String>,
        #[arg(long)]
        worker_url: Option<String>,
        #[arg(long)]
        workers_token: Option<String>,
        #[arg(long)]
        root_token: Option<String>,
        #[arg(long)]
        tz: Option<String>,
        #[arg(long)]
        sleep_start: Option<String>,
        #[arg(long)]
        sleep_end: Option<String>,
    },
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
            help = "Std dev of duration (same format as avg_time). 0 = auto (avg/5)"
        )]
        sigma_time: String,
        #[arg(long, default_value_t = 0.5, help = "Abandonability 0.0-1.0")]
        abandonability: f64,
        #[arg(long)]
        description: Option<String>,
        #[arg(long)]
        depends: Option<Vec<String>>,
        #[arg(long)]
        parallelizable: Option<bool>,
        #[arg(long)]
        allows_parallel: Option<bool>,
        #[arg(long, help = "Lock start time (scheduler cannot move)")]
        fixed: Option<bool>,
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
        #[arg(long, help = "Lock start time (scheduler cannot move)")]
        fixed: Option<bool>,
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
            help = "Std dev of duration (same format as avg_time). 0 = auto (avg/5)"
        )]
        sigma_time: String,
        #[arg(long, default_value_t = 0.5)]
        abandonability: f64,
        #[arg(long)]
        description: Option<String>,
        #[arg(long)]
        depends: Option<Vec<String>>,
        #[arg(long)]
        parallelizable: Option<bool>,
        #[arg(long)]
        allows_parallel: Option<bool>,
        #[arg(long, help = "Lock start time (scheduler cannot move)")]
        fixed: Option<bool>,
    },

    /// Delete a task
    #[command(visible_alias = "rm")]
    Delete { id: String },

    /// Change task status (pending, scheduled, in_progress, completed, skipped)
    Status { id: String, status: String },

    /// Detect and offer to remove redundant (composite) dependency edges (#355)
    #[command(visible_alias = "deps-check")]
    DepsCheck,
}

#[derive(Subcommand)]
enum HabitCommands {
    /// List habits
    #[command(visible_alias = "ls")]
    List,

    /// Show habit detail
    #[command(visible_alias = "get")]
    Show { id: String },

    /// Create a habit (interactive if no args in terminal)
    Create {
        #[arg(short, long, help = "Habit title")]
        title: Option<String>,
        #[arg(long, short, help = "Recurrence (daily, weekdays, Mon,Wed,Fri)")]
        recurrence: Option<String>,
        #[arg(long, help = "Start time (HH:MM)")]
        start_time: Option<String>,
        #[arg(long, help = "End time (HH:MM)")]
        end_time: Option<String>,
        #[arg(
            long,
            default_value = "30m",
            help = "Average duration (e.g. 30m, 1h30m)"
        )]
        avg_time: String,
        #[arg(
            long,
            default_value = "0",
            help = "Std dev of duration (same format as avg_time). 0 = auto (avg/5)"
        )]
        sigma_time: String,
        #[arg(long, default_value_t = 0.5, help = "Abandonability 0.0-1.0")]
        abandonability: f64,
        #[arg(long)]
        description: Option<String>,
        #[arg(long)]
        parallelizable: bool,
        #[arg(long)]
        allows_parallel: bool,
        #[arg(long, help = "Lock start time (scheduler cannot move)")]
        fixed: bool,
        #[arg(
            long,
            help = "Window mode: 'day' (occurrence day) or 'period' (until next occurrence)"
        )]
        window: Option<String>,
    },

    /// Edit a habit in $EDITOR
    Edit { id: String },

    /// Partially update a habit (PATCH)
    Update {
        id: String,
        #[arg(long)]
        title: Option<String>,
        #[arg(long)]
        description: Option<String>,
        #[arg(long)]
        recurrence: Option<String>,
        #[arg(long, help = "Start time (HH:MM)")]
        start_time: Option<String>,
        #[arg(long, help = "End time (HH:MM)")]
        end_time: Option<String>,
        #[arg(long, help = "Average duration (e.g. 30m, 1h30m)")]
        avg_time: Option<String>,
        #[arg(long, help = "Std dev of duration (same format as avg_time)")]
        sigma_time: Option<String>,
        #[arg(long)]
        parallelizable: Option<bool>,
        #[arg(long)]
        allows_parallel: Option<bool>,
        #[arg(long)]
        abandonability: Option<f64>,
        #[arg(long)]
        active: Option<bool>,
        #[arg(long, help = "Lock start time (scheduler cannot move)")]
        fixed: Option<bool>,
        #[arg(
            long,
            help = "Window mode: 'day' (occurrence day) or 'period' (until next occurrence)"
        )]
        window: Option<String>,
    },

    /// Full replace a habit (PUT)
    Replace {
        id: String,
        #[arg(long)]
        title: String,
        #[arg(long)]
        recurrence: String,
        #[arg(long, help = "Start time (HH:MM)")]
        start_time: String,
        #[arg(long, help = "End time (HH:MM)")]
        end_time: String,
        #[arg(
            long,
            default_value = "30m",
            help = "Average duration (e.g. 30m, 1h30m)"
        )]
        avg_time: String,
        #[arg(
            long,
            default_value = "0",
            help = "Std dev of duration (same format as avg_time). 0 = auto (avg/5)"
        )]
        sigma_time: String,
        #[arg(long, default_value_t = 0.5)]
        abandonability: f64,
        #[arg(long)]
        description: Option<String>,
        #[arg(long)]
        parallelizable: bool,
        #[arg(long)]
        allows_parallel: bool,
        #[arg(long, help = "Lock start time (scheduler cannot move)")]
        fixed: bool,
        #[arg(
            long,
            help = "Window mode: 'day' (occurrence day) or 'period' (until next occurrence)"
        )]
        window: Option<String>,
    },

    /// Delete a habit
    #[command(visible_alias = "rm")]
    Delete { id: String },

    /// Manage habit pause periods (#303)
    Pause {
        #[command(subcommand)]
        command: PauseCommands,
    },

    /// Detect and offer to remove redundant step dependency edges (#355)
    StepsCheck { id: String },

    /// Manage habit steps (#95)
    Steps {
        #[command(subcommand)]
        command: StepsCommands,
    },
}

#[derive(Subcommand)]
enum SkillCommands {
    /// List skills
    #[command(visible_alias = "ls")]
    List,

    /// Show skill detail
    #[command(visible_alias = "get")]
    Show { slug: String },

    /// Create a skill (interactive if no args in terminal)
    Create {
        #[arg(short, long, help = "Skill slug")]
        slug: Option<String>,
        #[arg(short, long, help = "Skill name")]
        name: Option<String>,
        #[arg(long, help = "Skill description")]
        description: Option<String>,
        #[arg(long, help = "Skill body file or '-' for stdin")]
        body: Option<String>,
    },

    /// Update a skill (interactive if no args in terminal)
    Update {
        slug: String,
        #[arg(short, long, help = "Skill name")]
        name: Option<String>,
        #[arg(long, help = "Skill description")]
        description: Option<String>,
        #[arg(long, help = "Skill body file or '-' for stdin")]
        body: Option<String>,
    },

    /// Delete a skill
    #[command(visible_alias = "rm")]
    Delete { slug: String },
}

#[derive(Subcommand)]
enum PauseCommands {
    /// Add a pause period to a habit
    Add {
        id: String,
        #[arg(long, help = "Start date (YYYY-MM-DD, inclusive)")]
        from: String,
        #[arg(long, help = "End date (YYYY-MM-DD, inclusive)")]
        to: String,
        #[arg(long, help = "Optional reason (e.g. 休暇)")]
        reason: Option<String>,
    },
    /// List pause periods for a habit
    #[command(visible_alias = "ls")]
    List { id: String },
    /// Remove a pause period
    #[command(visible_alias = "rm")]
    Remove { id: String, pause_id: String },
}

#[derive(Subcommand)]
enum StepsCommands {
    /// List steps for a habit
    #[command(visible_alias = "ls")]
    List { id: String },

    /// Edit steps for a habit in $EDITOR (JSON array)
    Edit { id: String },

    /// Replace steps from a JSON file or stdin ("-"; // comments ignored)
    Set {
        id: String,
        #[arg(help = "JSON file path or '-' for stdin (// comments are ignored)")]
        file: String,
    },
}

#[derive(Subcommand)]
enum ScheduleCommands {
    /// Get active schedule
    Get,

    /// Generate a new schedule
    Generate {
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

    /// Start a local server and complete Google OAuth2 login in one step
    Login {
        #[arg(long)]
        client_id: Option<String>,
        #[arg(long)]
        client_secret: Option<String>,
        #[arg(long)]
        calendar_id: Option<String>,
        #[arg(long, default_value_t = 8765)]
        port: u16,
        #[arg(long)]
        no_browser: bool,
    },

    /// Manually trigger Google Calendar sync
    Trigger,

    /// Delete all mapped Google Calendar events and clear local mappings
    #[command(visible_alias = "cleanup")]
    DeleteAll,
}

fn main() {
    let _guard = takusu_local_lib::sentry::init(
        "takusu_local_lib=info",
        Some(concat!(env!("CARGO_PKG_NAME"), "@", env!("CARGO_PKG_VERSION")).into()),
    );

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();

    runtime.block_on(async {
        let cli = Cli::parse();
        let mut cfg = config::load();

        if matches!(cli.command, Commands::GenRootToken) {
            let token = generate_root_token();
            println!("{token}");
            eprintln!("\nSet this as TAKUSU_ROOT_TOKEN env var for takusu.");
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

        // Handle config commands before building the app so storage/worker_url
        // changes are reflected immediately.
        if let Commands::Config { command } = &cli.command {
            match command {
                ConfigCommands::Show => {
                    config::show();
                    return;
                }
                ConfigCommands::Init => {
                    config::init();
                    return;
                }
                ConfigCommands::Set {
                    storage,
                    db,
                    worker_url,
                    workers_token,
                    root_token,
                    tz,
                    sleep_start,
                    sleep_end,
                } => {
                    if let Some(v) = storage {
                        config::set("storage", v).unwrap_or_else(|e| {
                            eprintln!("Error: {e}");
                            process::exit(1);
                        });
                    }
                    if let Some(v) = db {
                        config::set("db", v).unwrap_or_else(|e| {
                            eprintln!("Error: {e}");
                            process::exit(1);
                        });
                    }
                    if let Some(v) = worker_url {
                        config::set("worker_url", v).unwrap_or_else(|e| {
                            eprintln!("Error: {e}");
                            process::exit(1);
                        });
                    }
                    if let Some(v) = workers_token {
                        config::set("workers_token", v).unwrap_or_else(|e| {
                            eprintln!("Error: {e}");
                            process::exit(1);
                        });
                    }
                    if let Some(v) = root_token {
                        config::set("root_token", v).unwrap_or_else(|e| {
                            eprintln!("Error: {e}");
                            process::exit(1);
                        });
                    }
                    if let Some(v) = tz {
                        config::set("tz", v).unwrap_or_else(|e| {
                            eprintln!("Error: {e}");
                            process::exit(1);
                        });
                    }
                    if let Some(v) = sleep_start {
                        config::set("sleep_start", v).unwrap_or_else(|e| {
                            eprintln!("Error: {e}");
                            process::exit(1);
                        });
                    }
                    if let Some(v) = sleep_end {
                        config::set("sleep_end", v).unwrap_or_else(|e| {
                            eprintln!("Error: {e}");
                            process::exit(1);
                        });
                    }
                    cfg = config::load();
                }
            }
        }

        let tz_str = cli.tz.clone().or(cfg.tz.clone()).unwrap_or_else(|| "UTC".into());

        // Build local config from CLI config and environment overrides
        let mut local_cfg = LocalConfig::default();
        if let Ok(v) = std::env::var("TAKUSU_STORAGE") && !v.is_empty() {
            local_cfg.storage = v;
        } else if let Some(ref v) = cfg.storage {
            local_cfg.storage = v.clone();
        }
        if let Ok(v) = std::env::var("TAKUSU_DB") && !v.is_empty() {
            local_cfg.db = v;
        } else if let Some(ref v) = cfg.db {
            local_cfg.db = v.clone();
        }
        if let Ok(v) = std::env::var("TAKUSU_WORKERS_URL") && !v.is_empty() {
            local_cfg.worker_url = v;
        } else if let Ok(v) = std::env::var("TAKUSU_WORKER_URL") && !v.is_empty() {
            local_cfg.worker_url = v;
        } else if let Some(ref v) = cfg.worker_url {
            local_cfg.worker_url = v.clone();
        }

        let env_root = std::env::var("TAKUSU_ROOT_TOKEN")
            .ok()
            .filter(|s| !s.is_empty());
        let env_workers = std::env::var("TAKUSU_WORKERS_TOKEN")
            .ok()
            .filter(|s| !s.is_empty());

        let workers_token = env_workers
            .clone()
            .or_else(|| cfg.workers_token.clone())
            .or_else(|| env_root.clone())
            .or_else(|| cfg.root_token.clone())
            .unwrap_or_default();

        let storage: Arc<dyn takusu_storage::Storage> = match local_cfg.storage_kind() {
            StorageKind::Workers => {
                let url = std::env::var("TAKUSU_WORKERS_URL")
                    .ok()
                    .filter(|s| !s.is_empty())
                    .or_else(|| local_cfg.worker_url.split('|').next().map(|s| s.to_string()))
                    .unwrap_or_default();
                if url.is_empty() {
                    eprintln!("Error: worker_url is required for the workers backend");
                    process::exit(1);
                }
                if workers_token.is_empty() {
                    eprintln!("Error: workers_token (or TAKUSU_ROOT_TOKEN) is required for the workers backend");
                    process::exit(1);
                }
                Arc::new(WorkersStorage::new_with(url, workers_token))
            }
            StorageKind::Sqlite => {
                let storage = SqliteStorage::init(&local_cfg)
                    .await
                    .unwrap_or_else(|e| {
                        eprintln!("Error initializing sqlite storage: {e}");
                        process::exit(1);
                    });
                Arc::new(storage)
            }
        };

        let token_cache = Arc::new(TokenCache::with_default_ttl());
        let app = TakusuApp::new(storage, token_cache);

        let tz = jiff::tz::TimeZone::get(&tz_str).unwrap_or_else(|_| {
            eprintln!("Error: invalid timezone '{tz_str}' (e.g. Asia/Tokyo)");
            process::exit(1);
        });

        if let Err(e) = run(cli.mode, &app, tz, cli.command, &cfg).await {
            eprintln!("Error: {e}");
            process::exit(1);
        }
    })
}

async fn run(
    mode: DisplayMode,
    app: &TakusuApp,
    tz: jiff::tz::TimeZone,
    cmd: Commands,
    cfg: &CliConfig,
) -> Result<(), AppError> {
    match cmd {
        Commands::Health => {
            println!("OK (local mode)");
        }
        Commands::Task { command } => run_task(mode, app, &tz, command).await?,
        Commands::Schedule { command } => run_schedule(mode, app, &tz, command).await?,
        Commands::Token { command } => run_token(mode, app, command).await?,
        Commands::Sync { command } => run_sync(app, command).await?,
        Commands::Habit { command } => run_habit(mode, app, command).await?,
        Commands::Skill { command } => run_skill(mode, app, command).await?,
        Commands::GenRootToken => unreachable!(),
        Commands::Completion { .. } => unreachable!(),
        Commands::Config { command } => run_config(command, app, cfg).await?,
    }
    Ok(())
}

/// Build a habit_id (UUID) → display_id map for task ID labels (h1#5, #305).
/// Returns an empty map if the habit list cannot be fetched (e.g. empty DB),
/// in which case task labels fall back to the plain `#N` form.
async fn habit_display_map(app: &TakusuApp) -> std::collections::HashMap<String, i64> {
    app.list_habits()
        .await
        .map(|habits| habits.into_iter().map(|h| (h.id, h.display_id)).collect())
        .unwrap_or_default()
}

async fn run_task(
    mode: DisplayMode,
    app: &TakusuApp,
    tz: &jiff::tz::TimeZone,
    cmd: TaskCommands,
) -> Result<(), AppError> {
    // Build habit_id → display_id map once for task ID labels (h1#5, #305).
    // Habits are few, so fetching on every task command is cheap.
    let habit_map = habit_display_map(app).await;
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
            let tasks = app.list_tasks(&query).await?;
            match mode {
                DisplayMode::Rich => display_rich::display_tasks(&tasks, tz, &habit_map),
                DisplayMode::Simple => display_simple::display_tasks(&tasks, tz, &habit_map),
            }
        }
        TaskCommands::Show { id } => {
            let task = app.get_task(&id).await?;
            let entry = match app.get_schedule().await {
                Ok(schedule) => {
                    let entries: Vec<ScheduleEntry> =
                        serde_json::from_str(&schedule.schedule).unwrap_or_default();
                    entries.into_iter().find(|e| e.task_id == task.id)
                }
                Err(_) => None,
            };
            match mode {
                DisplayMode::Rich => {
                    display_rich::display_task_detail(&task, entry.as_ref(), tz, &habit_map)
                }
                DisplayMode::Simple => {
                    display_simple::display_task_detail(&task, entry.as_ref(), tz, &habit_map)
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
            parallelizable,
            allows_parallel,
            fixed,
        } => {
            let (title, end_at) = if is_interactive() && title.is_none() && end_at.is_none() {
                let t = prompt("Title");
                let e = prompt("End at (e.g. 2025-06-05 or 2025-06-05T23:59)");
                (Some(t), Some(e))
            } else {
                (title, end_at)
            };
            let avg_minutes = parse_duration(&avg_time).map_err(AppError::BadRequest)?;
            let sigma_minutes: i64 = parse_duration(&sigma_time).map_err(AppError::BadRequest)?;
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
                parallelizable,
                allows_parallel,
                abandonability: Some(abandonability),
                description,
                ical_uid: None,
                habit_id: None,
                fixed,
                habit_step_id: None,
            };
            let task = app.create_task(&body).await?;
            match mode {
                DisplayMode::Rich => display_rich::display_tasks(&[task], tz, &habit_map),
                DisplayMode::Simple => display_simple::display_tasks(&[task], tz, &habit_map),
            }
        }
        TaskCommands::Edit { id } => {
            let task = app.get_task(&id).await?;
            let all_tasks = app.list_tasks(&Default::default()).await?;
            let original = editor::format_task_for_editing(&task, &all_tasks);
            let edited = editor::open_editor(&original, &task.id)
                .map_err(|e| AppError::BadRequest(e.to_string()))?;
            let update = editor::parse_edited_task(&edited).map_err(AppError::BadRequest)?;
            let updated = app.update_task(&id, &update).await?;
            match mode {
                DisplayMode::Rich => display_rich::display_tasks(&[updated], tz, &habit_map),
                DisplayMode::Simple => display_simple::display_tasks(&[updated], tz, &habit_map),
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
            fixed,
        } => {
            let avg_minutes = avg_time
                .as_ref()
                .map(|s| parse_duration(s))
                .transpose()
                .map_err(AppError::BadRequest)?;
            let sigma_minutes = sigma_time
                .as_ref()
                .map(|s| parse_duration(s))
                .transpose()
                .map_err(AppError::BadRequest)?;
            let body = takusu_storage::UpdateTask {
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
                habit_id: None,
                user_edited: None,
                fixed,
                habit_step_id: None,
            };
            let task = app.update_task(&id, &body).await?;
            match mode {
                DisplayMode::Rich => display_rich::display_tasks(&[task], tz, &habit_map),
                DisplayMode::Simple => display_simple::display_tasks(&[task], tz, &habit_map),
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
            parallelizable,
            allows_parallel,
            fixed,
        } => {
            let avg_minutes = parse_duration(&avg_time).map_err(AppError::BadRequest)?;
            let sigma_minutes: i64 = parse_duration(&sigma_time).map_err(AppError::BadRequest)?;
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
                parallelizable,
                allows_parallel,
                abandonability: Some(abandonability),
                description,
                ical_uid: None,
                habit_id: None,
                fixed,
                habit_step_id: None,
            };
            let task = app.replace_task(&id, &body).await?;
            match mode {
                DisplayMode::Rich => display_rich::display_tasks(&[task], tz, &habit_map),
                DisplayMode::Simple => display_simple::display_tasks(&[task], tz, &habit_map),
            }
        }
        TaskCommands::Delete { id } => {
            app.delete_task(&id).await?;
            println!("Task {id} deleted.");
        }
        TaskCommands::Status { id, status } => {
            let body = takusu_storage::UpdateTask {
                status: Some(status.clone()),
                ..Default::default()
            };
            let task = app.update_task(&id, &body).await?;
            match mode {
                DisplayMode::Rich => display_rich::display_tasks(&[task], tz, &habit_map),
                DisplayMode::Simple => display_simple::display_tasks(&[task], tz, &habit_map),
            }
        }
        TaskCommands::DepsCheck => {
            deps_check_tasks(app).await?;
        }
    }
    Ok(())
}

async fn run_habit(mode: DisplayMode, app: &TakusuApp, cmd: HabitCommands) -> Result<(), AppError> {
    match cmd {
        HabitCommands::List => {
            let habits = app.list_habits().await?;
            match mode {
                DisplayMode::Rich => display_rich::display_habits(&habits),
                DisplayMode::Simple => display_simple::display_habits(&habits),
            }
        }
        HabitCommands::Show { id } => {
            let detail = app.get_habit(&id).await?;
            match mode {
                DisplayMode::Rich => display_rich::display_habit_detail(&detail.habit),
                DisplayMode::Simple => display_simple::display_habit_detail(&detail.habit),
            }
            // Show steps (#95) if any.
            if !detail.steps.is_empty() {
                println!("   steps:");
                for s in &detail.steps {
                    let deps: Vec<String> = serde_json::from_str(&s.depends_on).unwrap_or_default();
                    println!(
                        "     {} [{}] {} ({}–{}, {}min){}",
                        s.id,
                        s.position,
                        s.title,
                        s.start_time,
                        s.end_time,
                        s.avg_minutes,
                        if deps.is_empty() {
                            String::new()
                        } else {
                            format!(" ← {}", deps.join(","))
                        }
                    );
                }
            }
            // Show pause periods (#303) if any.
            let pauses = app.list_habit_pauses(&id).await.unwrap_or_default();
            if !pauses.is_empty() {
                println!("   pauses:");
                for p in &pauses {
                    println!(
                        "     {} {}..{} ({})",
                        p.id,
                        p.start_date,
                        p.end_date,
                        p.reason.as_deref().unwrap_or("")
                    );
                }
            }
        }
        HabitCommands::Create {
            title,
            recurrence,
            start_time,
            end_time,
            avg_time,
            sigma_time,
            abandonability,
            description,
            parallelizable,
            allows_parallel,
            fixed,
            window,
        } => {
            let (title, recurrence, start_time, end_time) = if is_interactive()
                && title.is_none()
                && recurrence.is_none()
                && start_time.is_none()
                && end_time.is_none()
            {
                let t = prompt("Title");
                let r = prompt("Recurrence (e.g. daily, weekdays, Mon,Wed,Fri)");
                let s = prompt("Start time (HH:MM)");
                let e = prompt("End time (HH:MM)");
                (Some(t), Some(r), Some(s), Some(e))
            } else {
                (title, recurrence, start_time, end_time)
            };
            let avg_minutes = parse_duration(&avg_time).map_err(AppError::BadRequest)?;
            let sigma_minutes: i64 = parse_duration(&sigma_time).map_err(AppError::BadRequest)?;
            let body = CreateHabit {
                title: title.unwrap_or_default(),
                recurrence: recurrence.unwrap_or_default(),
                start_time: start_time.unwrap_or_default(),
                end_time: end_time.unwrap_or_default(),
                avg_minutes,
                sigma_minutes: if sigma_minutes > 0 {
                    Some(sigma_minutes)
                } else {
                    None
                },
                parallelizable: if parallelizable { Some(true) } else { None },
                allows_parallel: if allows_parallel { Some(true) } else { None },
                abandonability: Some(abandonability),
                description,
                fixed: if fixed { Some(true) } else { None },
                window_mode: window,
            };
            let habit = app.create_habit(&body).await?;
            match mode {
                DisplayMode::Rich => display_rich::display_habit_detail(&habit),
                DisplayMode::Simple => display_simple::display_habit_detail(&habit),
            }
        }
        HabitCommands::Edit { id } => {
            let detail = app.get_habit(&id).await?;
            let habit = &detail.habit;
            let original = editor::format_habit_for_editing(habit);
            let edited = editor::open_editor(&original, &habit.id)
                .map_err(|e| AppError::BadRequest(e.to_string()))?;
            let update = editor::parse_edited_habit(&edited).map_err(AppError::BadRequest)?;
            let updated = app.update_habit(&id, &update).await?;
            match mode {
                DisplayMode::Rich => display_rich::display_habit_detail(&updated),
                DisplayMode::Simple => display_simple::display_habit_detail(&updated),
            }
        }
        HabitCommands::Update {
            id,
            title,
            description,
            recurrence,
            start_time,
            end_time,
            avg_time,
            sigma_time,
            parallelizable,
            allows_parallel,
            abandonability,
            active,
            fixed,
            window,
        } => {
            let avg_minutes = avg_time
                .as_ref()
                .map(|s| parse_duration(s))
                .transpose()
                .map_err(AppError::BadRequest)?;
            let sigma_minutes = sigma_time
                .as_ref()
                .map(|s| parse_duration(s))
                .transpose()
                .map_err(AppError::BadRequest)?;
            let body = UpdateHabit {
                title,
                description,
                recurrence,
                start_time,
                end_time,
                avg_minutes,
                sigma_minutes,
                parallelizable,
                allows_parallel,
                abandonability,
                active,
                fixed,
                window_mode: window,
            };
            let habit = app.update_habit(&id, &body).await?;
            match mode {
                DisplayMode::Rich => display_rich::display_habit_detail(&habit),
                DisplayMode::Simple => display_simple::display_habit_detail(&habit),
            }
        }
        HabitCommands::Replace {
            id,
            title,
            recurrence,
            start_time,
            end_time,
            avg_time,
            sigma_time,
            abandonability,
            description,
            parallelizable,
            allows_parallel,
            fixed,
            window,
        } => {
            let avg_minutes = parse_duration(&avg_time).map_err(AppError::BadRequest)?;
            let sigma_minutes: i64 = parse_duration(&sigma_time).map_err(AppError::BadRequest)?;
            let body = CreateHabit {
                title,
                recurrence,
                start_time,
                end_time,
                avg_minutes,
                sigma_minutes: if sigma_minutes > 0 {
                    Some(sigma_minutes)
                } else {
                    None
                },
                parallelizable: if parallelizable { Some(true) } else { None },
                allows_parallel: if allows_parallel { Some(true) } else { None },
                abandonability: Some(abandonability),
                description,
                fixed: if fixed { Some(true) } else { None },
                window_mode: window,
            };
            let habit = app.replace_habit(&id, &body).await?;
            match mode {
                DisplayMode::Rich => display_rich::display_habit_detail(&habit),
                DisplayMode::Simple => display_simple::display_habit_detail(&habit),
            }
        }
        HabitCommands::Delete { id } => {
            app.delete_habit(&id).await?;
            println!("Habit {id} deleted.");
        }
        HabitCommands::Pause { command } => run_pause(mode, app, command).await?,
        HabitCommands::StepsCheck { id } => {
            deps_check_steps(app, &id).await?;
        }
        HabitCommands::Steps { command } => run_habit_steps(mode, app, command).await?,
    }
    Ok(())
}

async fn run_habit_steps(
    mode: DisplayMode,
    app: &TakusuApp,
    cmd: StepsCommands,
) -> Result<(), AppError> {
    match cmd {
        StepsCommands::List { id } => {
            let steps = app.list_habit_steps(&id).await?;
            match mode {
                DisplayMode::Rich => display_rich::display_habit_steps(&steps),
                DisplayMode::Simple => display_simple::display_habit_steps(&steps),
            }
        }
        StepsCommands::Edit { id } => {
            let steps = app.list_habit_steps(&id).await?;
            let original =
                editor::format_steps_for_editing(&steps).map_err(AppError::BadRequest)?;
            let suffix = format!("{}", uuid::Uuid::now_v7());
            let edited = editor::open_editor(&original, &suffix)
                .map_err(|e| AppError::BadRequest(e.to_string()))?;
            let inputs = editor::parse_edited_steps(&edited).map_err(AppError::BadRequest)?;
            let replaced = app.replace_habit_steps(&id, &inputs).await?;
            match mode {
                DisplayMode::Rich => display_rich::display_habit_steps(&replaced),
                DisplayMode::Simple => display_simple::display_habit_steps(&replaced),
            }
        }
        StepsCommands::Set { id, file } => {
            let content = read_steps_file(&file).await?;
            let inputs = editor::parse_edited_steps(&content).map_err(AppError::BadRequest)?;
            let replaced = app.replace_habit_steps(&id, &inputs).await?;
            match mode {
                DisplayMode::Rich => display_rich::display_habit_steps(&replaced),
                DisplayMode::Simple => display_simple::display_habit_steps(&replaced),
            }
        }
    }
    Ok(())
}

async fn read_steps_file(path: &str) -> Result<String, AppError> {
    match path {
        "-" => {
            let mut buf = String::new();
            std::io::stdin()
                .read_to_string(&mut buf)
                .map_err(|e| AppError::BadRequest(format!("failed to read stdin: {e}")))?;
            Ok(buf)
        }
        path => tokio::fs::read_to_string(path)
            .await
            .map_err(|e| AppError::BadRequest(format!("failed to read {path}: {e}"))),
    }
}

async fn run_skill(mode: DisplayMode, app: &TakusuApp, cmd: SkillCommands) -> Result<(), AppError> {
    match cmd {
        SkillCommands::List => {
            let skills = app.list_skills().await?;
            match mode {
                DisplayMode::Rich => display_rich::display_skills(&skills),
                DisplayMode::Simple => display_simple::display_skills(&skills),
            }
        }
        SkillCommands::Show { slug } => {
            let skill = app.get_skill(&slug).await?;
            match mode {
                DisplayMode::Rich => display_rich::display_skill_detail(&skill),
                DisplayMode::Simple => display_simple::display_skill_detail(&skill),
            }
        }
        SkillCommands::Create {
            slug,
            name,
            description,
            body,
        } => {
            let (slug, name, description, body) = if is_interactive()
                && slug.is_none()
                && name.is_none()
                && description.is_none()
                && body.is_none()
            {
                let slug = prompt("Slug");
                let name = prompt("Name");
                let description = prompt("Description");
                let body_path = prompt("Body file (or - for stdin)");
                (Some(slug), Some(name), Some(description), Some(body_path))
            } else {
                (slug, name, description, body)
            };
            let slug = slug.ok_or_else(|| AppError::BadRequest("slug is required".into()))?;
            let name = name.ok_or_else(|| AppError::BadRequest("name is required".into()))?;
            let description = description.unwrap_or_default();
            let body = read_skill_body(body).await?;
            let body = body.ok_or_else(|| AppError::BadRequest("body is required".into()))?;
            let created = app
                .create_skill(&CreateSkill {
                    slug,
                    name,
                    description,
                    body,
                    built_in: None,
                })
                .await?;
            match mode {
                DisplayMode::Rich => display_rich::display_skill_detail(&created),
                DisplayMode::Simple => display_simple::display_skill_detail(&created),
            }
        }
        SkillCommands::Update {
            slug,
            name,
            description,
            body,
        } => {
            let body = read_skill_body(body).await?;
            if name.is_none() && description.is_none() && body.is_none() {
                return Err(AppError::BadRequest(
                    "at least one of name, description, or body is required".into(),
                ));
            }
            let updated = app
                .update_skill(
                    &slug,
                    &takusu_storage::UpdateSkill {
                        name,
                        description,
                        body,
                    },
                )
                .await?;
            match mode {
                DisplayMode::Rich => display_rich::display_skill_detail(&updated),
                DisplayMode::Simple => display_simple::display_skill_detail(&updated),
            }
        }
        SkillCommands::Delete { slug } => {
            app.delete_skill(&slug).await?;
            println!("Skill {slug} deleted.");
        }
    }
    Ok(())
}

async fn read_skill_body(path: Option<String>) -> Result<Option<String>, AppError> {
    match path.as_deref() {
        None => Ok(None),
        Some("-") => {
            let mut buf = String::new();
            std::io::stdin()
                .read_to_string(&mut buf)
                .map_err(|e| AppError::BadRequest(format!("failed to read stdin: {e}")))?;
            Ok(Some(buf))
        }
        Some(path) => tokio::fs::read_to_string(path)
            .await
            .map(Some)
            .map_err(|e| AppError::BadRequest(format!("failed to read {path}: {e}"))),
    }
}

async fn run_pause(
    _mode: DisplayMode,
    app: &TakusuApp,
    cmd: PauseCommands,
) -> Result<(), AppError> {
    match cmd {
        PauseCommands::Add {
            id,
            from,
            to,
            reason,
        } => {
            let body = CreateHabitPause {
                start_date: from,
                end_date: to,
                reason,
            };
            let pause = app.create_habit_pause(&id, &body).await?;
            println!(
                "Pause added: {} {}..{} ({})",
                pause.id,
                pause.start_date,
                pause.end_date,
                pause.reason.as_deref().unwrap_or("")
            );
        }
        PauseCommands::List { id } => {
            let pauses = app.list_habit_pauses(&id).await?;
            if pauses.is_empty() {
                println!("No pauses for habit {id}.");
            } else {
                for p in &pauses {
                    println!(
                        "{}\t{}\t{}\t{}",
                        p.id,
                        p.start_date,
                        p.end_date,
                        p.reason.as_deref().unwrap_or("")
                    );
                }
            }
        }
        PauseCommands::Remove { id, pause_id } => {
            app.delete_habit_pause(&id, &pause_id).await?;
            println!("Pause {pause_id} removed.");
        }
    }
    Ok(())
}

async fn run_schedule(
    mode: DisplayMode,
    app: &TakusuApp,
    tz: &jiff::tz::TimeZone,
    cmd: ScheduleCommands,
) -> Result<(), AppError> {
    let habit_map = habit_display_map(app).await;
    match cmd {
        ScheduleCommands::Get => {
            let schedule = app.get_schedule().await?;
            let entries: Vec<ScheduleEntry> =
                serde_json::from_str(&schedule.schedule).unwrap_or_default();
            let tasks = app
                .list_tasks(&TaskQuery::default())
                .await
                .unwrap_or_default();
            match mode {
                DisplayMode::Rich => {
                    display_rich::display_schedule(&entries, &tasks, tz, &habit_map)
                }
                DisplayMode::Simple => {
                    display_simple::display_schedule(&entries, &tasks, tz, &habit_map)
                }
            }
        }
        ScheduleCommands::Generate { task_ids, sleep } => {
            let body = takusu_local_lib::app::GenerateScheduleInput { task_ids, sleep };
            let schedule = app.generate_schedule(&body).await?;
            let entries: Vec<ScheduleEntry> =
                serde_json::from_str(&schedule.schedule).unwrap_or_default();
            let tasks = app
                .list_tasks(&TaskQuery::default())
                .await
                .unwrap_or_default();
            match mode {
                DisplayMode::Rich => {
                    display_rich::display_schedule(&entries, &tasks, tz, &habit_map)
                }
                DisplayMode::Simple => {
                    display_simple::display_schedule(&entries, &tasks, tz, &habit_map)
                }
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
            let body = takusu_local_lib::app::RescheduleInput {
                mode: rmode,
                from: from.map(|s| parse_dt(&s, tz)).transpose()?,
                until: until.map(|s| parse_dt(&s, tz)).transpose()?,
                task_ids,
                pinned: pinned.unwrap_or_default(),
                sleep,
            };
            let schedule = app.reschedule(&body).await?;
            let entries: Vec<ScheduleEntry> =
                serde_json::from_str(&schedule.schedule).unwrap_or_default();
            let tasks = app
                .list_tasks(&TaskQuery::default())
                .await
                .unwrap_or_default();
            match mode {
                DisplayMode::Rich => {
                    display_rich::display_schedule(&entries, &tasks, tz, &habit_map)
                }
                DisplayMode::Simple => {
                    display_simple::display_schedule(&entries, &tasks, tz, &habit_map)
                }
            }
        }
        ScheduleCommands::Move {
            task_id,
            start_at,
            force,
        } => {
            let result = app
                .move_entry(&task_id, &parse_dt(&start_at, tz)?, force)
                .await?;
            println!("{}", serde_json::to_string_pretty(&result).unwrap());
        }
        ScheduleCommands::Clear => {
            app.clear_schedule().await?;
            println!("Schedule cleared.");
        }
    }
    Ok(())
}

async fn run_token(mode: DisplayMode, app: &TakusuApp, cmd: TokenCommands) -> Result<(), AppError> {
    match cmd {
        TokenCommands::Create { label } => {
            let resp = app.create_token(label.as_deref()).await?;
            println!("Token issued:");
            println!("  ID:    {}", resp.id);
            println!("  Token: {}", resp.token);
            println!("  Label: {}", resp.label.as_deref().unwrap_or("—"));
            println!("  Created: {}", resp.created_at);
            eprintln!("\n⚠ Save the token value; it won't be shown again.");
        }
        TokenCommands::List => {
            let tokens = app.list_tokens().await?;
            match mode {
                DisplayMode::Rich => display_rich::display_tokens(&tokens),
                DisplayMode::Simple => display_simple::display_tokens(&tokens),
            }
        }
        TokenCommands::Revoke { id } => {
            app.revoke_token(id).await?;
            println!("Token {id} revoked.");
        }
    }
    Ok(())
}

async fn run_sync(app: &TakusuApp, cmd: SyncCommands) -> Result<(), AppError> {
    match cmd {
        SyncCommands::Settings => {
            let settings = app.get_gcal_settings().await?;
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
            let body = takusu_storage::UpdateGoogleCalSettings {
                enabled,
                calendar_id,
                client_id,
                client_secret,
                refresh_token,
            };
            let settings = app.update_gcal_settings(&body).await?;
            println!("Sync settings updated:");
            println!("  enabled:           {}", settings.enabled);
            println!("  calendar_id:      {}", settings.calendar_id);
            println!("  has_client_secret: {}", settings.has_client_secret);
            println!("  has_refresh_token:  {}", settings.has_refresh_token);
        }
        SyncCommands::Login {
            client_id,
            client_secret,
            calendar_id,
            port,
            no_browser,
        } => {
            oauth_login(app, client_id, client_secret, calendar_id, port, no_browser).await?;
        }
        SyncCommands::Trigger => {
            app.do_sync().await.map_err(AppError::Internal)?;
            println!("Sync triggered.");
        }
        SyncCommands::DeleteAll => {
            let result = app.delete_all_gcal_events().await?;
            println!("Deleted {} Google Calendar event(s).", result.deleted);
            if !result.failed.is_empty() {
                eprintln!("{} deletion(s) failed:", result.failed.len());
                for f in &result.failed {
                    eprintln!("  - {}: {}", f.task_id, f.error);
                }
            }
        }
    }
    Ok(())
}

#[derive(Debug, serde::Deserialize)]
struct OAuthCallbackQuery {
    code: Option<String>,
    error: Option<String>,
    error_description: Option<String>,
}

async fn oauth_callback_handler(
    State(tx): State<tokio::sync::mpsc::Sender<Result<String, String>>>,
    Query(query): Query<OAuthCallbackQuery>,
) -> Html<&'static str> {
    if let Some(error) = query.error {
        let msg = match query.error_description {
            Some(desc) => format!("{error}: {desc}"),
            None => error,
        };
        let _ = tx.send(Err(msg)).await;
        return Html(
            "<html><body><h1>認証に失敗しました</h1><p>ターミナルを確認してください。</p></body></html>",
        );
    }
    if let Some(code) = query.code {
        let _ = tx.send(Ok(code)).await;
        return Html(
            "<html><body><h1>認証成功</h1><p>このウィンドウを閉じて、ターミナルに戻ってください。</p></body></html>",
        );
    }
    Html("<html><body><h1>不正なリクエストです</h1></body></html>")
}

fn open_browser(url: &str) {
    let (program, arg) = if cfg!(target_os = "macos") {
        ("open", None)
    } else if cfg!(target_os = "windows") {
        ("cmd", Some("/c"))
    } else {
        ("xdg-open", None)
    };
    let mut cmd = process::Command::new(program);
    if let Some(a) = arg {
        cmd.arg(a);
    }
    if cfg!(target_os = "windows") {
        cmd.arg("start").arg("").arg(url);
    } else {
        cmd.arg(url);
    }
    let _ = cmd.spawn();
}

fn prompt_secret(label: &str) -> Result<String, AppError> {
    rpassword::prompt_password(format!("{label}: "))
        .map_err(|e| AppError::Internal(format!("failed to read secret: {e}")))
}

async fn oauth_login(
    app: &TakusuApp,
    client_id: Option<String>,
    client_secret: Option<String>,
    calendar_id: Option<String>,
    port: u16,
    no_browser: bool,
) -> Result<(), AppError> {
    let settings = app.get_gcal_settings().await?;

    let client_id = if let Some(id) = client_id {
        if id.is_empty() {
            return Err(AppError::BadRequest("client_id must not be empty".into()));
        }
        id
    } else if !settings.client_id.is_empty() {
        settings.client_id
    } else if is_interactive() {
        let id = prompt("Google OAuth client_id");
        if id.is_empty() {
            return Err(AppError::BadRequest("client_id is required".into()));
        }
        id
    } else {
        return Err(AppError::BadRequest("client_id is required".into()));
    };

    let client_secret_opt = if let Some(secret) = client_secret {
        if secret.is_empty() {
            return Err(AppError::BadRequest(
                "client_secret must not be empty".into(),
            ));
        }
        Some(secret)
    } else if settings.has_client_secret {
        None
    } else if is_interactive() {
        let secret = prompt_secret("Google OAuth client_secret")?;
        if secret.is_empty() {
            return Err(AppError::BadRequest("client_secret is required".into()));
        }
        Some(secret)
    } else {
        return Err(AppError::BadRequest("client_secret is required".into()));
    };

    let calendar_id = if let Some(id) = calendar_id {
        if id.is_empty() {
            if settings.calendar_id.is_empty() {
                "primary".to_string()
            } else {
                settings.calendar_id
            }
        } else {
            id
        }
    } else if settings.calendar_id.is_empty() {
        "primary".to_string()
    } else {
        settings.calendar_id
    };

    app.update_gcal_settings(&takusu_storage::UpdateGoogleCalSettings {
        enabled: Some(true),
        calendar_id: Some(calendar_id.clone()),
        client_id: Some(client_id.clone()),
        client_secret: client_secret_opt,
        refresh_token: None,
    })
    .await?;

    let (tx, mut rx) = tokio::sync::mpsc::channel::<Result<String, String>>(1);
    let router = Router::new()
        .route("/callback", get(oauth_callback_handler))
        .with_state(tx);

    let listener = tokio::net::TcpListener::bind(("127.0.0.1", port))
        .await
        .map_err(|e| AppError::Internal(format!("failed to bind callback server: {e}")))?;
    let actual_port = listener
        .local_addr()
        .map_err(|e| AppError::Internal(format!("{e}")))?
        .port();
    let redirect_uri = format!("http://127.0.0.1:{actual_port}/callback");
    let auth_url = app.oauth_url(&redirect_uri).await?;

    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();
    let server = axum::serve(listener, router).with_graceful_shutdown(async {
        let _ = shutdown_rx.await;
    });
    let server_handle = tokio::spawn(async move { server.await });

    println!("Starting local callback server on 127.0.0.1:{actual_port}");
    if no_browser {
        println!("Open this URL in your browser:\n  {auth_url}");
    } else {
        open_browser(&auth_url);
    }

    let code = tokio::time::timeout(Duration::from_secs(300), rx.recv())
        .await
        .map_err(|_| AppError::Internal("OAuth callback timed out".into()))?
        .ok_or_else(|| AppError::Internal("callback channel closed".into()))?
        .map_err(|e| AppError::Internal(format!("oauth error: {e}")))?;

    let _ = shutdown_tx.send(());
    let _ = server_handle.await;

    app.oauth_callback(&code, Some(&redirect_uri)).await?;
    println!("Google Calendar OAuth login completed successfully.");
    Ok(())
}

async fn run_config(cmd: ConfigCommands, app: &TakusuApp, cfg: &CliConfig) -> Result<(), AppError> {
    match cmd {
        ConfigCommands::Show => config::show(),
        ConfigCommands::Init => config::init(),
        ConfigCommands::Set {
            tz,
            sleep_start,
            sleep_end,
            ..
        } => {
            let mut update = UpdateSettings {
                tz,
                sleep_start,
                sleep_end,
                ..Default::default()
            };
            if update.tz.is_none() && cfg.tz.is_some() {
                update.tz = cfg.tz.clone();
            }
            if update.sleep_start.is_none() && cfg.sleep_start.is_some() {
                update.sleep_start = cfg.sleep_start.clone();
            }
            if update.sleep_end.is_none() && cfg.sleep_end.is_some() {
                update.sleep_end = cfg.sleep_end.clone();
            }
            let resp = app.update_settings(&update).await?;
            println!(
                "Settings updated: tz={}, sleep_start={}, sleep_end={}",
                resp.tz, resp.sleep_start, resp.sleep_end
            );
        }
    }
    Ok(())
}

// ── Dependency analysis (#355) ─────────────────────────────────────────

use takusu_local_lib::app::DependencyNode;

fn format_path(via: &[DependencyNode]) -> String {
    via.iter()
        .map(|n| n.title.clone())
        .collect::<Vec<_>>()
        .join("→")
}

/// Remove `to_id` from the `depends` list of task `from_id` via PATCH.
async fn remove_task_dep(app: &TakusuApp, from_id: &str, to_id: &str) -> Result<(), AppError> {
    let task = app.get_task(from_id).await?;
    let mut deps: Vec<String> = serde_json::from_str(&task.depends).unwrap_or_default();
    deps.retain(|d| d != to_id);
    let body = takusu_storage::UpdateTask {
        depends: Some(deps),
        ..Default::default()
    };
    app.update_task(from_id, &body).await?;
    Ok(())
}

/// Interactive loop: detect redundant task dependency edges and let the
/// user choose which edge to remove. Iterates through all detected edges;
/// re-analyzes only after a deletion (which may introduce new redundancies
/// or remove some).
async fn deps_check_tasks(app: &TakusuApp) -> Result<(), AppError> {
    let mut redundant = app.analyze_task_dependencies().await?;
    if redundant.is_empty() {
        println!("冗長な依存はありません");
        return Ok(());
    }
    if !is_interactive() {
        println!("冗長な依存が見つかりました:");
        for r in &redundant {
            println!(
                "  「{}」→「{}」  (経路: {})",
                r.from_title,
                r.to_title,
                format_path(&r.via)
            );
        }
        return Ok(());
    }
    let mut idx = 0;
    while idx < redundant.len() {
        let r = &redundant[idx];
        println!(
            "冗長な依存が見つかりました ({}/{}):",
            idx + 1,
            redundant.len()
        );
        println!(
            "  「{}」 の経路があるため「{}」→「{}」 は冗長です。",
            format_path(&r.via),
            r.from_title,
            r.to_title
        );
        // [1] remove redundant edge; [2.N] remove the Nth path edge
        let path_pairs: Vec<(String, String)> = r
            .via
            .windows(2)
            .map(|w| (w[0].id.clone(), w[1].id.clone()))
            .collect();
        println!("[1] 冗長な辺 {}→{} を削除", r.from_title, r.to_title);
        for (i, (a, b)) in path_pairs.iter().enumerate() {
            let ta = r.via.iter().find(|n| &n.id == a).unwrap().title.clone();
            let tb = r.via.iter().find(|n| &n.id == b).unwrap().title.clone();
            println!("[2.{}] 経路上の辺 {}→{} を削除", i + 1, ta, tb);
        }
        println!("[s] スキップ  [q] 終了");
        let choice = prompt(">");
        if choice == "q" || choice == "Q" {
            return Ok(());
        }
        if choice == "s" || choice == "S" {
            idx += 1;
            continue;
        }
        if choice == "1" {
            remove_task_dep(app, &r.from, &r.to).await?;
            println!("削除しました: {}→{}", r.from_title, r.to_title);
            // Re-analyze: deletion may change the set.
            redundant = app.analyze_task_dependencies().await?;
            // Keep current index if still valid, otherwise restart from 0.
            if idx >= redundant.len() {
                idx = 0;
            }
            continue;
        }
        // Try 2.1, 2.2, ...
        if let Some(rest) = choice.strip_prefix("2.")
            && let Ok(n) = rest.parse::<usize>()
            && n >= 1
            && n <= path_pairs.len()
        {
            let (a, b) = &path_pairs[n - 1];
            remove_task_dep(app, a, b).await?;
            println!("削除しました: 経路上の辺");
            redundant = app.analyze_task_dependencies().await?;
            if idx >= redundant.len() {
                idx = 0;
            }
            continue;
        }
        println!("無効な選択です");
    }
    Ok(())
}

/// Remove `to_id` from the `depends_on` of step `from_id` within habit
/// `habit_id` via bulk replace.
async fn remove_step_dep(
    app: &TakusuApp,
    habit_id: &str,
    from_id: &str,
    to_id: &str,
) -> Result<(), AppError> {
    let steps = app.list_habit_steps(habit_id).await?;
    let inputs: Vec<takusu_storage::HabitStepInput> = steps
        .iter()
        .map(|s| {
            let mut deps: Vec<String> = serde_json::from_str(&s.depends_on).unwrap_or_default();
            if s.id == from_id {
                deps.retain(|d| d != to_id);
            }
            takusu_storage::HabitStepInput {
                id: Some(s.id.clone()),
                position: s.position,
                title: s.title.clone(),
                description: s.description.clone(),
                start_time: s.start_time.clone(),
                end_time: s.end_time.clone(),
                avg_minutes: s.avg_minutes,
                sigma_minutes: if s.sigma_minutes > 0 {
                    Some(s.sigma_minutes)
                } else {
                    None
                },
                parallelizable: Some(s.parallelizable),
                allows_parallel: Some(s.allows_parallel),
                abandonability: Some(s.abandonability),
                fixed: Some(s.fixed),
                depends_on: deps,
            }
        })
        .collect();
    app.replace_habit_steps(habit_id, &inputs).await?;
    Ok(())
}

/// Interactive loop for habit step redundant dependencies (#355).
async fn deps_check_steps(app: &TakusuApp, habit_id: &str) -> Result<(), AppError> {
    let mut redundant = app.analyze_habit_step_dependencies(habit_id).await?;
    if redundant.is_empty() {
        println!("冗長な依存はありません");
        return Ok(());
    }
    if !is_interactive() {
        println!("冗長な依存が見つかりました:");
        for r in &redundant {
            println!(
                "  「{}」→「{}」  (経路: {})",
                r.from_title,
                r.to_title,
                format_path(&r.via)
            );
        }
        return Ok(());
    }
    let mut idx = 0;
    while idx < redundant.len() {
        let r = &redundant[idx];
        println!(
            "冗長な依存が見つかりました ({}/{}):",
            idx + 1,
            redundant.len()
        );
        println!(
            "  「{}」 の経路があるため「{}」→「{}」 は冗長です。",
            format_path(&r.via),
            r.from_title,
            r.to_title
        );
        let path_pairs: Vec<(String, String)> = r
            .via
            .windows(2)
            .map(|w| (w[0].id.clone(), w[1].id.clone()))
            .collect();
        println!("[1] 冗長な辺 {}→{} を削除", r.from_title, r.to_title);
        for (i, (a, b)) in path_pairs.iter().enumerate() {
            let ta = r.via.iter().find(|n| &n.id == a).unwrap().title.clone();
            let tb = r.via.iter().find(|n| &n.id == b).unwrap().title.clone();
            println!("[2.{}] 経路上の辺 {}→{} を削除", i + 1, ta, tb);
        }
        println!("[s] スキップ  [q] 終了");
        let choice = prompt(">");
        if choice == "q" || choice == "Q" {
            return Ok(());
        }
        if choice == "s" || choice == "S" {
            idx += 1;
            continue;
        }
        if choice == "1" {
            remove_step_dep(app, habit_id, &r.from, &r.to).await?;
            println!("削除しました: {}→{}", r.from_title, r.to_title);
            redundant = app.analyze_habit_step_dependencies(habit_id).await?;
            if idx >= redundant.len() {
                idx = 0;
            }
            continue;
        }
        if let Some(rest) = choice.strip_prefix("2.")
            && let Ok(n) = rest.parse::<usize>()
            && n >= 1
            && n <= path_pairs.len()
        {
            let (a, b) = &path_pairs[n - 1];
            remove_step_dep(app, habit_id, a, b).await?;
            println!("削除しました: 経路上の辺");
            redundant = app.analyze_habit_step_dependencies(habit_id).await?;
            if idx >= redundant.len() {
                idx = 0;
            }
            continue;
        }
        println!("無効な選択です");
    }
    Ok(())
}
