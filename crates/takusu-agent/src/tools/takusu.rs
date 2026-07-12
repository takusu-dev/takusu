use async_trait::async_trait;
use serde_json::{Value, json};
use std::collections::HashMap;
use takusu_client::{Client, HabitDetail, TaskQuery};
use takusu_util::parse_datetime_tz;

use crate::{Tool, ToolError, ToolOutput, ToolRegistry};

/// Registers the read-only planner tools used by the agent.
pub fn register_read_tools(registry: &mut ToolRegistry, client: Client) {
    registry.register(Box::new(ListTasks {
        client: client.clone(),
    }));
    registry.register(Box::new(GetTask {
        client: client.clone(),
    }));
    registry.register(Box::new(ListHabits {
        client: client.clone(),
    }));
    registry.register(Box::new(GetHabit {
        client: client.clone(),
    }));
    registry.register(Box::new(GetSchedule {
        client: client.clone(),
    }));
    registry.register(Box::new(GetSettings { client }));
}

fn object(args: Value) -> Result<serde_json::Map<String, Value>, ToolError> {
    args.as_object()
        .cloned()
        .ok_or_else(|| ToolError::InvalidArgs("arguments must be an object".into()))
}

fn required_string(args: &serde_json::Map<String, Value>, name: &str) -> Result<String, ToolError> {
    args.get(name)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .ok_or_else(|| ToolError::InvalidArgs(format!("missing or empty {name}")))
}

fn optional_string(
    args: &serde_json::Map<String, Value>,
    name: &str,
) -> Result<Option<String>, ToolError> {
    match args.get(name) {
        None | Some(Value::Null) => Ok(None),
        Some(value) => value
            .as_str()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|value| Some(value.to_owned()))
            .ok_or_else(|| ToolError::InvalidArgs(format!("{name} must be a string"))),
    }
}

fn client_error(error: takusu_client::ClientError) -> ToolError {
    match error {
        takusu_client::ClientError::Api {
            status: 400..=499,
            body,
        } => {
            if body.contains("not found") || body.contains("Not found") {
                ToolError::NotFound(body)
            } else {
                ToolError::InvalidArgs(body)
            }
        }
        error => ToolError::Other(Box::new(error)),
    }
}

async fn server_timezone(client: &Client) -> Result<jiff::tz::TimeZone, ToolError> {
    let settings = client.get_settings().await.map_err(client_error)?;
    jiff::tz::TimeZone::get(&settings.tz).map_err(|error| ToolError::Other(Box::new(error)))
}

async fn normalize_datetime(
    client: &Client,
    value: Option<String>,
    name: &str,
) -> Result<Option<String>, ToolError> {
    let Some(value) = value else { return Ok(None) };
    let timezone = server_timezone(client).await?;
    parse_datetime_tz(&value, &timezone)
        .map(Some)
        .map_err(|error| ToolError::InvalidArgs(format!("invalid {name}: {error}")))
}

fn task_json(task: &takusu_client::TaskRow, habit_display_ids: &HashMap<String, i64>) -> Value {
    let reference = task
        .habit_id
        .as_ref()
        .and_then(|habit_id| habit_display_ids.get(habit_id))
        .map(|habit_display_id| format!("h{habit_display_id}#{}", task.display_id))
        .unwrap_or_else(|| format!("#{}", task.display_id));
    json!({
        "display_id": task.display_id,
        "reference": reference,
        "habit_id": task.habit_id,
        "title": task.title,
        "description": task.description,
        "start_at": task.start_at,
        "end_at": task.end_at,
        "avg_minutes": task.avg_minutes,
        "sigma_minutes": task.sigma_minutes,
        "depends": task.depends,
        "parallelizable": task.parallelizable,
        "allows_parallel": task.allows_parallel,
        "abandonability": task.abandonability,
        "status": task.status,
        "fixed": task.fixed,
        "habit_step_id": task.habit_step_id,
        "created_at": task.created_at,
        "updated_at": task.updated_at,
    })
}

