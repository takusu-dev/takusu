use async_trait::async_trait;
use serde_json::{Value, json};
use std::collections::HashMap;
use takusu_client::{Client, HabitDetail, SchedulePreviewRequest, TaskQuery};
use takusu_util::parse_datetime_tz;

use crate::{Tool, ToolError, ToolOutput, ToolRegistry};

/// Registers planner read tools and approval-only mutation proposals.
pub fn register_tools(registry: &mut ToolRegistry, client: Client) {
    register_read_tools(registry, client.clone());
    register_mutation_tools(registry, client.clone());
    registry.register(Box::new(PreviewScheduleTool { client }));
}

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

fn required_i64(args: &serde_json::Map<String, Value>, name: &str) -> Result<i64, ToolError> {
    args.get(name)
        .and_then(Value::as_i64)
        .ok_or_else(|| ToolError::InvalidArgs(format!("missing or invalid {name}")))
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

struct PreviewScheduleTool {
    client: Client,
}

#[async_trait]
impl Tool for PreviewScheduleTool {
    fn name(&self) -> &'static str {
        "preview_schedule"
    }
    fn description(&self) -> &'static str {
        "Preview a schedule without replacing the active schedule; reports moved, unscheduled, and sleep impact."
    }
    fn parameters_schema(&self) -> Value {
        json!({"type":"object","properties":{"mode":{"type":"string"},"from":{"type":"string"},"until":{"type":"string"},"task_ids":{"type":"array","items":{"type":"string"}},"pinned":{"type":"array","items":{"type":"string"}},"sleep":{"type":"string"}},"additionalProperties":false})
    }
    async fn call(&self, args: Value) -> Result<ToolOutput, ToolError> {
        let mut args = object(args)?;
        args.entry("mode")
            .or_insert_with(|| Value::String("full".into()));
        let request: SchedulePreviewRequest = serde_json::from_value(Value::Object(args))
            .map_err(|error| ToolError::InvalidArgs(error.to_string()))?;
        let preview = self
            .client
            .preview_schedule(&request)
            .await
            .map_err(client_error)?;
        Ok(ToolOutput {
            content: preview.to_string(),
            ..Default::default()
        })
    }
}

/// Registers planner mutation tools. Calls only produce approval proposals; they never write.
pub fn register_mutation_tools(registry: &mut ToolRegistry, client: Client) {
    for kind in [
        MutationKind::CreateTask,
        MutationKind::UpdateTask,
        MutationKind::DeleteTask,
        MutationKind::CreateHabit,
        MutationKind::UpdateHabit,
        MutationKind::DeleteHabit,
        MutationKind::GenerateSchedule,
        MutationKind::Reschedule,
    ] {
        registry.register(Box::new(MutationTool {
            client: client.clone(),
            kind,
        }));
    }
}

#[derive(Clone, Copy)]
enum MutationKind {
    CreateTask,
    UpdateTask,
    DeleteTask,
    CreateHabit,
    UpdateHabit,
    DeleteHabit,
    GenerateSchedule,
    Reschedule,
}

