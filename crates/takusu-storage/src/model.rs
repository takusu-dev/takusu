use serde::{Deserialize, Serialize};

#[allow(dead_code)]
fn default_abandonability() -> f64 {
    0.5
}

#[allow(dead_code)]
fn default_sleep() -> String {
    "recommended".to_string()
}

pub mod bool_compat {
    use serde::{Deserialize, Deserializer};

    pub fn deserialize<'de, D>(deserializer: D) -> Result<bool, D::Error>
    where
        D: Deserializer<'de>,
    {
        let v = serde_json::Value::deserialize(deserializer)?;
        match v {
            serde_json::Value::Bool(b) => Ok(b),
            serde_json::Value::Number(n) => Ok(n.as_f64().map(|f| f != 0.0).unwrap_or(false)),
            serde_json::Value::Null => Ok(false),
            _ => Err(serde::de::Error::custom(
                "expected bool or number for boolean field",
            )),
        }
    }

    pub fn serialize<S>(value: &bool, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serde::Serialize::serialize(value, serializer)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct TaskRow {
    pub id: String,
    #[serde(default)]
    pub display_id: i64,
    pub title: String,
    pub description: Option<String>,
    pub start_at: Option<String>,
    pub end_at: String,
    pub avg_minutes: i64,
    pub sigma_minutes: i64,
    pub depends: String,
    #[serde(with = "bool_compat", default)]
    pub parallelizable: bool,
    #[serde(with = "bool_compat", default)]
    pub allows_parallel: bool,
    pub abandonability: f64,
    pub status: String,
    pub habit_id: Option<String>,
    pub ical_uid: Option<String>,
    #[serde(with = "bool_compat", default)]
    pub user_edited: bool,
    #[serde(with = "bool_compat", default)]
    pub fixed: bool,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateTask {
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_at: Option<String>,
    pub end_at: String,
    pub avg_minutes: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sigma_minutes: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub depends: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parallelizable: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allows_parallel: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub abandonability: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ical_uid: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub habit_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub fixed: Option<bool>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct UpdateTask {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub avg_minutes: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sigma_minutes: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub depends: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parallelizable: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allows_parallel: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub abandonability: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub habit_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub user_edited: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub fixed: Option<bool>,
}

#[derive(Debug, Default, serde::Deserialize)]
pub struct TaskQuery {
    pub status: Option<String>,
    pub from: Option<String>,
    pub until: Option<String>,
    pub habit_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct HabitRow {
    pub id: String,
    pub title: String,
    pub description: Option<String>,
    pub recurrence: String,
    pub start_time: String,
    pub end_time: String,
    pub avg_minutes: i64,
    pub sigma_minutes: i64,
    #[serde(with = "bool_compat", default)]
    pub parallelizable: bool,
    #[serde(with = "bool_compat", default)]
    pub allows_parallel: bool,
    pub abandonability: f64,
    #[serde(with = "bool_compat", default)]
    pub active: bool,
    #[serde(with = "bool_compat", default)]
    pub fixed: bool,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CreateHabit {
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub recurrence: String,
    pub start_time: String,
    pub end_time: String,
    pub avg_minutes: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sigma_minutes: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parallelizable: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allows_parallel: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub abandonability: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub fixed: Option<bool>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct UpdateHabit {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recurrence: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_time: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_time: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub avg_minutes: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sigma_minutes: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parallelizable: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allows_parallel: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub abandonability: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub fixed: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct ScheduleRow {
    pub id: String,
    pub created_at: String,
    pub updated_at: String,
    pub schedule: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScheduleEntry {
    pub task_id: String,
    pub start_at: String,
    pub end_at: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SaveScheduleRequest {
    pub entries: Vec<ScheduleEntry>,
    #[serde(default)]
    pub mark_scheduled_task_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct TokenRow {
    pub id: i64,
    pub token_hash: String,
    pub label: Option<String>,
    pub created_by: String,
    pub created_at: String,
    pub revoked_at: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TokenCreateResponse {
    pub id: i64,
    pub token: String,
    pub label: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct GoogleCalSettingsRow {
    pub id: String,
    #[serde(with = "bool_compat", default)]
    pub enabled: bool,
    pub calendar_id: String,
    pub client_id: String,
    pub client_secret: String,
    pub refresh_token: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct UpdateGoogleCalSettings {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub calendar_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_secret: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub refresh_token: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct GoogleCalEventRow {
    pub task_id: String,
    pub google_event_id: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct SettingsRow {
    pub id: String,
    pub tz: String,
    pub sleep_start: String,
    pub sleep_end: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct UpdateSettings {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tz: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sleep_start: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sleep_end: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bool_compat_deserializes_true_false() {
        #[derive(serde::Deserialize)]
        struct Wrap {
            #[serde(with = "bool_compat", default)]
            #[allow(dead_code)]
            v: bool,
        }
        assert!(serde_json::from_str::<Wrap>(r#"{"v":true}"#).unwrap().v);
        assert!(!serde_json::from_str::<Wrap>(r#"{"v":false}"#).unwrap().v);
    }

    #[test]
    fn bool_compat_deserializes_numbers_as_bool() {
        // Non-zero numbers → true, zero → false. This is the compat path for
        // clients that send 0/1 instead of booleans (e.g. some CLI/worker paths).
        #[derive(serde::Deserialize)]
        struct Wrap {
            #[serde(with = "bool_compat", default)]
            #[allow(dead_code)]
            v: bool,
        }
        assert!(serde_json::from_str::<Wrap>(r#"{"v":1}"#).unwrap().v);
        assert!(!serde_json::from_str::<Wrap>(r#"{"v":0}"#).unwrap().v);
        // Floats: 0.0 → false, anything else → true.
        assert!(!serde_json::from_str::<Wrap>(r#"{"v":0.0}"#).unwrap().v);
        assert!(serde_json::from_str::<Wrap>(r#"{"v":2.5}"#).unwrap().v);
    }

    #[test]
    fn bool_compat_deserializes_null_as_false() {
        #[derive(serde::Deserialize)]
        struct Wrap {
            #[serde(with = "bool_compat", default)]
            #[allow(dead_code)]
            v: bool,
        }
        assert!(!serde_json::from_str::<Wrap>(r#"{"v":null}"#).unwrap().v);
    }

    #[test]
    fn bool_compat_rejects_strings() {
        #[derive(serde::Deserialize)]
        struct Wrap {
            #[serde(with = "bool_compat")]
            #[allow(dead_code)]
            v: bool,
        }
        assert!(serde_json::from_str::<Wrap>(r#"{"v":"true"}"#).is_err());
    }

    #[test]
    fn bool_compat_defaults_to_false_when_missing() {
        #[derive(serde::Deserialize)]
        struct Wrap {
            #[serde(with = "bool_compat", default)]
            #[allow(dead_code)]
            v: bool,
        }
        assert!(!serde_json::from_str::<Wrap>(r#"{}"#).unwrap().v);
    }

    #[test]
    fn task_row_defaults_optional_bools_when_missing() {
        // TaskRow has #[serde(default)] on parallelizable/allows_parallel/user_edited.
        // A minimal JSON missing those fields should still deserialize.
        let json = r#"{
            "id": "t1",
            "display_id": 1,
            "title": "T",
            "description": null,
            "start_at": null,
            "end_at": "2025-01-01T00:00:00Z",
            "avg_minutes": 30,
            "sigma_minutes": 0,
            "depends": "[]",
            "abandonability": 0.5,
            "status": "pending",
            "habit_id": null,
            "ical_uid": null,
            "created_at": "",
            "updated_at": ""
        }"#;
        let row: TaskRow = serde_json::from_str(json).unwrap();
        assert!(!row.parallelizable);
        assert!(!row.allows_parallel);
        assert!(!row.user_edited);
    }

    #[test]
    fn update_task_skip_serializing_none() {
        let u = UpdateTask::default();
        let json = serde_json::to_string(&u).unwrap();
        // All fields None → serialized JSON should be empty object.
        assert_eq!(json, "{}");
    }

    #[test]
    fn create_task_roundtrip() {
        let c = CreateTask {
            title: "Test".into(),
            description: Some("desc".into()),
            start_at: None,
            end_at: "2025-01-01T00:00:00Z".into(),
            avg_minutes: 30,
            sigma_minutes: Some(5),
            depends: Some(vec!["t1".into()]),
            parallelizable: Some(true),
            allows_parallel: Some(false),
            abandonability: Some(0.3),
            ical_uid: None,
            habit_id: None,
            fixed: None,
        };
        let json = serde_json::to_string(&c).unwrap();
        let back: CreateTask = serde_json::from_str(&json).unwrap();
        assert_eq!(back.title, "Test");
        assert_eq!(back.avg_minutes, 30);
        assert_eq!(back.sigma_minutes, Some(5));
        assert_eq!(back.parallelizable, Some(true));
    }

    #[test]
    fn save_schedule_request_default_mark_ids_empty() {
        let json = r#"{"entries":[]}"#;
        let req: SaveScheduleRequest = serde_json::from_str(json).unwrap();
        assert!(req.entries.is_empty());
        assert!(req.mark_scheduled_task_ids.is_empty());
    }
}