fn habit_json(habit: &HabitDetail) -> Value {
    json!({
        "display_id": habit.habit.display_id,
        "reference": format!("h{}", habit.habit.display_id),
        "title": habit.habit.title,
        "description": habit.habit.description,
        "recurrence": habit.habit.recurrence,
        "start_time": habit.habit.start_time,
        "end_time": habit.habit.end_time,
        "avg_minutes": habit.habit.avg_minutes,
        "sigma_minutes": habit.habit.sigma_minutes,
        "parallelizable": habit.habit.parallelizable,
        "allows_parallel": habit.habit.allows_parallel,
        "abandonability": habit.habit.abandonability,
        "active": habit.habit.active,
        "fixed": habit.habit.fixed,
        "window_mode": habit.habit.window_mode,
        "steps": habit.steps,
    })
}

struct ListTasks {
    client: Client,
}

#[async_trait]
impl Tool for ListTasks {
    fn name(&self) -> &'static str {
        "list_tasks"
    }
    fn description(&self) -> &'static str {
        "List tasks, optionally filtered by status, time range, or habit."
    }
    fn parameters_schema(&self) -> Value {
        json!({"type":"object","properties":{
            "status":{"type":"string","description":"Task status filter."},
            "from":{"type":"string","description":"Start of range; interpreted in server timezone."},
            "until":{"type":"string","description":"End of range; interpreted in server timezone."},
            "habit_id":{"type":"string","description":"Habit reference such as h1."}
        },"additionalProperties":false})
    }
    async fn call(&self, args: Value) -> Result<ToolOutput, ToolError> {
        let args = object(args)?;
        let habit_ref = optional_string(&args, "habit_id")?;
        let habit = match habit_ref {
            Some(reference) => Some(
                self.client
                    .get_habit(&reference)
                    .await
                    .map_err(client_error)?,
            ),
            None => None,
        };
        let query = TaskQuery {
            status: optional_string(&args, "status")?,
            from: normalize_datetime(&self.client, optional_string(&args, "from")?, "from").await?,
            until: normalize_datetime(&self.client, optional_string(&args, "until")?, "until")
                .await?,
            habit_id: habit.as_ref().map(|habit| habit.habit.id.clone()),
        };
        let tasks = self.client.list_tasks(&query).await.map_err(client_error)?;
        let habit_display_ids = match habit {
            Some(habit) => HashMap::from([(habit.habit.id, habit.habit.display_id)]),
            None => self
                .client
                .list_habits()
                .await
                .map_err(client_error)?
                .into_iter()
                .map(|habit| (habit.id, habit.display_id))
                .collect(),
        };
        let content = tasks
            .iter()
            .map(|task| task_json(task, &habit_display_ids))
            .collect::<Vec<_>>();
        Ok(ToolOutput {
            content: serde_json::to_string(&content).unwrap(),
            ..Default::default()
        })
    }
}

struct GetTask {
    client: Client,
}

#[async_trait]
impl Tool for GetTask {
    fn name(&self) -> &'static str {
        "get_task"
    }
    fn description(&self) -> &'static str {
        "Get one task by global #display_id or habit-scoped h<habit_display_id>#<task_display_id>."
    }
    fn parameters_schema(&self) -> Value {
        json!({"type":"object","properties":{"task_ref":{"type":"string","description":"#42 or h1#5"}},"required":["task_ref"],"additionalProperties":false})
    }
    async fn call(&self, args: Value) -> Result<ToolOutput, ToolError> {
        let task_ref = required_string(&object(args)?, "task_ref")?;
        if let Some(stripped) = task_ref.strip_prefix('#') {
            return self.get(stripped.to_string()).await;
        }
        self.get(task_ref).await
    }
}

impl GetTask {
    async fn get(&self, task_ref: String) -> Result<ToolOutput, ToolError> {
        let habit_display_id = task_ref
            .strip_prefix(['h', 'H'])
            .and_then(|value| value.split_once('#'))
            .and_then(|(habit, _)| habit.parse::<i64>().ok());
        let task = self
            .client
            .get_task(&task_ref)
            .await
            .map_err(client_error)?;
        let reference = match (task.habit_id.as_ref(), habit_display_id) {
            (Some(_), Some(habit_display_id)) => format!("h{habit_display_id}#{}", task.display_id),
            _ => format!("#{}", task.display_id),
        };
        let mut result = task_json(&task, &HashMap::new());
        result["reference"] = Value::String(reference);
        Ok(ToolOutput {
            content: serde_json::to_string(&result).unwrap(),
            ..Default::default()
        })
    }
}