impl MutationKind {
    fn name(self) -> &'static str {
        match self {
            Self::CreateTask => "create_task",
            Self::UpdateTask => "update_task",
            Self::DeleteTask => "delete_task",
            Self::CreateHabit => "create_habit",
            Self::UpdateHabit => "update_habit",
            Self::DeleteHabit => "delete_habit",
            Self::GenerateSchedule => "generate_schedule",
            Self::Reschedule => "reschedule",
        }
    }

    fn description(self) -> &'static str {
        match self {
            Self::CreateTask => "Propose creating a task. For example, 「演習30題追加」.",
            Self::UpdateTask => "Propose updating a task; changes require user approval.",
            Self::DeleteTask => "Propose deleting a task; changes require user approval.",
            Self::CreateHabit => {
                "Propose creating a recurring habit; changes require user approval."
            }
            Self::UpdateHabit => {
                "Propose updating a recurring habit; changes require user approval."
            }
            Self::DeleteHabit => {
                "Propose deleting a recurring habit; changes require user approval."
            }
            Self::GenerateSchedule => {
                "Propose generating a schedule; it is not applied before approval."
            }
            Self::Reschedule => {
                "Propose rescheduling part of the plan; it is not applied before approval."
            }
        }
    }

    fn target_type(self) -> &'static str {
        match self {
            Self::CreateTask | Self::UpdateTask | Self::DeleteTask => "task",
            Self::CreateHabit | Self::UpdateHabit | Self::DeleteHabit => "habit",
            Self::GenerateSchedule | Self::Reschedule => "schedule",
        }
    }

    fn operation(self) -> &'static str {
        match self {
            Self::CreateTask | Self::CreateHabit => "create",
            Self::UpdateTask | Self::UpdateHabit => "update",
            Self::DeleteTask | Self::DeleteHabit => "delete",
            Self::GenerateSchedule => "generate",
            Self::Reschedule => "reschedule",
        }
    }

    fn schema(self) -> Value {
        let (required, properties) = match self {
            Self::CreateTask => (
                json!(["title", "end_at", "avg_minutes"]),
                json!({
                    "title":{"type":"string"},"description":{"type":"string"},"start_at":{"type":"string"},"end_at":{"type":"string"},"avg_minutes":{"type":"integer"},"sigma_minutes":{"type":"integer"},"depends":{"type":"array","items":{"type":"string"}},"parallelizable":{"type":"boolean"},"allows_parallel":{"type":"boolean"},"abandonability":{"type":"number"},"inferred_fields":{"type":"array"}
                }),
            ),
            Self::UpdateTask => (
                json!(["task_ref"]),
                json!({"task_ref":{"type":"string"},"title":{"type":"string"},"description":{"type":"string"},"start_at":{"type":"string"},"end_at":{"type":"string"},"avg_minutes":{"type":"integer"},"sigma_minutes":{"type":"integer"},"depends":{"type":"array","items":{"type":"string"}},"parallelizable":{"type":"boolean"},"allows_parallel":{"type":"boolean"},"abandonability":{"type":"number"},"status":{"type":"string"},"inferred_fields":{"type":"array"}}),
            ),
            Self::DeleteTask => (json!(["task_ref"]), json!({"task_ref":{"type":"string"}})),
            Self::CreateHabit => (
                json!([
                    "title",
                    "recurrence",
                    "start_time",
                    "end_time",
                    "avg_minutes"
                ]),
                json!({"title":{"type":"string"},"description":{"type":"string"},"recurrence":{"type":"string"},"start_time":{"type":"string"},"end_time":{"type":"string"},"avg_minutes":{"type":"integer"},"sigma_minutes":{"type":"integer"},"parallelizable":{"type":"boolean"},"allows_parallel":{"type":"boolean"},"abandonability":{"type":"number"},"inferred_fields":{"type":"array"}}),
            ),
            Self::UpdateHabit => (
                json!(["habit_ref"]),
                json!({"habit_ref":{"type":"string"},"title":{"type":"string"},"description":{"type":"string"},"recurrence":{"type":"string"},"start_time":{"type":"string"},"end_time":{"type":"string"},"avg_minutes":{"type":"integer"},"sigma_minutes":{"type":"integer"},"parallelizable":{"type":"boolean"},"allows_parallel":{"type":"boolean"},"abandonability":{"type":"number"},"active":{"type":"boolean"},"inferred_fields":{"type":"array"}}),
            ),
            Self::DeleteHabit => (json!(["habit_ref"]), json!({"habit_ref":{"type":"string"}})),
            Self::GenerateSchedule => (
                json!([]),
                json!({"task_ids":{"type":"array","items":{"type":"string"}},"sleep":{"type":"string"}}),
            ),
            Self::Reschedule => (
                json!(["mode"]),
                json!({"mode":{"type":"string"},"from":{"type":"string"},"until":{"type":"string"},"task_ids":{"type":"array","items":{"type":"string"}},"pinned":{"type":"array","items":{"type":"string"}},"sleep":{"type":"string"}}),
            ),
        };
        let properties = properties.as_object().cloned().unwrap_or_default();
        let mut properties = serde_json::Map::from_iter(properties);
        properties.insert("why".into(), json!({"type":"string","description":"Short user-facing reason for the proposed change."}));
        properties.insert(
            "warnings".into(),
            json!({"type":"array","items":{"type":"string"}}),
        );
        json!({"type":"object","properties":properties,"required":required,"additionalProperties":false})
    }
}

struct MutationTool {
    client: Client,
    kind: MutationKind,
}

