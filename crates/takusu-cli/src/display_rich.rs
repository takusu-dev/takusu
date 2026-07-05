use comfy_table::{Cell, Color, ContentArrangement, Table, presets::UTF8_FULL};
use jiff::Timestamp;
use takusu_habit::{RecurrenceRule, summarize};
use takusu_storage::{HabitRow, ScheduleEntry, TaskRow, TokenRow};

/// Parse a recurrence JSON string into a human-readable summary.
/// Falls back to the raw string if parsing fails.
fn format_recurrence(raw: &str) -> String {
    serde_json::from_str::<RecurrenceRule>(raw)
        .map(|r| summarize(&r))
        .unwrap_or_else(|_| raw.to_string())
}

pub fn display_task_detail(task: &TaskRow, entry: Option<&ScheduleEntry>, tz: &jiff::tz::TimeZone) {
    let status_color = match task.status.as_str() {
        "pending" => Color::Yellow,
        "scheduled" => Color::Green,
        "in_progress" => Color::DarkYellow,
        "completed" => Color::DarkCyan,
        "skipped" => Color::DarkGrey,
        _ => Color::White,
    };

    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL)
        .set_content_arrangement(ContentArrangement::Dynamic);
    table.set_header(vec![
        Cell::new("ID").fg(Color::Cyan),
        Cell::new("Title").fg(Color::Cyan),
        Cell::new("Status").fg(Color::Cyan),
        Cell::new("Start").fg(Color::Cyan),
        Cell::new("Deadline").fg(Color::Cyan),
        Cell::new("Avg (min)").fg(Color::Cyan),
        Cell::new("σ (min)").fg(Color::Cyan),
        Cell::new("Parallel").fg(Color::Cyan),
        Cell::new("Abandon").fg(Color::Cyan),
    ]);
    table.add_row(vec![
        Cell::new(format!("#{}", task.display_id)),
        Cell::new(&task.title),
        Cell::new(&task.status).fg(status_color),
        Cell::new(
            task.start_at
                .as_deref()
                .map(|s| format_datetime(s, tz))
                .unwrap_or_else(|| "—".into()),
        ),
        Cell::new(format_datetime(&task.end_at, tz)),
        Cell::new(task.avg_minutes),
        Cell::new(task.sigma_minutes),
        Cell::new(if task.parallelizable { "✓" } else { "✗" }),
        Cell::new(format!("{:.1}", task.abandonability)),
    ]);
    println!("{table}");

    if let Some(entry) = entry {
        println!();
        let mut table = Table::new();
        table
            .load_preset(UTF8_FULL)
            .set_content_arrangement(ContentArrangement::Dynamic);
        table.set_header(vec![
            Cell::new("Start").fg(Color::Cyan),
            Cell::new("End").fg(Color::Cyan),
            Cell::new("Duration").fg(Color::Cyan),
        ]);
        let start = format_datetime(&entry.start_at, tz);
        let end = format_datetime(&entry.end_at, tz);
        let dur = format_duration(&entry.start_at, &entry.end_at);
        table.add_row(vec![Cell::new(start), Cell::new(end), Cell::new(dur)]);
        println!("{table}");
    }
}

