mod config;
mod display_rich;
mod display_simple;
mod editor;

use clap::{CommandFactory, Parser, Subcommand};
use std::io::{self, Write};
use std::process;
use std::sync::Arc;
use takusu_local_lib::{
    app::TakusuApp,
    config::{LocalConfig, StorageKind},
    error::AppError,
    storage_sqlite::SqliteStorage,
    storage_workers::WorkersStorage,
    token_cache::TokenCache,
};
use takusu_storage::{CreateHabit, CreateTask, ScheduleEntry, TaskQuery, UpdateHabit, UpdateSettings};
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

fn parse_dt(s: &str, tz: &jiff::tz::TimeZone) -> Result<String, AppError> {
    parse_datetime_tz(s, tz).map_err(|e| AppError::BadRequest(e))
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

    /// Habit management
    Habit {
        #[command(subcommand)]
        command: HabitCommands,
    },
}

#[derive(Subcommand)]
enum ConfigCommands {
    /// Show config file path and contents
    Show,
    /// Initialize config file with defaults
    Init,
    /// Set a config value and sync to server
    Set {
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

    /// Change task status (pending, scheduled, in_progress, completed, skipped)
    Status { id: String, status: String },
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
            help = "Std dev of duration (same format as avg_time)"
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
            help = "Std dev of duration (same format as avg_time)"
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
    },

    /// Delete a habit
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

    let tz_str = cli.tz.or(cfg.tz).unwrap_or_else(|| "UTC".into());

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

    // Initialize local storage and app
    let local_cfg = LocalConfig::load().unwrap_or_else(|e| {
        eprintln!("Error loading config: {e}");
        process::exit(1);
    });

    let storage: Arc<dyn takusu_storage::Storage> = match local_cfg.storage_kind() {
        StorageKind::Workers => {
            WorkersStorage::shared(&local_cfg).unwrap_or_else(|e| {
                eprintln!("Error initializing workers storage: {e}");
                process::exit(1);
            })
        }
        StorageKind::Sqlite => {
            let root_token = LocalConfig::load_root_token();
            let storage = SqliteStorage::init(&local_cfg, root_token)
                .await
                .unwrap_or_else(|e| {
                    eprintln!("Error initializing sqlite storage: {e}");
                    process::exit(1);
                });
            Arc::new(storage)
        }
    };

    let root_token = LocalConfig::load_root_token();

    let token_cache = Arc::new(TokenCache::with_default_ttl());
    let app = TakusuApp::new(storage, root_token, token_cache);

    let tz = jiff::tz::TimeZone::get(&tz_str).unwrap_or_else(|_| {
        eprintln!("Error: invalid timezone '{tz_str}' (e.g. Asia/Tokyo)");
        process::exit(1);
    });

    if let Err(e) = run(cli.mode, &app, tz, cli.command).await {
        eprintln!("Error: {e}");
        process::exit(1);
    }
}

async fn run(
    mode: DisplayMode,
    app: &TakusuApp,
    tz: jiff::tz::TimeZone,
    cmd: Commands,
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
        Commands::GenRootToken => unreachable!(),
        Commands::Completion { .. } => unreachable!(),
        Commands::Config { command } => run_config(command, app).await?,
    }
    Ok(())
}

async fn run_task(
    mode: DisplayMode,
    app: &TakusuApp,
    tz: &jiff::tz::TimeZone,
    cmd: TaskCommands,
) -> Result<(), AppError> {
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
                DisplayMode::Rich => display_rich::display_tasks(&tasks, tz),
                DisplayMode::Simple => display_simple::display_tasks(&tasks, tz),
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
                .map_err(|e| AppError::BadRequest(e))?;
            let sigma_minutes: i64 = parse_duration(&sigma_time)
                .map_err(|e| AppError::BadRequest(e))?;
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
                ical_uid: None,
            };
            let task = app.create_task(&body).await?;
            match mode {
                DisplayMode::Rich => display_rich::display_tasks(&[task], tz),
                DisplayMode::Simple => display_simple::display_tasks(&[task], tz),
            }
        }
        TaskCommands::Edit { id } => {
            let task = app.get_task(&id).await?;
            let original = editor::format_task_for_editing(&task);
            let edited = editor::open_editor(&original, &task.id).map_err(|e| {
                AppError::BadRequest(e.to_string())
            })?;
            let update = editor::parse_edited_task(&edited).ok_or_else(|| {
                AppError::BadRequest("failed to parse edited task".to_string())
            })?;
            let updated = app.update_task(&id, &update).await?;
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
                .map_err(|e| AppError::BadRequest(e))?;
            let sigma_minutes = sigma_time
                .as_ref()
                .map(|s| parse_duration(s))
                .transpose()
                .map_err(|e| AppError::BadRequest(e))?;
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
            };
            let task = app.update_task(&id, &body).await?;
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
                .map_err(|e| AppError::BadRequest(e))?;
            let sigma_minutes: i64 = parse_duration(&sigma_time)
                .map_err(|e| AppError::BadRequest(e))?;
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
                ical_uid: None,
            };
            let task = app.replace_task(&id, &body).await?;
            match mode {
                DisplayMode::Rich => display_rich::display_tasks(&[task], tz),
                DisplayMode::Simple => display_simple::display_tasks(&[task], tz),
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
                DisplayMode::Rich => display_rich::display_tasks(&[task], tz),
                DisplayMode::Simple => display_simple::display_tasks(&[task], tz),
            }
        }
    }
    Ok(())
}

