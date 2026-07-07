use std::env;
use std::fs;
use std::io;
use std::process::Command;
use takusu_storage::{HabitRow, TaskRow, UpdateHabit, UpdateTask};

pub fn format_task_for_editing(task: &TaskRow, all_tasks: &[TaskRow]) -> String {
    let depends_uuids: Vec<String> =
        serde_json::from_str::<Vec<String>>(&task.depends).unwrap_or_default();
    // Show display_ids when the dependency task is known, otherwise fall back to UUID.
    let depends: String = depends_uuids
        .iter()
        .map(|uuid| {
            all_tasks
                .iter()
                .find(|t| &t.id == uuid)
                .map(|t| format!("#{}", t.display_id))
                .unwrap_or_else(|| uuid.clone())
        })
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        r#"# Edit task. Lines starting with '#' are comments.
# Empty fields will not be updated. Save and quit to apply changes.
# depends: comma-separated display IDs (e.g. #3, #17, #42) or full UUIDs
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
fixed: {fixed}
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
        fixed = task.fixed,
    )
}

pub fn parse_edited_task(content: &str) -> Result<UpdateTask, String> {
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
    let mut fixed = None;
    let mut depends = None;

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (key, value) = match line.split_once(':') {
            Some(kv) => kv,
            None => {
                eprintln!("warning: skipping malformed line (no ':'): {line}");
                continue;
            }
        };
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
            "end_at" => {
                end_at = if value.is_empty() {
                    Some(None)
                } else {
                    Some(Some(value.to_string()))
                }
            }
            "status" => status = Some(value.to_string()),
            "avg_minutes" => {
                avg_minutes = Some(
                    value
                        .parse()
                        .map_err(|e| format!("invalid avg_minutes '{value}': {e}"))?,
                )
            }
            "sigma_minutes" => {
                sigma_minutes = Some(
                    value
                        .parse()
                        .map_err(|e| format!("invalid sigma_minutes '{value}': {e}"))?,
                )
            }
            "parallelizable" => {
                parallelizable = Some(
                    value
                        .parse()
                        .map_err(|e| format!("invalid parallelizable '{value}': {e}"))?,
                )
            }
            "allows_parallel" => {
                allows_parallel = Some(
                    value
                        .parse()
                        .map_err(|e| format!("invalid allows_parallel '{value}': {e}"))?,
                )
            }
            "abandonability" => {
                abandonability = Some(
                    value
                        .parse()
                        .map_err(|e| format!("invalid abandonability '{value}': {e}"))?,
                )
            }
            "fixed" => {
                fixed = Some(
                    value
                        .parse()
                        .map_err(|e| format!("invalid fixed '{value}': {e}"))?,
                )
            }
            "depends" => {
                let items: Vec<String> = if value.is_empty() {
                    vec![]
                } else {
                    value
                        .split(',')
                        .map(|s| s.trim().trim_start_matches('#').to_string())
                        .filter(|s| !s.is_empty())
                        .collect()
                };
                depends = Some(items);
            }
            _ => {}
        }
    }

    Ok(UpdateTask {
        title,
        description: description.flatten(),
        start_at: start_at.flatten(),
        end_at: end_at.flatten(),
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
    })
}

pub fn format_habit_for_editing(habit: &HabitRow) -> String {
    format!(
        r#"# Edit habit. Lines starting with '#' are comments.
# Empty fields will not be updated. Save and quit to apply changes.
# window_mode: 'day' (occurrence day) or 'period' (until next occurrence)
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
fixed: {fixed}
active: {active}
window_mode: {window_mode}"#,
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
        fixed = habit.fixed,
        active = habit.active,
        window_mode = habit.window_mode,
    )
}

