use jiff::Timestamp;
use takusu_storage::{HabitRow, ScheduleEntry, SkillRow, TaskRow, TokenRow};

/// Build the display label for a task ID.
/// Habit-generated tasks show `h{habit_display_id}#{task_display_id}` (#305);
/// other tasks show `#{task_display_id}`.
fn task_id_label(task: &TaskRow, habit_map: &std::collections::HashMap<String, i64>) -> String {
    if let Some(hid) = task.habit_id.as_deref()
        && let Some(&hdisplay) = habit_map.get(hid)
    {
        format!("h{}#{}", hdisplay, task.display_id)
    } else {
        format!("#{}", task.display_id)
    }
}

pub fn display_task_detail(
    task: &TaskRow,
    entry: Option<&ScheduleEntry>,
    tz: &jiff::tz::TimeZone,
    habit_map: &std::collections::HashMap<String, i64>,
) {
    let status_marker = match task.status.as_str() {
        "pending" => "[ ]",
        "scheduled" => "[~]",
        "in_progress" => "[>]",
        "completed" => "[x]",
        "skipped" => "[-]",
        _ => "[?]",
    };
    println!(
        "{} {} {}",
        status_marker,
        task_id_label(task, habit_map),
        task.title
    );
    println!(
        "   deadline: {} | est: {}min (+/-{}) | abandon: {:.1} | parallel: {}",
        fmt_simple(&task.end_at, tz),
        task.avg_minutes,
        task.sigma_minutes,
        task.abandonability,
        if task.parallelizable { "yes" } else { "no" },
    );
    if let Some(ref start) = task.start_at {
        println!("   start: {}", fmt_simple(start, tz));
    }
    if let Some(ref desc) = task.description {
        println!("   {desc}");
    }

    if let Some(entry) = entry {
        println!(
            "   scheduled: {} -- {} ({})",
            fmt_simple(&entry.start_at, tz),
            fmt_simple(&entry.end_at, tz),
            fmt_duration(&entry.start_at, &entry.end_at)
        );
    }
    println!();
}

pub fn display_tasks(
    tasks: &[TaskRow],
    tz: &jiff::tz::TimeZone,
    habit_map: &std::collections::HashMap<String, i64>,
) {
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
        let short_id = task_id_label(t, habit_map);
        println!("{} {} {}", status_marker, short_id, t.title);
        println!(
            "   deadline: {} | est: {}min (+/-{}) | abandon: {:.1}",
            fmt_simple(&t.end_at, tz),
            t.avg_minutes,
            t.sigma_minutes,
            t.abandonability
        );
        if let Some(ref desc) = t.description {
            println!("   {desc}");
        }
        println!();
    }
}

pub fn display_schedule(
    entries: &[ScheduleEntry],
    tasks: &[TaskRow],
    tz: &jiff::tz::TimeZone,
    habit_map: &std::collections::HashMap<String, i64>,
) {
    if entries.is_empty() {
        println!("  (no schedule)");
        return;
    }

    let mut sorted = entries.to_vec();
    sorted.sort_by(|a, b| a.start_at.cmp(&b.start_at));

    let task_map: std::collections::HashMap<&str, &TaskRow> =
        tasks.iter().map(|t| (t.id.as_str(), t)).collect();

    for (i, e) in sorted.iter().enumerate() {
        let task = task_map.get(e.task_id.as_str());
        let title = task.map(|t| t.title.as_str()).unwrap_or("(unknown)");
        let id_label = task
            .map(|t| task_id_label(t, habit_map))
            .unwrap_or_else(|| e.task_id[..8].to_string());
        let start = fmt_simple(&e.start_at, tz);
        let end = fmt_simple(&e.end_at, tz);
        let dur = fmt_duration(&e.start_at, &e.end_at);
        println!("  {:>3}. {} -- {} [{}] {}", i + 1, start, end, dur, title);
        println!("       id: {}", id_label);
    }
}

pub fn display_tokens(tokens: &[TokenRow]) {
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

fn fmt_simple(iso: &str, tz: &jiff::tz::TimeZone) -> String {
    iso.parse::<Timestamp>()
        .map(|ts| {
            let zdt = ts.to_zoned(tz.clone());
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

pub fn display_habits(habits: &[HabitRow]) {
    if habits.is_empty() {
        println!("  (no habits)");
        return;
    }

    for h in habits {
        let active = if h.active { "active" } else { "inactive" };
        let short_id = format!("h{}", h.display_id);
        println!(
            "  {} {} [{}] {}–{} {}",
            short_id, h.title, h.recurrence, h.start_time, h.end_time, active
        );
        println!(
            "   est: {}min (+/-{}) | abandon: {:.1} | parallel: {}",
            h.avg_minutes,
            h.sigma_minutes,
            h.abandonability,
            if h.parallelizable { "yes" } else { "no" },
        );
        if let Some(ref desc) = h.description
            && !desc.is_empty()
        {
            println!("   {desc}");
        }
        println!();
    }
}

pub fn display_habit_detail(habit: &HabitRow) {
    let active = if habit.active { "active" } else { "inactive" };
    println!(
        "h{} {} [{}] {}–{} {}",
        habit.display_id, habit.title, habit.recurrence, habit.start_time, habit.end_time, active
    );
    println!(
        "   est: {}min (+/-{}) | abandon: {:.1} | parallel: {} | allows_parallel: {} | window: {}",
        habit.avg_minutes,
        habit.sigma_minutes,
        habit.abandonability,
        if habit.parallelizable { "yes" } else { "no" },
        if habit.allows_parallel { "yes" } else { "no" },
        habit.window_mode,
    );
    if let Some(ref desc) = habit.description
        && !desc.is_empty()
    {
        println!("   {desc}");
    }
    println!();
}

pub fn display_skills(skills: &[SkillRow]) {
    if skills.is_empty() {
        println!("  (no skills)");
        return;
    }
    for s in skills {
        let marker = if s.built_in { "[b]" } else { "[u]" };
        println!("{} {} {}: {}", marker, s.slug, s.name, s.description);
    }
}

pub fn display_skill_detail(skill: &SkillRow) {
    let marker = if skill.built_in { "built-in" } else { "user" };
    println!("{} {} ({})", skill.slug, skill.name, marker);
    println!("  {}\n", skill.description);
    println!("{}", skill.body);
}