struct ListHabits {
    client: Client,
}

#[async_trait]
impl Tool for ListHabits {
    fn name(&self) -> &'static str {
        "list_habits"
    }
    fn description(&self) -> &'static str {
        "List all habits."
    }
    fn parameters_schema(&self) -> Value {
        json!({"type":"object","properties":{},"additionalProperties":false})
    }
    async fn call(&self, args: Value) -> Result<ToolOutput, ToolError> {
        let _ = object(args)?;
        let habits = self.client.list_habits().await.map_err(client_error)?;
        Ok(ToolOutput {
            content: serde_json::to_string(&habits).unwrap(),
            ..Default::default()
        })
    }
}

struct GetHabit {
    client: Client,
}

#[async_trait]
impl Tool for GetHabit {
    fn name(&self) -> &'static str {
        "get_habit"
    }
    fn description(&self) -> &'static str {
        "Get one habit by h<display_id>."
    }
    fn parameters_schema(&self) -> Value {
        json!({"type":"object","properties":{"habit_ref":{"type":"string","description":"Habit reference such as h1"}},"required":["habit_ref"],"additionalProperties":false})
    }
    async fn call(&self, args: Value) -> Result<ToolOutput, ToolError> {
        let habit_ref = required_string(&object(args)?, "habit_ref")?;
        let habit = self
            .client
            .get_habit(&habit_ref)
            .await
            .map_err(client_error)?;
        Ok(ToolOutput {
            content: serde_json::to_string(&habit_json(&habit)).unwrap(),
            ..Default::default()
        })
    }
}

struct GetSchedule {
    client: Client,
}

#[async_trait]
impl Tool for GetSchedule {
    fn name(&self) -> &'static str {
        "get_schedule"
    }
    fn description(&self) -> &'static str {
        "Get the current generated schedule with absolute timestamps."
    }
    fn parameters_schema(&self) -> Value {
        json!({"type":"object","properties":{},"additionalProperties":false})
    }
    async fn call(&self, args: Value) -> Result<ToolOutput, ToolError> {
        let _ = object(args)?;
        let schedule = self.client.get_schedule().await.map_err(client_error)?;
        let entries: Value = serde_json::from_str(&schedule.schedule)
            .map_err(|error| ToolError::Other(Box::new(error)))?;
        Ok(ToolOutput { content: serde_json::to_string(&json!({"id": schedule.id, "created_at": schedule.created_at, "updated_at": schedule.updated_at, "entries": entries})).unwrap(), ..Default::default() })
    }
}

struct GetSettings {
    client: Client,
}

#[async_trait]
impl Tool for GetSettings {
    fn name(&self) -> &'static str {
        "get_settings"
    }
    fn description(&self) -> &'static str {
        "Get server timezone and sleep/work settings."
    }
    fn parameters_schema(&self) -> Value {
        json!({"type":"object","properties":{},"additionalProperties":false})
    }
    async fn call(&self, args: Value) -> Result<ToolOutput, ToolError> {
        let _ = object(args)?;
        let settings = self.client.get_settings().await.map_err(client_error)?;
        Ok(ToolOutput {
            content: serde_json::to_string(&settings).unwrap(),
            ..Default::default()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn task_reference_schema_requires_scoped_reference() {
        let tool = GetTask {
            client: Client::new("http://localhost", ""),
        };
        let schema = tool.parameters_schema();
        assert_eq!(schema["required"], json!(["task_ref"]));
        assert_eq!(schema["properties"]["task_ref"]["type"], "string");
    }

    #[test]
    fn task_reference_parser_accepts_global_and_scoped_forms() {
        assert_eq!("#42".trim_start_matches('#'), "42");
        assert!("h1#5".starts_with('h'));
    }
}
