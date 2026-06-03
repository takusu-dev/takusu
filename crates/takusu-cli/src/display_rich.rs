use comfy_table::{Cell, Color, ContentArrangement, Table, presets::UTF8_FULL};
use jiff::Timestamp;
use takusu_client::{ScheduleEntry, TaskRow};

pub fn display_tasks(tasks: &[TaskRow]) {
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
            "completed" => Color::DarkCyan,
            "skipped" => Color::DarkGrey,
            _ => Color::White,
        };
        let short_id = &t.id[..8];
        table.add_row(vec![
            Cell::new(short_id),
            Cell::new(&t.title),
            Cell::new(&t.status).fg(status_color),
            Cell::new(t.start_at.as_deref().unwrap_or("—")),
            Cell::new(&t.end_at),
            Cell::new(t.avg_minutes),
            Cell::new(t.sigma_minutes),
            Cell::new(if t.parallelizable { "✓" } else { "✗" }),
            Cell::new(format!("{:.1}", t.abandonability)),
        ]);
    }
    println!("{table}");
}

pub fn display_schedule(entries: &[ScheduleEntry], tasks: &[TaskRow]) {
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
        let title = task_map
            .get(e.task_id.as_str())
            .map(|t| t.title.as_str())
            .unwrap_or("(unknown)");
        let short_id = &e.task_id[..8];
        let start = format_datetime(&e.start_at);
        let end = format_datetime(&e.end_at);
        let dur = format_duration(&e.start_at, &e.end_at);
        table.add_row(vec![
            Cell::new(i + 1),
            Cell::new(title),
            Cell::new(short_id),
            Cell::new(start),
            Cell::new(end),
            Cell::new(dur),
        ]);
    }
    println!("{table}");
}

fn format_datetime(iso: &str) -> String {
    iso.parse::<Timestamp>()
        .map(|ts| {
            let zdt = ts
                .in_tz("UTC")
                .unwrap_or_else(|_| ts.to_zoned(jiff::tz::TimeZone::UTC));
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

pub fn display_tokens(tokens: &[takusu_client::TokenRow]) {
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
