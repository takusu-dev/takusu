use serde_json::json;
use takusu_client::*;

#[test]
fn create_task_serialization() {
    let ct = CreateTask {
        title: "Test task".to_string(),
        description: Some("A test".to_string()),
        start_at: None,
        end_at: "2025-06-05T18:00:00Z".to_string(),
        avg_minutes: 60,
        sigma_minutes: Some(10),
        depends: Some(vec!["dep1".to_string()]),
        parallelizable: Some(true),
        allows_parallel: Some(false),
        abandonability: Some(0.3),
    };

    let json = serde_json::to_value(&ct).unwrap();
    assert_eq!(json["title"], "Test task");
    assert_eq!(json["avg_minutes"], 60);
    assert_eq!(json["sigma_minutes"], 10);
    assert_eq!(json["abandonability"], 0.3);
}

#[test]
fn create_task_defaults_are_skipped() {
    let ct = CreateTask {
        title: "Minimal".to_string(),
        description: None,
        start_at: None,
        end_at: "2025-06-05T18:00:00Z".to_string(),
        avg_minutes: 30,
        sigma_minutes: None,
        depends: None,
        parallelizable: None,
        allows_parallel: None,
        abandonability: None,
    };

    let json = serde_json::to_value(&ct).unwrap();
    assert_eq!(json["title"], "Minimal");
    assert_eq!(json["avg_minutes"], 30);
    assert!(!json.as_object().unwrap().contains_key("sigma_minutes"));
    assert!(!json.as_object().unwrap().contains_key("parallelizable"));
}

#[test]
fn update_task_serialization() {
    let ut = UpdateTask {
        title: Some("Updated".to_string()),
        status: Some("in_progress".to_string()),
        parallelizable: Some(true),
        ..Default::default()
    };

    let json = serde_json::to_value(&ut).unwrap();
    assert_eq!(json["title"], "Updated");
    assert_eq!(json["status"], "in_progress");
    assert_eq!(json["parallelizable"], true);
}

#[test]
fn update_task_default_is_empty() {
    let ut = UpdateTask::default();
    let json = serde_json::to_value(&ut).unwrap();
    assert!(json.as_object().unwrap().is_empty());
}

#[test]
fn generate_schedule_serialization() {
    let gs = GenerateSchedule {
        task_ids: None,
        until: "2025-06-05T23:59:59Z".to_string(),
        sleep: "recommended".to_string(),
    };

    let json = serde_json::to_value(&gs).unwrap();
    assert_eq!(json["until"], "2025-06-05T23:59:59Z");
    assert_eq!(json["sleep"], "recommended");
    assert!(!json.as_object().unwrap().contains_key("task_ids"));
}

#[test]
fn reschedule_serialization() {
    let rs = Reschedule {
        mode: "range".to_string(),
        from: Some("2025-06-05T08:00:00Z".to_string()),
        until: Some("2025-06-05T18:00:00Z".to_string()),
        task_ids: None,
        pinned: vec![],
        sleep: "recommended".to_string(),
    };

    let json = serde_json::to_value(&rs).unwrap();
    assert_eq!(json["mode"], "range");
    assert_eq!(json["sleep"], "recommended");
}

#[test]
fn move_entry_serialization() {
    let me = MoveEntry {
        start_at: "2025-06-05T12:00:00Z".to_string(),
        force: true,
    };

    let json = serde_json::to_value(&me).unwrap();
    assert_eq!(json["start_at"], "2025-06-05T12:00:00Z");
    assert_eq!(json["force"], true);
}

#[test]
fn move_entry_default_force_omitted() {
    let me = MoveEntry {
        start_at: "2025-06-05T12:00:00Z".to_string(),
        force: false,
    };

    let json = serde_json::to_value(&me).unwrap();
    assert_eq!(json["start_at"], "2025-06-05T12:00:00Z");
    assert_eq!(json["force"], false);
}

#[test]
fn create_habit_serialization() {
    let ch = CreateHabit {
        title: "Daily standup".to_string(),
        description: None,
        recurrence: "daily".to_string(),
        start_time: "09:00".to_string(),
        end_time: "09:30".to_string(),
        avg_minutes: 30,
        sigma_minutes: None,
        parallelizable: None,
        allows_parallel: None,
        abandonability: None,
    };

    let json = serde_json::to_value(&ch).unwrap();
    assert_eq!(json["title"], "Daily standup");
    assert_eq!(json["recurrence"], "daily");
}

#[test]
fn update_settings_serialization() {
    let us = UpdateSettings {
        tz: Some("Asia/Tokyo".to_string()),
        sleep_start: Some("23:00".to_string()),
        sleep_end: None,
    };

    let json = serde_json::to_value(&us).unwrap();
    assert_eq!(json["tz"], "Asia/Tokyo");
    assert_eq!(json["sleep_start"], "23:00");
    assert!(!json.as_object().unwrap().contains_key("sleep_end"));
}

