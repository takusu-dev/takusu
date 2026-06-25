use std::env;
use std::fs;
use std::io;
use std::process::Command;
use takusu_storage::{HabitRow, TaskRow, UpdateHabit, UpdateTask};

pub fn format_task_for_editing(task: &TaskRow) -> String {
    let depends: String = serde_json::from_str::<Vec<String>>(&task.depends)
        .map(|v| v.join(", "))
        .unwrap_or_default();
    format!(
        r#"# Edit task. Lines starting with '#' are comments.
# Empty fields will not be updated. Save and quit to apply changes.
title: {title}
description: {desc}
start_at: {start}
end_at: {end}
status: {status}
avg_minutes: {avg}
sigma_minutes: {sigma}
parallelizable: {par}
allows_parallel: {apar}
abandonability: {abandon}
depends: {depends}"#,
        title = task.title,
        desc = task.description.as_deref().unwrap_or(""),
        start = task.start_at.as_deref().unwrap_or(""),
        end = task.end_at,
        status = task.status,
        avg = task.avg_minutes,
        sigma = task.sigma_minutes,
        par = task.parallelizable,
        apar = task.allows_parallel,
        abandon = task.abandonability,
    )
}

pub fn parse_edited_task(content: &str) -> Option<UpdateTask> {
    let mut title = None;
    let mut description = None;
    let mut start_at = None;
    let mut end_at = None;
    let mut status = None;
    let mut avg_minutes = None;
    let mut sigma_minutes = None;
    let mut parallelizable = None;
    let mut allows_parallel = None;
    let mut abandonability = None;
    let mut depends = None;

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (key, value) = line.split_once(':')?;
        let key = key.trim();
        let value = value.trim();
        match key {
            "title" => title = Some(value.to_string()),
            "description" => {
                description = if value.is_empty() {
                    Some(None)
                } else {
                    Some(Some(value.to_string()))
                }
            }
            "start_at" => {
                start_at = if value.is_empty() {
                    Some(None)
                } else {
                    Some(Some(value.to_string()))
                }
            }
            "end_at" => end_at = Some(value.to_string()),
            "status" => status = Some(value.to_string()),
            "avg_minutes" => avg_minutes = Some(value.parse().ok()?),
            "sigma_minutes" => sigma_minutes = Some(value.parse().ok()?),
            "parallelizable" => parallelizable = Some(value.parse().ok()?),
            "allows_parallel" => allows_parallel = Some(value.parse().ok()?),
            "abandonability" => abandonability = Some(value.parse().ok()?),
            "depends" => {
                let items: Vec<String> = if value.is_empty() {
                    vec![]
                } else {
                    value
                        .split(',')
                        .map(|s| s.trim().to_string())
                        .filter(|s| !s.is_empty())
                        .collect()
                };
                depends = Some(items);
            }
            _ => {}
        }
    }

    Some(UpdateTask {
        title,
        description: description.flatten(),
        start_at: start_at.flatten(),
        end_at,
        avg_minutes,
        sigma_minutes,
        depends,
        parallelizable,
        allows_parallel,
        abandonability,
        status,
    })
}

pub fn format_habit_for_editing(habit: &HabitRow) -> String {
    format!(
        r#"# Edit habit. Lines starting with '#' are comments.
# Empty fields will not be updated. Save and quit to apply changes.
title: {title}
description: {desc}
recurrence: {recurrence}
start_time: {start}
end_time: {end}
avg_minutes: {avg}
sigma_minutes: {sigma}
parallelizable: {par}
allows_parallel: {apar}
abandonability: {abandon}
active: {active}"#,
        title = habit.title,
        desc = habit.description.as_deref().unwrap_or(""),
        recurrence = habit.recurrence,
        start = habit.start_time,
        end = habit.end_time,
        avg = habit.avg_minutes,
        sigma = habit.sigma_minutes,
        par = habit.parallelizable,
        apar = habit.allows_parallel,
        abandon = habit.abandonability,
        active = habit.active,
    )
}

pub fn parse_edited_habit(content: &str) -> Option<UpdateHabit> {
    let mut title = None;
    let mut description = None;
    let mut recurrence = None;
    let mut start_time = None;
    let mut end_time = None;
    let mut avg_minutes = None;
    let mut sigma_minutes = None;
    let mut parallelizable = None;
    let mut allows_parallel = None;
    let mut abandonability = None;
    let mut active = None;

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (key, value) = line.split_once(':')?;
        let key = key.trim();
        let value = value.trim();
        match key {
            "title" => title = Some(value.to_string()),
            "description" => {
                description = if value.is_empty() {
                    Some(None)
                } else {
                    Some(Some(value.to_string()))
                }
            }
            "recurrence" => recurrence = Some(value.to_string()),
            "start_time" => start_time = Some(value.to_string()),
            "end_time" => end_time = Some(value.to_string()),
            "avg_minutes" => avg_minutes = Some(value.parse().ok()?),
            "sigma_minutes" => sigma_minutes = Some(value.parse().ok()?),
            "parallelizable" => parallelizable = Some(value.parse().ok()?),
            "allows_parallel" => allows_parallel = Some(value.parse().ok()?),
            "abandonability" => abandonability = Some(value.parse().ok()?),
            "active" => active = Some(value.parse().ok()?),
            _ => {}
        }
    }

    Some(UpdateHabit {
        title,
        description: description.flatten(),
        recurrence,
        start_time,
        end_time,
        avg_minutes,
        sigma_minutes,
        parallelizable,
        allows_parallel,
        abandonability,
        active,
    })
}

pub fn open_editor(content: &str, suffix: &str) -> io::Result<String> {
    let dir = env::temp_dir();
    let path = dir.join(format!("takusu-edit-{suffix}.txt"));
    fs::write(&path, content)?;

    let editor = env::var("EDITOR").unwrap_or_else(|_| "vi".to_string());
    let status = Command::new("sh")
        .arg("-c")
        .arg(format!("{editor} '{}'", path.display()))
        .status()?;

    if !status.success() {
        fs::remove_file(&path).ok();
        return Err(io::Error::other("editor exited with non-zero status"));
    }

    let edited = fs::read_to_string(&path)?;
    fs::remove_file(&path).ok();
    Ok(edited)
}