async fn run_habit(
    mode: DisplayMode,
    app: &TakusuApp,
    cmd: HabitCommands,
) -> Result<(), AppError> {
    match cmd {
        HabitCommands::List => {
            let habits = app.list_habits().await?;
            match mode {
                DisplayMode::Rich => display_rich::display_habits(&habits),
                DisplayMode::Simple => display_simple::display_habits(&habits),
            }
        }
        HabitCommands::Show { id } => {
            let habit = app.get_habit(&id).await?;
            match mode {
                DisplayMode::Rich => display_rich::display_habit_detail(&habit),
                DisplayMode::Simple => display_simple::display_habit_detail(&habit),
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
            let avg_minutes = parse_duration(&avg_time)
                .map_err(|e| AppError::BadRequest(e))?;
            let sigma_minutes: i64 = parse_duration(&sigma_time)
                .map_err(|e| AppError::BadRequest(e))?;
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
            };
            let habit = app.create_habit(&body).await?;
            match mode {
                DisplayMode::Rich => display_rich::display_habit_detail(&habit),
                DisplayMode::Simple => display_simple::display_habit_detail(&habit),
            }
        }
        HabitCommands::Edit { id } => {
            let habit = app.get_habit(&id).await?;
            let original = editor::format_habit_for_editing(&habit);
            let edited = editor::open_editor(&original, &habit.id).map_err(|e| {
                AppError::BadRequest(e.to_string())
            })?;
            let update = editor::parse_edited_habit(&edited).ok_or_else(|| {
                AppError::BadRequest("failed to parse edited habit".to_string())
            })?;
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
        } => {
            let avg_minutes = avg_time
                .as_ref()
                .map(|s| parse_duration(s))
                .transpose()
                .map_err(|e| AppError::BadRequest(e))?;
            let sigma_minutes = sigma_time
                .as_ref()
                .map(|s| parse_duration(s))
                .transpose()
                .map_err(|e| AppError::BadRequest(e))?;
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
        } => {
            let avg_minutes = parse_duration(&avg_time)
                .map_err(|e| AppError::BadRequest(e))?;
            let sigma_minutes: i64 = parse_duration(&sigma_time)
                .map_err(|e| AppError::BadRequest(e))?;
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
    }
    Ok(())
}

async fn run_schedule(
    mode: DisplayMode,
    app: &TakusuApp,
    tz: &jiff::tz::TimeZone,
    cmd: ScheduleCommands,
) -> Result<(), AppError> {
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
                DisplayMode::Rich => display_rich::display_schedule(&entries, &tasks, tz),
                DisplayMode::Simple => display_simple::display_schedule(&entries, &tasks, tz),
            }
        }
        ScheduleCommands::Generate {
            range,
            until,
            task_ids,
            sleep,
        } => {
            let _until = if let Some(range) = range {
                let (_from, u) = parse_range_tz(&range, tz)
                    .map_err(|e| AppError::BadRequest(e))?;
                u
            } else {
                match until {
                    Some(s) => parse_dt(&s, tz)?,
                    None => {
                        let now = jiff::Timestamp::now();
                        let until_secs = now.as_second().saturating_add(7 * 86400);
                        jiff::Timestamp::from_second(until_secs)
                            .unwrap_or(now)
                            .to_string()
                    }
                }
            };
            let body = takusu_local_lib::app::GenerateScheduleInput {
                task_ids,
                sleep,
            };
            let schedule = app.generate_schedule(&body).await?;
            let entries: Vec<ScheduleEntry> =
                serde_json::from_str(&schedule.schedule).unwrap_or_default();
            let tasks = app
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
                DisplayMode::Rich => display_rich::display_schedule(&entries, &tasks, tz),
                DisplayMode::Simple => display_simple::display_schedule(&entries, &tasks, tz),
            }
        }
        ScheduleCommands::Move {
            task_id,
            start_at,
            force,
        } => {
            let result = app.move_entry(&task_id, &parse_dt(&start_at, tz)?, force).await?;
            println!("{}", serde_json::to_string_pretty(&result).unwrap());
        }
        ScheduleCommands::Clear => {
            app.clear_schedule().await?;
            println!("Schedule cleared.");
        }
    }
    Ok(())
}

async fn run_token(
    mode: DisplayMode,
    app: &TakusuApp,
    cmd: TokenCommands,
) -> Result<(), AppError> {
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
        SyncCommands::OauthUrl { redirect_uri } => {
            let result = app.oauth_url(&redirect_uri).await?;
            println!("{}", serde_json::to_string_pretty(&result).unwrap());
        }
        SyncCommands::OauthCallback { code, redirect_uri } => {
            app.oauth_callback(&code, &redirect_uri).await?;
            println!("OAuth callback completed successfully.");
        }
        SyncCommands::Trigger => {
            app.do_sync().await.map_err(|e| AppError::Internal(e))?;
            println!("Sync triggered.");
        }
    }
    Ok(())
}

async fn run_config(cmd: ConfigCommands, app: &TakusuApp) -> Result<(), AppError> {
    match cmd {
        ConfigCommands::Show => config::show(),
        ConfigCommands::Init => config::init(),
        ConfigCommands::Set {
            tz,
            sleep_start,
            sleep_end,
        } => {
            if let Some(ref v) = tz {
                config::set("tz", v).map_err(|e| AppError::BadRequest(e))?;
            }
            if let Some(ref v) = sleep_start {
                config::set("sleep_start", v).map_err(|e| AppError::BadRequest(e))?;
            }
            if let Some(ref v) = sleep_end {
                config::set("sleep_end", v).map_err(|e| AppError::BadRequest(e))?;
            }
            let mut update = UpdateSettings {
                tz,
                sleep_start,
                sleep_end,
            };
            let cfg = config::load();
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