#[test]
fn task_query_default_is_empty() {
    let tq = TaskQuery::default();
    assert!(tq.status.is_none());
    assert!(tq.from.is_none());
    assert!(tq.until.is_none());
    assert!(tq.habit_id.is_none());
}

#[test]
fn client_error_display() {
    let api_err = ClientError::Api {
        status: 404,
        body: "not found".to_string(),
    };
    assert!(!format!("{api_err}").is_empty());
}

#[test]
fn task_row_deserialization() {
    let json = json!({
        "id": "task-123",
        "title": "Write tests",
        "description": "Add tests",
        "start_at": "2025-06-05T09:00:00Z",
        "end_at": "2025-06-05T10:00:00Z",
        "avg_minutes": 60,
        "sigma_minutes": 5,
        "depends": "[]",
        "parallelizable": false,
        "allows_parallel": false,
        "abandonability": 0.5,
        "status": "pending",
        "habit_id": null,
        "ical_uid": null,
        "created_at": "2025-06-01T00:00:00Z",
        "updated_at": "2025-06-01T00:00:00Z"
    });
    let tr: TaskRow = serde_json::from_value(json).unwrap();
    assert_eq!(tr.id, "task-123");
    assert_eq!(tr.status, "pending");
    assert_eq!(tr.depends, "[]");
}

#[test]
fn schedule_entry_roundtrip() {
    let original = json!({
        "task_id": "t1",
        "start_at": "2025-06-05T10:00:00Z",
        "end_at": "2025-06-05T11:00:00Z"
    });

    let se: ScheduleEntry = serde_json::from_value(original.clone()).unwrap();
    assert_eq!(se.task_id, "t1");
    let json = serde_json::to_value(&se).unwrap();
    assert_eq!(json["task_id"], "t1");
}

#[test]
fn token_row_deserialization() {
    let json = json!({
        "id": 1,
        "token_hash": "abc123",
        "label": "test",
        "created_by": "root",
        "created_at": "2025-06-01T00:00:00Z",
        "revoked_at": null
    });
    let tr: TokenRow = serde_json::from_value(json).unwrap();
    assert_eq!(tr.id, 1);
    assert!(tr.revoked_at.is_none());
}

#[test]
fn token_create_response_deserialization() {
    let json = json!({
        "id": 1,
        "token": "tsk_new_token_value",
        "label": "cli-token",
        "created_at": "2025-06-01T00:00:00Z"
    });
    let tcr: TokenCreateResponse = serde_json::from_value(json).unwrap();
    assert_eq!(tcr.token, "tsk_new_token_value");
    assert_eq!(tcr.id, 1);
}

#[test]
fn settings_response_deserialization() {
    let json = json!({
        "tz": "UTC",
        "sleep_start": "22:00",
        "sleep_end": "06:00"
    });
    let sr: SettingsResponse = serde_json::from_value(json).unwrap();
    assert_eq!(sr.tz, "UTC");
    assert_eq!(sr.sleep_start, "22:00");
}

#[test]
fn schedule_row_deserialization() {
    let json = json!({
        "id": "active",
        "created_at": "2025-06-01T00:00:00Z",
        "updated_at": "2025-06-01T00:00:00Z",
        "schedule": "[{\"task_id\":\"t1\",\"start_at\":\"...\",\"end_at\":\"...\"}]"
    });
    let sr: ScheduleRow = serde_json::from_value(json).unwrap();
    assert_eq!(sr.id, "active");
    assert!(sr.schedule.contains("t1"));
}

#[test]
fn sync_settings_response_deserialization() {
    let json = json!({
        "enabled": true,
        "calendar_id": "primary",
        "client_id": "client-123",
        "has_client_secret": true,
        "has_refresh_token": false
    });
    let resp: SyncSettingsResponse = serde_json::from_value(json).unwrap();
    assert!(resp.enabled);
    assert!(!resp.has_refresh_token);
}

#[test]
fn update_sync_settings_serialization() {
    let uss = UpdateSyncSettings {
        enabled: Some(true),
        calendar_id: Some("primary".to_string()),
        client_id: None,
        client_secret: None,
        refresh_token: None,
    };

    let json = serde_json::to_value(&uss).unwrap();
    assert_eq!(json["enabled"], true);
    assert!(!json.as_object().unwrap().contains_key("client_id"));
}

#[test]
fn habit_row_deserialization() {
    let json = json!({
        "id": "h1",
        "title": "Exercise",
        "description": null,
        "recurrence": "weekly",
        "start_time": "07:00",
        "end_time": "08:00",
        "avg_minutes": 60,
        "sigma_minutes": 5,
        "parallelizable": false,
        "allows_parallel": false,
        "abandonability": 0.3,
        "active": true,
        "created_at": "2025-06-01T00:00:00Z",
        "updated_at": "2025-06-01T00:00:00Z"
    });
    let hr: HabitRow = serde_json::from_value(json).unwrap();
    assert_eq!(hr.id, "h1");
    assert!(hr.active);
}

#[test]
fn client_error_is_send_sync() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<ClientError>();
}