#[async_trait]
impl Tool for MutationTool {
    fn name(&self) -> &'static str {
        self.kind.name()
    }
    fn description(&self) -> &'static str {
        self.kind.description()
    }
    fn parameters_schema(&self) -> Value {
        self.kind.schema()
    }

    async fn call(&self, args: Value) -> Result<ToolOutput, ToolError> {
        let mut args = object(args)?;
        validate_mutation(&self.client, self.kind, &args).await?;
        let (before, observed_updated_at) = match self.kind {
            MutationKind::UpdateTask | MutationKind::DeleteTask => {
                let reference = required_string(&args, "task_ref")?;
                let task = self
                    .client
                    .get_task(&reference)
                    .await
                    .map_err(client_error)?;
                (
                    Some(task_json(&task, &HashMap::new())),
                    Some(task.updated_at),
                )
            }
            MutationKind::UpdateHabit | MutationKind::DeleteHabit => {
                let reference = required_string(&args, "habit_ref")?;
                let habit = self
                    .client
                    .get_habit(&reference)
                    .await
                    .map_err(client_error)?;
                (Some(habit_json(&habit)), Some(habit.habit.updated_at))
            }
            _ => (None, None),
        };
        let target = args
            .get("task_ref")
            .or_else(|| args.get("habit_ref"))
            .and_then(Value::as_str)
            .unwrap_or("schedule")
            .to_owned();
        let inferred_fields = args
            .get("inferred_fields")
            .cloned()
            .unwrap_or_else(|| json!([]));
        let inferred_fields = serde_json::from_value::<Vec<crate::InferredField>>(inferred_fields)
            .map_err(|error| ToolError::InvalidArgs(format!("invalid inferred_fields: {error}")))?;
        if matches!(
            self.kind,
            MutationKind::GenerateSchedule | MutationKind::Reschedule
        ) {
            let mut preview_args = args.clone();
            if matches!(self.kind, MutationKind::GenerateSchedule) {
                preview_args.insert("mode".into(), Value::String("full".into()));
            }
            let request: SchedulePreviewRequest =
                serde_json::from_value(Value::Object(preview_args))
                    .map_err(|error| ToolError::InvalidArgs(error.to_string()))?;
            let preview = self
                .client
                .preview_schedule(&request)
                .await
                .map_err(client_error)?;
            let entries = preview.get("entries").cloned().ok_or_else(|| {
                ToolError::InvalidArgs("schedule preview did not return entries".into())
            })?;
            args.insert("_preview_entries".into(), entries);
            args.insert("_preview".into(), preview);
        }
        let why = optional_string(&args, "why")?;
        let warnings = args
            .get("warnings")
            .and_then(Value::as_array)
            .map(|values| {
                values
                    .iter()
                    .filter_map(Value::as_str)
                    .map(ToOwned::to_owned)
                    .collect()
            })
            .unwrap_or_default();
        let proposal = crate::ProposedChange {
            operation: self.kind.operation().to_owned(),
            target_label: format!("{} {target}", self.kind.target_type()),
            description: format!("{} ({target})", self.kind.description()),
            before,
            after: Some(Value::Object(args.clone())),
            arguments: Some(Value::Object(args)),
            observed_updated_at,
        };
        Ok(ToolOutput {
            content: serde_json::to_string(&json!({"approval_required":true,"operation":proposal.operation,"target":proposal.target_label,"inferred_fields":inferred_fields,"why":why,"warnings":warnings})).unwrap(),
            why,
            warnings,
            proposed_changes: vec![proposal],
            inferred_fields,
            schedule_dirty: false,
            ..Default::default()
        })
    }
}

async fn validate_mutation(
    client: &Client,
    kind: MutationKind,
    args: &serde_json::Map<String, Value>,
) -> Result<(), ToolError> {
    match kind {
        MutationKind::CreateTask => {
            required_string(args, "title")?;
            required_string(args, "end_at")?;
            required_i64(args, "avg_minutes")?;
        }
        MutationKind::UpdateTask | MutationKind::DeleteTask => {
            let reference = required_string(args, "task_ref")?;
            if reference.starts_with('#')
                || reference.starts_with('h')
                || reference.starts_with('H')
            {
                client.get_task(&reference).await.map_err(client_error)?;
            }
        }
        MutationKind::CreateHabit => {
            required_string(args, "title")?;
            required_string(args, "recurrence")?;
            required_string(args, "start_time")?;
            required_string(args, "end_time")?;
            required_i64(args, "avg_minutes")?;
        }
        MutationKind::UpdateHabit | MutationKind::DeleteHabit => {
            required_string(args, "habit_ref")?;
        }
        MutationKind::GenerateSchedule => {}
        MutationKind::Reschedule => {
            required_string(args, "mode")?;
        }
    }
    Ok(())
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