pub fn parse_edited_habit(content: &str) -> Result<UpdateHabit, String> {
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
    let mut fixed = None;
    let mut active = None;
    let mut window_mode = None;

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (key, value) = match line.split_once(':') {
            Some(kv) => kv,
            None => {
                eprintln!("warning: skipping malformed line (no ':'): {line}");
                continue;
            }
        };
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
            "start_time" => {
                start_time = if value.is_empty() {
                    Some(None)
                } else {
                    Some(Some(value.to_string()))
                }
            }
            "end_time" => {
                end_time = if value.is_empty() {
                    Some(None)
                } else {
                    Some(Some(value.to_string()))
                }
            }
            "avg_minutes" => {
                avg_minutes = Some(
                    value
                        .parse()
                        .map_err(|e| format!("invalid avg_minutes '{value}': {e}"))?,
                )
            }
            "sigma_minutes" => {
                sigma_minutes = Some(
                    value
                        .parse()
                        .map_err(|e| format!("invalid sigma_minutes '{value}': {e}"))?,
                )
            }
            "parallelizable" => {
                parallelizable = Some(
                    value
                        .parse()
                        .map_err(|e| format!("invalid parallelizable '{value}': {e}"))?,
                )
            }
            "allows_parallel" => {
                allows_parallel = Some(
                    value
                        .parse()
                        .map_err(|e| format!("invalid allows_parallel '{value}': {e}"))?,
                )
            }
            "abandonability" => {
                abandonability = Some(
                    value
                        .parse()
                        .map_err(|e| format!("invalid abandonability '{value}': {e}"))?,
                )
            }
            "fixed" => {
                fixed = Some(
                    value
                        .parse()
                        .map_err(|e| format!("invalid fixed '{value}': {e}"))?,
                )
            }
            "active" => {
                active = Some(
                    value
                        .parse()
                        .map_err(|e| format!("invalid active '{value}': {e}"))?,
                )
            }
            "window_mode" if !value.is_empty() => {
                window_mode = Some(value.to_string());
            }
            _ => {}
        }
    }

    Ok(UpdateHabit {
        title,
        description: description.flatten(),
        recurrence,
        start_time: start_time.flatten(),
        end_time: end_time.flatten(),
        avg_minutes,
        sigma_minutes,
        parallelizable,
        allows_parallel,
        abandonability,
        active,
        fixed,
        window_mode,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_edited_task_empty_end_at_is_skipped() {
        let input = "title: t\nend_at:\n";
        let update = parse_edited_task(input).unwrap();
        assert_eq!(update.title.as_deref(), Some("t"));
        assert_eq!(update.end_at, None, "empty end_at should skip update");
    }

    #[test]
    fn parse_edited_task_nonempty_end_at_is_set() {
        let input = "title: t\nend_at: 2026-07-06T18:00:00\n";
        let update = parse_edited_task(input).unwrap();
        assert_eq!(
            update.end_at.as_deref(),
            Some("2026-07-06T18:00:00"),
            "non-empty end_at should be set"
        );
    }

    #[test]
    fn parse_edited_task_empty_start_at_is_skipped() {
        let input = "title: t\nstart_at:\n";
        let update = parse_edited_task(input).unwrap();
        assert_eq!(update.start_at, None, "empty start_at should skip update");
    }

    #[test]
    fn parse_edited_habit_empty_times_are_skipped() {
        let input = "title: h\nstart_time:\nend_time:\n";
        let update = parse_edited_habit(input).unwrap();
        assert_eq!(
            update.start_time, None,
            "empty start_time should skip update"
        );
        assert_eq!(update.end_time, None, "empty end_time should skip update");
    }

    #[test]
    fn parse_edited_habit_nonempty_times_are_set() {
        let input = "title: h\nstart_time: 09:00\nend_time: 10:00\n";
        let update = parse_edited_habit(input).unwrap();
        assert_eq!(update.start_time.as_deref(), Some("09:00"));
        assert_eq!(update.end_time.as_deref(), Some("10:00"));
    }

    // ── Per-line error reporting (#347) ─────────────────────────────────

    #[test]
    fn parse_edited_task_line_without_colon_is_skipped() {
        // A malformed line should NOT discard the whole edit.
        let input = "title: t\nthis line has no colon\navg_minutes: 30\n";
        let update = parse_edited_task(input).unwrap();
        assert_eq!(update.title.as_deref(), Some("t"));
        assert_eq!(update.avg_minutes, Some(30));
    }

    #[test]
    fn parse_edited_task_bad_numeric_field_reports_error() {
        let input = "title: t\navg_minutes: abc\n";
        let err = parse_edited_task(input).unwrap_err();
        assert!(
            err.contains("avg_minutes"),
            "error should mention the field: {err}"
        );
        assert!(
            err.contains("abc"),
            "error should mention the bad value: {err}"
        );
    }

    #[test]
    fn parse_edited_task_bad_field_does_not_discard_valid_fields() {
        // Even when one numeric field is bad, the error should be returned
        // (we do not silently drop the valid `title`). The caller can show
        // the error so the user fixes the one bad line and re-edits.
        let input = "title: t\nsigma_minutes: xyz\nfixed: true\n";
        let err = parse_edited_task(input).unwrap_err();
        assert!(err.contains("sigma_minutes"), "error: {err}");
    }

    #[test]
    fn parse_edited_habit_line_without_colon_is_skipped() {
        let input = "title: h\nno colon here\nactive: true\n";
        let update = parse_edited_habit(input).unwrap();
        assert_eq!(update.title.as_deref(), Some("h"));
        assert_eq!(update.active, Some(true));
    }

    #[test]
    fn parse_edited_habit_bad_bool_field_reports_error() {
        let input = "title: h\nactive: maybe\n";
        let err = parse_edited_habit(input).unwrap_err();
        assert!(err.contains("active"), "error: {err}");
        assert!(err.contains("maybe"), "error: {err}");
    }
}
