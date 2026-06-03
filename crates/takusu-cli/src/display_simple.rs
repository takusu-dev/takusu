use jiff::Timestamp;
use takusu_client::{ScheduleEntry, TaskRow};

pub fn display_tasks(tasks: &[TaskRow]) {
    if tasks.is_empty() {
        println!("  (no tasks)");
        return;
    }

    for t in tasks {
        let status_marker = match t.status.as_str() {
            "pending" => "[ ]",
            "scheduled" => "[~]",
            "in_progress" => "[>]",
            "completed" => "[x]",
            "skipped" => "[-]",
            _ => "[?]",
        };
        let short_id = &t.id[..8];
        println!("{} {} {}", status_marker, short_id, t.title);
        println!(
            "   deadline: {} | est: {}min (+/-{}) | abandon: {:.1}",
            t.end_at, t.avg_minutes, t.sigma_minutes, t.abandonability
        );
        if let Some(ref desc) = t.description {
            println!("   {desc}");
        }
        println!();
    }
}

pub fn display_schedule(entries: &[ScheduleEntry], tasks: &[TaskRow]) {
    if entries.is_empty() {
        println!("  (no schedule)");
        return;
    }

    let mut sorted = entries.to_vec();
    sorted.sort_by(|a, b| a.start_at.cmp(&b.start_at));

    let task_map: std::collections::HashMap<&str, &TaskRow> =
        tasks.iter().map(|t| (t.id.as_str(), t)).collect();

    for (i, e) in sorted.iter().enumerate() {
        let title = task_map
            .get(e.task_id.as_str())
            .map(|t| t.title.as_str())
            .unwrap_or("(unknown)");
        let short_id = &e.task_id[..8];
        let start = fmt_simple(&e.start_at);
        let end = fmt_simple(&e.end_at);
        let dur = fmt_duration(&e.start_at, &e.end_at);
        println!("  {:>3}. {} -- {} [{}] {}", i + 1, start, end, dur, title);
        println!("       id: {}", short_id);
    }
}

pub fn display_tokens(tokens: &[takusu_client::TokenRow]) {
    if tokens.is_empty() {
        println!("  (no tokens)");
        return;
    }
    for t in tokens {
        let revoked = t.revoked_at.as_deref().map(|_| " [REVOKED]").unwrap_or("");
        println!(
            "  #{} {:8}  {}{}",
            t.id,
            t.label.as_deref().unwrap_or("-"),
            &t.created_at,
            revoked
        );
    }
}

fn fmt_simple(iso: &str) -> String {
    iso.parse::<Timestamp>()
        .map(|ts| {
            let zdt = ts
                .in_tz("UTC")
                .unwrap_or_else(|_| ts.to_zoned(jiff::tz::TimeZone::UTC));
            zdt.strftime("%d %H:%M").to_string()
        })
        .unwrap_or_else(|_| iso.to_string())
}

fn fmt_duration(start_iso: &str, end_iso: &str) -> String {
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