pub fn display_habits(habits: &[HabitRow]) {
    if habits.is_empty() {
        println!("No habits found.");
        return;
    }

    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL)
        .set_content_arrangement(ContentArrangement::Dynamic)
        .set_header(vec![
            Cell::new("ID").fg(Color::Cyan),
            Cell::new("Title").fg(Color::Cyan),
            Cell::new("Recurrence").fg(Color::Cyan),
            Cell::new("Time").fg(Color::Cyan),
            Cell::new("Avg (min)").fg(Color::Cyan),
            Cell::new("σ (min)").fg(Color::Cyan),
            Cell::new("Parallel").fg(Color::Cyan),
            Cell::new("Abandon").fg(Color::Cyan),
            Cell::new("Active").fg(Color::Cyan),
        ]);

    for h in habits {
        let short_id = &h.id[..8];
        let time = format!("{}–{}", h.start_time, h.end_time);
        let active_color = if h.active {
            Color::Green
        } else {
            Color::DarkGrey
        };
        let active_text = if h.active { "yes" } else { "no" };
        table.add_row(vec![
            Cell::new(short_id),
            Cell::new(&h.title),
            Cell::new(format_recurrence(&h.recurrence)),
            Cell::new(time),
            Cell::new(h.avg_minutes),
            Cell::new(h.sigma_minutes),
            Cell::new(if h.parallelizable { "✓" } else { "✗" }),
            Cell::new(format!("{:.1}", h.abandonability)),
            Cell::new(active_text).fg(active_color),
        ]);
    }
    println!("{table}");
}

pub fn display_habit_detail(habit: &HabitRow) {
    let active_color = if habit.active {
        Color::Green
    } else {
        Color::DarkGrey
    };
    let active_text = if habit.active { "yes" } else { "no" };

    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL)
        .set_content_arrangement(ContentArrangement::Dynamic);
    table.set_header(vec![
        Cell::new("ID").fg(Color::Cyan),
        Cell::new("Title").fg(Color::Cyan),
        Cell::new("Recurrence").fg(Color::Cyan),
        Cell::new("Time").fg(Color::Cyan),
        Cell::new("Avg (min)").fg(Color::Cyan),
        Cell::new("σ (min)").fg(Color::Cyan),
        Cell::new("Parallel").fg(Color::Cyan),
        Cell::new("Abandon").fg(Color::Cyan),
        Cell::new("Active").fg(Color::Cyan),
    ]);
    let time = format!("{}–{}", habit.start_time, habit.end_time);
    table.add_row(vec![
        Cell::new(habit.id.as_str()),
        Cell::new(&habit.title),
        Cell::new(format_recurrence(&habit.recurrence)),
        Cell::new(time),
        Cell::new(habit.avg_minutes),
        Cell::new(habit.sigma_minutes),
        Cell::new(if habit.parallelizable { "✓" } else { "✗" }),
        Cell::new(format!("{:.1}", habit.abandonability)),
        Cell::new(active_text).fg(active_color),
    ]);
    println!("{table}");

    if let Some(ref desc) = habit.description
        && !desc.is_empty()
    {
        println!("\nDescription: {desc}");
    }
}

pub fn display_tasks(tasks: &[TaskRow], tz: &jiff::tz::TimeZone) {
    if tasks.is_empty() {
        println!("No tasks found.");
        return;
    }

    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL)
        .set_content_arrangement(ContentArrangement::Dynamic)
        .set_header(vec![
            Cell::new("ID").fg(Color::Cyan),
            Cell::new("Title").fg(Color::Cyan),
            Cell::new("Status").fg(Color::Cyan),
            Cell::new("Start").fg(Color::Cyan),
            Cell::new("Deadline").fg(Color::Cyan),
            Cell::new("Avg (min)").fg(Color::Cyan),
            Cell::new("σ (min)").fg(Color::Cyan),
            Cell::new("Parallel").fg(Color::Cyan),
            Cell::new("Abandon").fg(Color::Cyan),
        ]);

    for t in tasks {
        let status_color = match t.status.as_str() {
            "pending" => Color::Yellow,
            "scheduled" => Color::Green,
            "in_progress" => Color::DarkYellow,
            "completed" => Color::DarkCyan,
            "skipped" => Color::DarkGrey,
            _ => Color::White,
        };
        let short_id = format!("#{}", t.display_id);
        table.add_row(vec![
            Cell::new(short_id),
            Cell::new(&t.title),
            Cell::new(&t.status).fg(status_color),
            Cell::new(
                t.start_at
                    .as_deref()
                    .map(|s| format_datetime(s, tz))
                    .unwrap_or_else(|| "—".into()),
            ),
            Cell::new(format_datetime(&t.end_at, tz)),
            Cell::new(t.avg_minutes),
            Cell::new(t.sigma_minutes),
            Cell::new(if t.parallelizable { "✓" } else { "✗" }),
            Cell::new(format!("{:.1}", t.abandonability)),
        ]);
    }
    println!("{table}");
}

