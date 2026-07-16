use async_trait::async_trait;
use serde_json::{Value, json};
use std::collections::HashMap;
use takusu_client::{
    Client, HabitDetail, HabitRow, HabitStepRow, SchedulePreviewRequest, TaskQuery, TaskRow,
};
use takusu_util::parse_datetime_tz;

use crate::{Tool, ToolError, ToolOutput, ToolRegistry};

/// Registers planner read tools and approval-only mutation proposals.
pub fn register_tools(registry: &mut ToolRegistry, client: Client) {
    register_read_tools(registry, client.clone());
    register_mutation_tools(registry, client.clone());
    registry.register(Box::new(PreviewScheduleTool {
        client: client.clone(),
    }));
    crate::tools::skills::register_tools(registry, client);
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

fn normalize_datetime(
    value: Option<String>,
    tz: &jiff::tz::TimeZone,
    name: &str,
) -> Result<Option<String>, ToolError> {
    let Some(value) = value else { return Ok(None) };
    parse_datetime_tz(&value, tz)
        .map(Some)
        .map_err(|error| ToolError::InvalidArgs(format!("invalid {name}: {error}")))
}

/// Strip a leading `#` from a user-supplied task reference.
/// Keeps habit-scoped references such as `h1#5` and raw UUIDs intact.
fn strip_leading_hash(reference: &str) -> &str {
    reference.strip_prefix('#').unwrap_or(reference)
}

/// Normalize a task status string to the canonical backend value.
/// Handles common LLM/user synonyms such as "done" -> "completed".
fn normalize_status(status: &str) -> String {
    let lower = status.trim().to_lowercase();
    match lower.as_str() {
        "done" | "complete" | "completed" => "completed".to_string(),
        "todo" | "to-do" | "to_do" | "pending" => "pending".to_string(),
        "in-progress" | "in_progress" | "inprogress" | "doing" | "in progress" => {
            "in_progress".to_string()
        }
        "skip" | "skipped" => "skipped".to_string(),
        "planned" | "scheduled" => "scheduled".to_string(),
        _ => lower,
    }
}

/// Normalize an array of task references by stripping leading `#` characters.
fn normalize_reference_array(
    args: &mut serde_json::Map<String, Value>,
    key: &str,
) -> Result<(), ToolError> {
    if let Some(Value::Array(values)) = args.get_mut(key) {
        for value in values.iter_mut() {
            match value.as_str() {
                Some(reference) => {
                    *value = Value::String(strip_leading_hash(reference.trim()).to_string());
                }
                None => {
                    return Err(ToolError::InvalidArgs(format!(
                        "{key} must contain only strings"
                    )));
                }
            }
        }
    }
    Ok(())
}

#[derive(Debug, Clone)]
struct TaskRef {
    display_id: i64,
    reference: String,
    title: String,
}

#[derive(Debug, Clone)]
struct TaskContext {
    task_refs: HashMap<String, TaskRef>,
    habit_display_ids: HashMap<String, i64>,
}

impl TaskContext {
    fn new(tasks: &[TaskRow], habits: &[HabitRow]) -> Self {
        let habit_display_ids: HashMap<String, i64> = habits
            .iter()
            .map(|habit| (habit.id.clone(), habit.display_id))
            .collect();
        let task_refs: HashMap<String, TaskRef> = tasks
            .iter()
            .map(|task| {
                let reference = task_reference(task, &habit_display_ids);
                (
                    task.id.clone(),
                    TaskRef {
                        display_id: task.display_id,
                        reference,
                        title: task.title.clone(),
                    },
                )
            })
            .collect();
        Self {
            task_refs,
            habit_display_ids,
        }
    }

    fn ref_by_id(&self, id: &str) -> Option<&TaskRef> {
        self.task_refs.get(id)
    }

    fn reference(&self, task: &TaskRow) -> String {
        self.task_refs
            .get(&task.id)
            .map(|task_ref| task_ref.reference.clone())
            .unwrap_or_else(|| task_reference(task, &self.habit_display_ids))
    }

    fn depends(&self, task: &TaskRow) -> Vec<String> {
        serde_json::from_str::<Vec<String>>(&task.depends)
            .unwrap_or_default()
            .into_iter()
            .filter_map(|id| self.task_refs.get(&id).map(|r| r.reference.clone()))
            .collect()
    }
}

fn task_reference(task: &TaskRow, habit_display_ids: &HashMap<String, i64>) -> String {
    task.habit_id
        .as_ref()
        .and_then(|habit_id| habit_display_ids.get(habit_id))
        .map(|habit_display_id| format!("h{habit_display_id}#{}", task.display_id))
        .unwrap_or_else(|| format!("#{}", task.display_id))
}

fn task_json(task: &TaskRow, ctx: &TaskContext) -> Value {
    json!({
        "display_id": task.display_id,
        "reference": ctx.reference(task),
        "title": task.title,
        "description": task.description,
        "start_at": task.start_at,
        "end_at": task.end_at,
        "avg_minutes": task.avg_minutes,
        "sigma_minutes": task.sigma_minutes,
        "depends": ctx.depends(task),
        "parallelizable": task.parallelizable,
        "allows_parallel": task.allows_parallel,
        "abandonability": task.abandonability,
        "status": task.status,
        "fixed": task.fixed,
        "created_at": task.created_at,
        "updated_at": task.updated_at,
    })
}

fn habit_summary_json(habit: &HabitRow) -> Value {
    json!({
        "display_id": habit.display_id,
        "reference": format!("h{}", habit.display_id),
        "title": habit.title,
        "description": habit.description,
        "recurrence": habit.recurrence,
        "start_time": habit.start_time,
        "end_time": habit.end_time,
        "avg_minutes": habit.avg_minutes,
        "sigma_minutes": habit.sigma_minutes,
        "parallelizable": habit.parallelizable,
        "allows_parallel": habit.allows_parallel,
        "abandonability": habit.abandonability,
        "active": habit.active,
        "fixed": habit.fixed,
        "window_mode": habit.window_mode,
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
        "steps": habit.steps.iter().map(step_json).collect::<Vec<_>>(),
    })
}

fn step_json(step: &HabitStepRow) -> Value {
    json!({
        "position": step.position,
        "title": step.title,
        "description": step.description,
        "start_time": step.start_time,
        "end_time": step.end_time,
        "avg_minutes": step.avg_minutes,
        "sigma_minutes": step.sigma_minutes,
        "parallelizable": step.parallelizable,
        "allows_parallel": step.allows_parallel,
        "abandonability": step.abandonability,
        "fixed": step.fixed,
    })
}

fn schedule_entry_value(entry: &Value, ctx: &TaskContext) -> Value {
    let task_id = entry.get("task_id").and_then(Value::as_str).unwrap_or("");
    let (reference, display_id, title) = match ctx.ref_by_id(task_id) {
        Some(r) => (
            Value::String(r.reference.clone()),
            json!(r.display_id),
            Value::String(r.title.clone()),
        ),
        None => (
            Value::String("unknown".into()),
            Value::Null,
            Value::String("unknown task".into()),
        ),
    };
    json!({
        "reference": reference,
        "display_id": display_id,
        "title": title,
        "start_at": entry.get("start_at").cloned().unwrap_or(Value::Null),
        "end_at": entry.get("end_at").cloned().unwrap_or(Value::Null),
    })
}

fn reference_value(id: &str, ctx: &TaskContext) -> Value {
    ctx.ref_by_id(id)
        .map(|r| Value::String(r.reference.clone()))
        .unwrap_or_else(|| Value::String("unknown".into()))
}

fn transform_preview(preview: Value, ctx: &TaskContext) -> Value {
    let mut out = preview.as_object().cloned().unwrap_or_default();

    if let Some(Value::Array(entries)) = out.get("entries").cloned() {
        let transformed = entries
            .iter()
            .map(|entry| schedule_entry_value(entry, ctx))
            .collect::<Vec<_>>();
        out.insert("entries".into(), Value::Array(transformed));
    }

    for key in ["unscheduled_task_ids", "displaced_task_ids"] {
        if let Some(Value::Array(ids)) = out.get(key).cloned() {
            let transformed = ids
                .iter()
                .map(|id| {
                    id.as_str()
                        .map(|s| reference_value(s, ctx))
                        .unwrap_or_else(|| Value::String("unknown".into()))
                })
                .collect::<Vec<_>>();
            out.insert(key.into(), Value::Array(transformed));
        }
    }

    Value::Object(out)
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
        json!({
            "type": "object",
            "properties": {
                "status": {
                    "type": "string",
                    "enum": ["pending", "scheduled", "in_progress", "completed", "skipped"],
                    "description": "Task status filter. Use 'completed' for done tasks."
                },
                "from": {"type": "string", "description": "Start of range; interpreted in server timezone."},
                "until": {"type": "string", "description": "End of range; interpreted in server timezone."},
                "habit_id": {"type": "string", "description": "Habit reference such as h1."},
            },
            "additionalProperties": false,
        })
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
        let tz = server_timezone(&self.client).await?;
        let query = TaskQuery {
            status: optional_string(&args, "status")?.map(|s| normalize_status(&s)),
            from: normalize_datetime(optional_string(&args, "from")?, &tz, "from")?,
            until: normalize_datetime(optional_string(&args, "until")?, &tz, "until")?,
            habit_id: habit.as_ref().map(|habit| habit.habit.id.clone()),
        };

        let default_query = TaskQuery::default();
        let c1 = self.client.clone();
        let c2 = self.client.clone();
        let c3 = self.client.clone();
        let (tasks, all_tasks, habits) = tokio::try_join!(
            async { c1.list_tasks(&query).await },
            async { c2.list_tasks(&default_query).await },
            async { c3.list_habits().await },
        )
        .map_err(client_error)?;

        let ctx = TaskContext::new(&all_tasks, &habits);
        let content = tasks
            .iter()
            .map(|task| task_json(task, &ctx))
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
        json!({
            "type": "object",
            "properties": {
                "task_ref": {"type": "string", "description": "#42 or h1#5"},
            },
            "required": ["task_ref"],
            "additionalProperties": false,
        })
    }
    async fn call(&self, args: Value) -> Result<ToolOutput, ToolError> {
        let task_ref = required_string(&object(args)?, "task_ref")?;
        let task_ref = strip_leading_hash(&task_ref).to_string();

        let default_query = TaskQuery::default();
        let c1 = self.client.clone();
        let c2 = self.client.clone();
        let c3 = self.client.clone();
        let (task, all_tasks, habits) = tokio::try_join!(
            async { c1.get_task(&task_ref).await },
            async { c2.list_tasks(&default_query).await },
            async { c3.list_habits().await },
        )
        .map_err(client_error)?;

        let ctx = TaskContext::new(&all_tasks, &habits);
        let result = task_json(&task, &ctx);
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
        json!({
            "type": "object",
            "properties": {},
            "additionalProperties": false,
        })
    }
    async fn call(&self, args: Value) -> Result<ToolOutput, ToolError> {
        let _ = object(args)?;
        let habits = self.client.list_habits().await.map_err(client_error)?;
        let content = habits.iter().map(habit_summary_json).collect::<Vec<_>>();
        Ok(ToolOutput {
            content: serde_json::to_string(&content).unwrap(),
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
        json!({
            "type": "object",
            "properties": {
                "habit_ref": {"type": "string", "description": "Habit reference such as h1"},
            },
            "required": ["habit_ref"],
            "additionalProperties": false,
        })
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
        json!({
            "type": "object",
            "properties": {},
            "additionalProperties": false,
        })
    }
    async fn call(&self, args: Value) -> Result<ToolOutput, ToolError> {
        let _ = object(args)?;

        let default_query = TaskQuery::default();
        let c1 = self.client.clone();
        let c2 = self.client.clone();
        let c3 = self.client.clone();
        let (schedule, tasks, habits) = tokio::try_join!(
            async { c1.get_schedule().await },
            async { c2.list_tasks(&default_query).await },
            async { c3.list_habits().await },
        )
        .map_err(client_error)?;

        let ctx = TaskContext::new(&tasks, &habits);
        let entries: Value = serde_json::from_str(&schedule.schedule)
            .map_err(|error| ToolError::Other(Box::new(error)))?;
        let entries = match entries {
            Value::Array(entries) => entries,
            _ => Vec::new(),
        };
        let entries = entries
            .iter()
            .map(|entry| schedule_entry_value(entry, &ctx))
            .collect::<Vec<_>>();

        Ok(ToolOutput {
            content: serde_json::to_string(&json!({
                "id": schedule.id,
                "created_at": schedule.created_at,
                "updated_at": schedule.updated_at,
                "entries": entries,
            }))
            .unwrap(),
            ..Default::default()
        })
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
        json!({
            "type": "object",
            "properties": {},
            "additionalProperties": false,
        })
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
        json!({
            "type": "object",
            "properties": {
                "mode": {"type": "string"},
                "from": {"type": "string", "description": "Start of range; interpreted in server timezone."},
                "until": {"type": "string", "description": "End of range; interpreted in server timezone."},
                "task_ids": {"type": "array", "items": {"type": "string"}},
                "pinned": {"type": "array", "items": {"type": "string"}},
                "sleep": {"type": "string"},
            },
            "additionalProperties": false,
        })
    }
    async fn call(&self, args: Value) -> Result<ToolOutput, ToolError> {
        let mut args = object(args)?;
        args.entry("mode")
            .or_insert_with(|| Value::String("full".into()));

        let tz = server_timezone(&self.client).await?;
        normalize_mutation_field(&mut args, "from", &tz)?;
        normalize_mutation_field(&mut args, "until", &tz)?;
        normalize_reference_array(&mut args, "task_ids")?;
        normalize_reference_array(&mut args, "pinned")?;

        let request: SchedulePreviewRequest = serde_json::from_value(Value::Object(args))
            .map_err(|error| ToolError::InvalidArgs(error.to_string()))?;

        let default_query = TaskQuery::default();
        let c1 = self.client.clone();
        let c2 = self.client.clone();
        let c3 = self.client.clone();
        let req = request;
        let (preview, tasks, habits) = tokio::try_join!(
            async { c1.preview_schedule(&req).await },
            async { c2.list_tasks(&default_query).await },
            async { c3.list_habits().await },
        )
        .map_err(client_error)?;

        let ctx = TaskContext::new(&tasks, &habits);
        Ok(ToolOutput {
            content: serde_json::to_string(&transform_preview(preview, &ctx)).unwrap(),
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
                    "title": {"type": "string"},
                    "description": {"type": "string"},
                    "start_at": {"type": "string", "description": "Start time; interpreted in server timezone if no offset is given."},
                    "end_at": {"type": "string", "description": "Deadline; interpreted in server timezone if no offset is given."},
                    "avg_minutes": {"type": "integer"},
                    "sigma_minutes": {"type": "integer"},
                    "depends": {"type": "array", "items": {"type": "string"}},
                    "parallelizable": {"type": "boolean"},
                    "allows_parallel": {"type": "boolean"},
                    "abandonability": {"type": "number"},
                    "inferred_fields": {"type": "array"},
                }),
            ),
            Self::UpdateTask => (
                json!(["task_ref"]),
                json!({
                    "task_ref": {"type": "string"},
                    "title": {"type": "string"},
                    "description": {"type": "string"},
                    "start_at": {"type": "string", "description": "Start time; interpreted in server timezone if no offset is given."},
                    "end_at": {"type": "string", "description": "Deadline; interpreted in server timezone if no offset is given."},
                    "avg_minutes": {"type": "integer"},
                    "sigma_minutes": {"type": "integer"},
                    "depends": {"type": "array", "items": {"type": "string"}},
                    "parallelizable": {"type": "boolean"},
                    "allows_parallel": {"type": "boolean"},
                    "abandonability": {"type": "number"},
                    "status": {
                        "type": "string",
                        "enum": ["pending", "scheduled", "in_progress", "completed", "skipped"],
                        "description": "New task status. 'completed' means done."
                    },
                    "inferred_fields": {"type": "array"},
                }),
            ),
            Self::DeleteTask => (
                json!(["task_ref"]),
                json!({
                    "task_ref": {"type": "string"},
                }),
            ),
            Self::CreateHabit => (
                json!([
                    "title",
                    "recurrence",
                    "start_time",
                    "end_time",
                    "avg_minutes"
                ]),
                json!({
                    "title": {"type": "string"},
                    "description": {"type": "string"},
                    "recurrence": {"type": "string"},
                    "start_time": {"type": "string", "description": "Time of day (HH:MM)."},
                    "end_time": {"type": "string", "description": "Time of day (HH:MM)."},
                    "avg_minutes": {"type": "integer"},
                    "sigma_minutes": {"type": "integer"},
                    "parallelizable": {"type": "boolean"},
                    "allows_parallel": {"type": "boolean"},
                    "abandonability": {"type": "number"},
                    "inferred_fields": {"type": "array"},
                }),
            ),
            Self::UpdateHabit => (
                json!(["habit_ref"]),
                json!({
                    "habit_ref": {"type": "string"},
                    "title": {"type": "string"},
                    "description": {"type": "string"},
                    "recurrence": {"type": "string"},
                    "start_time": {"type": "string", "description": "Time of day (HH:MM)."},
                    "end_time": {"type": "string", "description": "Time of day (HH:MM)."},
                    "avg_minutes": {"type": "integer"},
                    "sigma_minutes": {"type": "integer"},
                    "parallelizable": {"type": "boolean"},
                    "allows_parallel": {"type": "boolean"},
                    "abandonability": {"type": "number"},
                    "active": {"type": "boolean"},
                    "inferred_fields": {"type": "array"},
                }),
            ),
            Self::DeleteHabit => (
                json!(["habit_ref"]),
                json!({
                    "habit_ref": {"type": "string"},
                }),
            ),
            Self::GenerateSchedule => (
                json!([]),
                json!({
                    "task_ids": {"type": "array", "items": {"type": "string"}},
                    "sleep": {"type": "string"},
                }),
            ),
            Self::Reschedule => (
                json!(["mode"]),
                json!({
                    "mode": {"type": "string"},
                    "from": {"type": "string", "description": "Start of range; interpreted in server timezone if no offset is given."},
                    "until": {"type": "string", "description": "End of range; interpreted in server timezone if no offset is given."},
                    "task_ids": {"type": "array", "items": {"type": "string"}},
                    "pinned": {"type": "array", "items": {"type": "string"}},
                    "sleep": {"type": "string"},
                }),
            ),
        };
        let properties = properties.as_object().cloned().unwrap_or_default();
        let mut properties = serde_json::Map::from_iter(properties);
        properties.insert(
            "why".into(),
            json!({"type": "string", "description": "Short user-facing reason for the proposed change."}),
        );
        properties.insert(
            "warnings".into(),
            json!({"type": "array", "items": {"type": "string"}}),
        );
        json!({
            "type": "object",
            "properties": properties,
            "required": required,
            "additionalProperties": false,
        })
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
        validate_mutation(self.kind, &args)?;

        let tz = server_timezone(&self.client).await?;
        normalize_mutation_args(self.kind, &mut args, &tz)?;

        let mut execution_args = args.clone();
        normalize_execution_references(self.kind, &mut execution_args)?;

        let (before, observed_updated_at) = match self.kind {
            MutationKind::UpdateTask | MutationKind::DeleteTask => {
                let lookup = required_string(&execution_args, "task_ref")?;

                let default_query = TaskQuery::default();
                let c1 = self.client.clone();
                let c2 = self.client.clone();
                let c3 = self.client.clone();
                let (task, all_tasks, habits) = tokio::try_join!(
                    async { c1.get_task(&lookup).await },
                    async { c2.list_tasks(&default_query).await },
                    async { c3.list_habits().await },
                )
                .map_err(client_error)?;

                let ctx = TaskContext::new(&all_tasks, &habits);
                (Some(task_json(&task, &ctx)), Some(task.updated_at))
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

        if matches!(
            self.kind,
            MutationKind::GenerateSchedule | MutationKind::Reschedule
        ) {
            let mut preview_args = execution_args.clone();
            if matches!(self.kind, MutationKind::GenerateSchedule) {
                preview_args.insert("mode".into(), Value::String("full".into()));
            }
            let request: SchedulePreviewRequest =
                serde_json::from_value(Value::Object(preview_args))
                    .map_err(|error| ToolError::InvalidArgs(error.to_string()))?;

            let default_query = TaskQuery::default();
            let c1 = self.client.clone();
            let c2 = self.client.clone();
            let c3 = self.client.clone();
            let req = request;
            let (preview, all_tasks, habits) = tokio::try_join!(
                async { c1.preview_schedule(&req).await },
                async { c2.list_tasks(&default_query).await },
                async { c3.list_habits().await },
            )
            .map_err(client_error)?;

            let ctx = TaskContext::new(&all_tasks, &habits);
            let entries = preview.get("entries").cloned().ok_or_else(|| {
                ToolError::InvalidArgs("schedule preview did not return entries".into())
            })?;
            execution_args.insert("_preview_entries".into(), entries);
            args.insert("_preview".into(), transform_preview(preview, &ctx));
        }

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
            after: Some(Value::Object(args)),
            arguments: Some(Value::Object(execution_args)),
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

fn normalize_mutation_field(
    args: &mut serde_json::Map<String, Value>,
    name: &str,
    tz: &jiff::tz::TimeZone,
) -> Result<(), ToolError> {
    if let Some(value) = optional_string(args, name)? {
        let normalized = parse_datetime_tz(&value, tz)
            .map_err(|error| ToolError::InvalidArgs(format!("invalid {name}: {error}")))?;
        args.insert(name.into(), Value::String(normalized));
    }
    Ok(())
}

fn normalize_mutation_args(
    kind: MutationKind,
    args: &mut serde_json::Map<String, Value>,
    tz: &jiff::tz::TimeZone,
) -> Result<(), ToolError> {
    match kind {
        MutationKind::CreateTask | MutationKind::UpdateTask => {
            normalize_mutation_field(args, "start_at", tz)?;
            normalize_mutation_field(args, "end_at", tz)?;
            if let Some(status) = args.get("status").and_then(Value::as_str) {
                args.insert("status".into(), Value::String(normalize_status(status)));
            }
        }
        MutationKind::Reschedule => {
            normalize_mutation_field(args, "from", tz)?;
            normalize_mutation_field(args, "until", tz)?;
        }
        _ => {}
    }
    Ok(())
}

/// Strip a leading `#` from a single string reference field for backend execution.
fn normalize_task_ref(
    args: &mut serde_json::Map<String, Value>,
    key: &str,
) -> Result<(), ToolError> {
    if let Some(value) = optional_string(args, key)? {
        args.insert(
            key.to_string(),
            Value::String(strip_leading_hash(&value).to_string()),
        );
    }
    Ok(())
}

/// Strip leading `#` characters from reference fields used for backend execution.
/// Display-facing `args` keep the original user input so approval diffs stay clean.
fn normalize_execution_references(
    kind: MutationKind,
    args: &mut serde_json::Map<String, Value>,
) -> Result<(), ToolError> {
    match kind {
        MutationKind::CreateTask => {
            normalize_reference_array(args, "depends")?;
        }
        MutationKind::UpdateTask => {
            normalize_task_ref(args, "task_ref")?;
            normalize_reference_array(args, "depends")?;
        }
        MutationKind::DeleteTask => {
            normalize_task_ref(args, "task_ref")?;
        }
        MutationKind::GenerateSchedule => {
            normalize_reference_array(args, "task_ids")?;
        }
        MutationKind::Reschedule => {
            normalize_reference_array(args, "task_ids")?;
            normalize_reference_array(args, "pinned")?;
        }
        _ => {}
    }
    Ok(())
}

fn validate_mutation(
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
            required_string(args, "task_ref")?;
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

    fn task_row(
        id: &str,
        display_id: i64,
        title: &str,
        habit_id: Option<&str>,
        depends: &[&str],
    ) -> TaskRow {
        TaskRow {
            id: id.to_string(),
            display_id,
            title: title.to_string(),
            description: None,
            start_at: None,
            end_at: "2025-06-05T10:00:00Z".to_string(),
            avg_minutes: 30,
            sigma_minutes: 5,
            depends: serde_json::to_string(
                &depends.iter().map(|s| s.to_string()).collect::<Vec<_>>(),
            )
            .unwrap(),
            parallelizable: false,
            allows_parallel: false,
            abandonability: 0.5,
            status: "pending".to_string(),
            habit_id: habit_id.map(|s| s.to_string()),
            ical_uid: None,
            user_edited: false,
            fixed: false,
            habit_step_id: None,
            created_at: "2025-06-01T00:00:00Z".to_string(),
            updated_at: "2025-06-01T00:00:00Z".to_string(),
        }
    }

    fn habit_row(id: &str, display_id: i64, title: &str) -> HabitRow {
        HabitRow {
            id: id.to_string(),
            display_id,
            title: title.to_string(),
            description: None,
            recurrence: "FREQ=DAILY".to_string(),
            start_time: "08:00".to_string(),
            end_time: "09:00".to_string(),
            avg_minutes: 60,
            sigma_minutes: 10,
            parallelizable: false,
            allows_parallel: false,
            abandonability: 0.5,
            active: true,
            fixed: false,
            window_mode: "day".to_string(),
            created_at: "2025-06-01T00:00:00Z".to_string(),
            updated_at: "2025-06-01T00:00:00Z".to_string(),
        }
    }

    fn step_row(id: &str, habit_id: &str, position: i64, title: &str) -> HabitStepRow {
        HabitStepRow {
            id: id.to_string(),
            habit_id: habit_id.to_string(),
            position,
            title: title.to_string(),
            description: None,
            start_time: "08:00".to_string(),
            end_time: "09:00".to_string(),
            avg_minutes: 30,
            sigma_minutes: 5,
            parallelizable: false,
            allows_parallel: false,
            abandonability: 0.5,
            fixed: false,
            depends_on: "[]".to_string(),
            created_at: "2025-06-01T00:00:00Z".to_string(),
        }
    }

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
    fn task_reference_uses_global_or_habit_scoped_display_id() {
        let habit_id = "habit-uuid";
        let mut habit_map = HashMap::new();
        habit_map.insert(habit_id.to_string(), 7);

        let standalone = task_row("task-1", 42, "standalone", None, &[]);
        assert_eq!(task_reference(&standalone, &HashMap::new()), "#42");

        let habit_task = task_row("task-2", 3, "habit task", Some(habit_id), &[]);
        assert_eq!(task_reference(&habit_task, &habit_map), "h7#3");
    }

    #[test]
    fn task_json_hides_internal_uuids_and_uses_references() {
        let habit = habit_row("habit-uuid", 7, "habit");
        let dep = task_row("dep-uuid", 5, "dep", None, &[]);
        let task = task_row("task-uuid", 3, "task", Some("habit-uuid"), &["dep-uuid"]);
        let ctx = TaskContext::new(&[task.clone(), dep.clone()], &[habit]);

        let value = task_json(&task, &ctx);
        assert!(value.get("id").is_none());
        assert!(value.get("habit_id").is_none());
        assert!(value.get("habit_step_id").is_none());
        assert_eq!(value["display_id"], 3);
        assert_eq!(value["reference"], "h7#3");
        assert_eq!(value["depends"], json!(["#5"]));
    }

    #[test]
    fn habit_json_hides_internal_uuids() {
        let habit = habit_row("habit-uuid", 7, "habit");
        let step = step_row("step-uuid", "habit-uuid", 1, "step");
        let detail = HabitDetail {
            habit,
            steps: vec![step],
        };

        let value = habit_json(&detail);
        assert!(value.get("id").is_none());
        assert_eq!(value["display_id"], 7);
        assert_eq!(value["reference"], "h7");

        let steps = value["steps"].as_array().unwrap();
        assert_eq!(steps.len(), 1);
        assert!(steps[0].get("id").is_none());
        assert!(steps[0].get("habit_id").is_none());
    }

    #[test]
    fn schedule_entry_value_includes_title_display_id_and_reference() {
        let task = task_row("task-uuid", 3, "task title", Some("habit-uuid"), &[]);
        let habit = habit_row("habit-uuid", 7, "habit");
        let ctx = TaskContext::new(&[task], &[habit]);

        let entry = json!({
            "task_id": "task-uuid",
            "start_at": "2025-06-05T10:00:00Z",
            "end_at": "2025-06-05T11:00:00Z",
        });
        let value = schedule_entry_value(&entry, &ctx);

        assert!(value.get("task_id").is_none());
        assert_eq!(value["reference"], "h7#3");
        assert_eq!(value["display_id"], 3);
        assert_eq!(value["title"], "task title");
        assert_eq!(value["start_at"], "2025-06-05T10:00:00Z");
        assert_eq!(value["end_at"], "2025-06-05T11:00:00Z");
    }

    #[test]
    fn transform_preview_replaces_internal_task_ids_with_references() {
        let task = task_row("task-uuid", 3, "task", Some("habit-uuid"), &[]);
        let habit = habit_row("habit-uuid", 7, "habit");
        let ctx = TaskContext::new(&[task], &[habit]);

        let preview = json!({
            "entries": [{
                "task_id": "task-uuid",
                "start_at": "2025-06-05T10:00:00Z",
                "end_at": "2025-06-05T11:00:00Z",
            }],
            "unscheduled_task_ids": ["task-uuid"],
            "displaced_task_ids": ["task-uuid"],
        });
        let out = transform_preview(preview, &ctx);

        let entries = out["entries"].as_array().unwrap();
        assert_eq!(entries[0]["reference"], "h7#3");
        assert_eq!(out["unscheduled_task_ids"], json!(["h7#3"]));
        assert_eq!(out["displaced_task_ids"], json!(["h7#3"]));
    }

    #[test]
    fn normalize_mutation_field_interprets_naive_datetime_in_server_timezone() {
        let tz = jiff::tz::TimeZone::get("Asia/Tokyo").unwrap();
        let mut args = serde_json::Map::new();
        args.insert(
            "end_at".to_string(),
            Value::String("2025-06-05T14:00".to_string()),
        );

        normalize_mutation_field(&mut args, "end_at", &tz).unwrap();

        // 2025-06-05 14:00 JST == 2025-06-05 05:00 UTC
        assert!(
            args["end_at"]
                .as_str()
                .unwrap()
                .starts_with("2025-06-05T05:00:00")
        );
        assert!(args["end_at"].as_str().unwrap().ends_with('Z'));
    }

    #[test]
    fn strip_leading_hash_removes_only_leading_hash() {
        assert_eq!(strip_leading_hash("#42"), "42");
        assert_eq!(strip_leading_hash("42"), "42");
        assert_eq!(strip_leading_hash("h1#5"), "h1#5");
        assert_eq!(strip_leading_hash("#h1#5"), "h1#5");
        assert_eq!(strip_leading_hash("uuid-like-string"), "uuid-like-string");
    }

    #[test]
    fn normalize_reference_array_trims_and_strips_leading_hash() {
        let mut args = serde_json::Map::new();
        args.insert(
            "depends".to_string(),
            json!(["#5", " h1#3", "#42 ", "  uuid  "]),
        );
        normalize_reference_array(&mut args, "depends").unwrap();

        assert_eq!(args["depends"], json!(["5", "h1#3", "42", "uuid"]));
    }

    #[test]
    fn normalize_reference_array_rejects_non_string_entries() {
        let mut args = serde_json::Map::new();
        args.insert("task_ids".to_string(), json!(["#5", 42]));

        assert!(normalize_reference_array(&mut args, "task_ids").is_err());
    }

    #[test]
    fn normalize_execution_references_strips_hashes_for_backend() {
        let tz = jiff::tz::TimeZone::get("UTC").unwrap();
        let mut display_args = serde_json::Map::new();
        display_args.insert("task_ref".to_string(), Value::String("#42".to_string()));
        display_args.insert("depends".to_string(), json!(["#1", "h2#3"]));

        normalize_mutation_args(MutationKind::UpdateTask, &mut display_args, &tz).unwrap();

        let mut execution_args = display_args.clone();
        normalize_execution_references(MutationKind::UpdateTask, &mut execution_args).unwrap();

        assert_eq!(display_args["task_ref"], "#42");
        assert_eq!(display_args["depends"], json!(["#1", "h2#3"]));
        assert_eq!(execution_args["task_ref"], "42");
        assert_eq!(execution_args["depends"], json!(["1", "h2#3"]));
    }

    #[test]
    fn normalize_status_maps_common_synonyms() {
        assert_eq!(normalize_status("done"), "completed");
        assert_eq!(normalize_status("Done"), "completed");
        assert_eq!(normalize_status("  DONE  "), "completed");
        assert_eq!(normalize_status("complete"), "completed");
        assert_eq!(normalize_status("in-progress"), "in_progress");
        assert_eq!(normalize_status("in progress"), "in_progress");
        assert_eq!(normalize_status("todo"), "pending");
        assert_eq!(normalize_status("skip"), "skipped");
        assert_eq!(normalize_status("completed"), "completed");
        assert_eq!(normalize_status("pending"), "pending");
    }

    #[test]
    fn normalize_mutation_args_normalizes_status_for_update_task() {
        let tz = jiff::tz::TimeZone::get("UTC").unwrap();
        let mut args = serde_json::Map::new();
        args.insert("status".to_string(), Value::String("done".to_string()));

        normalize_mutation_args(MutationKind::UpdateTask, &mut args, &tz).unwrap();

        assert_eq!(args["status"], "completed");
    }

    #[test]
    fn list_tasks_status_schema_has_enum() {
        let tool = ListTasks {
            client: Client::new("http://localhost", ""),
        };
        let schema = tool.parameters_schema();
        let values: Vec<String> =
            serde_json::from_value(schema["properties"]["status"]["enum"].clone()).unwrap();
        assert!(values.contains(&"completed".to_string()));
        assert!(values.contains(&"pending".to_string()));
    }

    #[test]
    fn update_task_status_schema_has_enum() {
        let tool = MutationTool {
            client: Client::new("http://localhost", ""),
            kind: MutationKind::UpdateTask,
        };
        let schema = tool.parameters_schema();
        let values: Vec<String> =
            serde_json::from_value(schema["properties"]["status"]["enum"].clone()).unwrap();
        assert!(values.contains(&"completed".to_string()));
    }
}