pub fn display_schedule(entries: &[ScheduleEntry], tasks: &[TaskRow], tz: &jiff::tz::TimeZone) {
    if entries.is_empty() {
        println!("No schedule found.");
        return;
    }

    let mut sorted = entries.to_vec();
    sorted.sort_by(|a, b| a.start_at.cmp(&b.start_at));

    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL)
        .set_content_arrangement(ContentArrangement::Dynamic)
        .set_header(vec![
            Cell::new("#").fg(Color::Cyan),
            Cell::new("Title").fg(Color::Cyan),
            Cell::new("Task ID").fg(Color::Cyan),
            Cell::new("Start").fg(Color::Cyan),
            Cell::new("End").fg(Color::Cyan),
            Cell::new("Duration").fg(Color::Cyan),
        ]);

    let task_map: std::collections::HashMap<&str, &TaskRow> =
        tasks.iter().map(|t| (t.id.as_str(), t)).collect();

    for (i, e) in sorted.iter().enumerate() {
        let task = task_map.get(e.task_id.as_str());
        let title = task.map(|t| t.title.as_str()).unwrap_or("(unknown)");
        let id_label = task
            .map(|t| format!("#{}", t.display_id))
            .unwrap_or_else(|| e.task_id[..8].to_string());
        let start = format_datetime(&e.start_at, tz);
        let end = format_datetime(&e.end_at, tz);
        let dur = format_duration(&e.start_at, &e.end_at);
        table.add_row(vec![
            Cell::new(i + 1),
            Cell::new(title),
            Cell::new(id_label),
            Cell::new(start),
            Cell::new(end),
            Cell::new(dur),
        ]);
    }
    println!("{table}");
}

fn format_datetime(iso: &str, tz: &jiff::tz::TimeZone) -> String {
    iso.parse::<Timestamp>()
        .map(|ts| {
            let zdt = ts.to_zoned(tz.clone());
            zdt.strftime("%m/%d %H:%M").to_string()
        })
        .unwrap_or_else(|_| iso.to_string())
}

fn format_duration(start_iso: &str, end_iso: &str) -> String {
    let start: Result<Timestamp, _> = start_iso.parse();
    let end: Result<Timestamp, _> = end_iso.parse();
    match (start, end) {
        (Ok(s), Ok(e)) => {
            let secs = (e.as_second() - s.as_second()).unsigned_abs();
            let mins = secs / 60;
            if mins >= 60 {
                format!("{}h{}m", mins / 60, mins % 60)
            } else {
                format!("{mins}m")
            }
        }
        _ => "?".to_string(),
    }
}

pub fn display_tokens(tokens: &[TokenRow]) {
    if tokens.is_empty() {
        println!("No tokens found.");
        return;
    }

    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL)
        .set_content_arrangement(ContentArrangement::Dynamic)
        .set_header(vec![
            Cell::new("ID").fg(Color::Cyan),
            Cell::new("Label").fg(Color::Cyan),
            Cell::new("Created By").fg(Color::Cyan),
            Cell::new("Created At").fg(Color::Cyan),
            Cell::new("Revoked").fg(Color::Cyan),
        ]);

    for t in tokens {
        let revoked = t
            .revoked_at
            .as_deref()
            .map(|_| Cell::new("YES").fg(Color::Red))
            .unwrap_or_else(|| Cell::new("no").fg(Color::Green));
        table.add_row(vec![
            Cell::new(t.id),
            Cell::new(t.label.as_deref().unwrap_or("—")),
            Cell::new(&t.created_by[..8]),
            Cell::new(&t.created_at),
            revoked,
        ]);
    }
    println!("{table}");
}
