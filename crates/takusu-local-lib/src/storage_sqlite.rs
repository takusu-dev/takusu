use async_trait::async_trait;
use sqlx::SqlitePool;
use sqlx::sqlite::SqlitePoolOptions;
use takusu_storage::{
    CreateHabit, CreateTask, GoogleCalEventRow, GoogleCalSettingsRow, HabitRow,
    SaveScheduleRequest, ScheduleRow, SettingsRow, Storage, StorageError, TaskQuery, TaskRow,
    TokenCreateResponse, TokenRow, UpdateGoogleCalSettings, UpdateHabit, UpdateSettings,
    UpdateTask, storage::StorageResult,
};

use crate::config::LocalConfig;

const MIGRATION_001: &str = include_str!("../migrations/001_init.sql");
const MIGRATION_002: &str = include_str!("../migrations/002_google_cal.sql");
const MIGRATION_003: &str = include_str!("../migrations/003_settings.sql");

pub struct SqliteStorage {
    pool: SqlitePool,
    root_token: String,
}

impl SqliteStorage {
    pub async fn init(
        cfg: &LocalConfig,
        root_token: String,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let url = ensure_create_mode(cfg.db_url());

        if let Some(path) = extract_db_path(&url)
            && let Some(parent) = std::path::Path::new(&path).parent()
        {
            std::fs::create_dir_all(parent).ok();
        }

        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect(&url)
            .await?;

        sqlx::raw_sql(MIGRATION_001).execute(&pool).await?;
        sqlx::raw_sql(MIGRATION_002).execute(&pool).await?;
        sqlx::raw_sql(MIGRATION_003).execute(&pool).await?;

        Ok(Self { pool, root_token })
    }

    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }
}

fn ensure_create_mode(db_url: &str) -> String {
    if !db_url.contains("mode=") {
        let separator = if db_url.contains('?') { '&' } else { '?' };
        format!("{db_url}{separator}mode=rwc")
    } else {
        db_url.to_string()
    }
}

fn extract_db_path(db_url: &str) -> Option<String> {
    let path = db_url.strip_prefix("sqlite:")?;
    if path.is_empty() || path.starts_with(':') {
        return None;
    }
    let path = path.split('?').next().unwrap();
    Some(path.to_string())
}

fn map_err(e: sqlx::Error) -> StorageError {
    StorageError::Internal(e.to_string())
}

#[async_trait]
impl Storage for SqliteStorage {
    /// root_token は生の値で比較 (環境変数で保持)。
    /// DB 内のトークンは SHA-256 でハッシュ化して保存、比較は hash vs hash。
    /// hash_token(token) は SHA-256 のため衝突耐性があり、
    /// hash == hash(root_token) は token == root_token と等価だが、
    /// ルートトークンが何らかの理由で SHA-256 hex で渡される場合に備えて残している。
    async fn verify_token(&self, token: &str) -> StorageResult<bool> {
        let hash = crate::auth::hash_token(token);
        if token == self.root_token || hash == crate::auth::hash_token(&self.root_token) {
            return Ok(true);
        }
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM tokens WHERE token_hash = ? AND revoked_at IS NULL",
        )
        .bind(&hash)
        .fetch_one(&self.pool)
        .await
        .map_err(map_err)?;
        Ok(count > 0)
    }

    async fn list_tasks(&self, query: &TaskQuery) -> StorageResult<Vec<TaskRow>> {
        let mut sql = String::from("SELECT * FROM tasks WHERE 1=1");
        let mut bindings: Vec<String> = Vec::new();
        if let Some(ref v) = query.status {
            sql.push_str(" AND status = ?");
            bindings.push(v.clone());
        }
        if let Some(ref v) = query.from {
            sql.push_str(" AND end_at >= ?");
            bindings.push(v.clone());
        }
        if let Some(ref v) = query.until {
            sql.push_str(" AND start_at <= ?");
            bindings.push(v.clone());
        }
        if let Some(ref v) = query.habit_id {
            sql.push_str(" AND habit_id = ?");
            bindings.push(v.clone());
        }
        sql.push_str(" ORDER BY created_at DESC");
        let mut q = sqlx::query_as::<_, TaskRow>(sqlx::AssertSqlSafe(sql.as_str()));
        for b in &bindings {
            q = q.bind(b);
        }
        q.fetch_all(&self.pool).await.map_err(map_err)
    }

    async fn get_task(&self, id: &str) -> StorageResult<TaskRow> {
        let full = resolve_task_id(&self.pool, id).await?;
        sqlx::query_as::<_, TaskRow>("SELECT * FROM tasks WHERE id = ?")
            .bind(&full)
            .fetch_one(&self.pool)
            .await
            .map_err(|e| match e {
                sqlx::Error::RowNotFound => StorageError::NotFound(format!("task {id} not found")),
                other => StorageError::Internal(other.to_string()),
            })
    }

    async fn create_task(&self, body: &CreateTask) -> StorageResult<TaskRow> {
        let id = uuid::Uuid::now_v7().to_string();
        let depends_json = serde_json::to_string(&body.depends.clone().unwrap_or_default())
            .unwrap_or_else(|_| "[]".to_string());
        let sigma = body.sigma_minutes.unwrap_or(0);
        let parallelizable = body.parallelizable.unwrap_or(false);
        let allows_parallel = body.allows_parallel.unwrap_or(false);
        let abandonability = body.abandonability.unwrap_or(0.5);
        sqlx::query(
            "INSERT INTO tasks (id, title, description, start_at, end_at, avg_minutes, sigma_minutes, depends, parallelizable, allows_parallel, abandonability, status, ical_uid, habit_id) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 'pending', ?, ?)"
        )
        .bind(&id)
        .bind(&body.title)
        .bind(&body.description)
        .bind(&body.start_at)
        .bind(&body.end_at)
        .bind(body.avg_minutes)
        .bind(sigma)
        .bind(&depends_json)
        .bind(parallelizable)
        .bind(allows_parallel)
        .bind(abandonability)
        .bind(&body.ical_uid)
        .bind(&body.habit_id)
        .execute(&self.pool)
        .await
        .map_err(map_err)?;
        sqlx::query_as::<_, TaskRow>("SELECT * FROM tasks WHERE id = ?")
            .bind(&id)
            .fetch_one(&self.pool)
            .await
            .map_err(map_err)
    }

    async fn update_task(&self, id: &str, body: &UpdateTask) -> StorageResult<TaskRow> {
        let full = resolve_task_id(&self.pool, id).await?;
        let existing = sqlx::query_as::<_, TaskRow>("SELECT * FROM tasks WHERE id = ?")
            .bind(&full)
            .fetch_one(&self.pool)
            .await
            .map_err(|e| match e {
                sqlx::Error::RowNotFound => StorageError::NotFound(format!("task {id} not found")),
                other => StorageError::Internal(other.to_string()),
            })?;

        let depends_json = body
            .depends
            .as_ref()
            .map(|d| serde_json::to_string(d).unwrap_or_else(|_| "[]".into()));
        let status = body.status.as_ref().unwrap_or(&existing.status);
        let validated = [
            "pending",
            "scheduled",
            "in_progress",
            "completed",
            "skipped",
        ];
        if !validated.contains(&status.as_str()) {
            return Err(StorageError::BadRequest(format!(
                "invalid status: {status}"
            )));
        }

        sqlx::query(
            "UPDATE tasks SET title=COALESCE(?,title), description=COALESCE(?,description), start_at=COALESCE(?,start_at), end_at=COALESCE(?,end_at), avg_minutes=COALESCE(?,avg_minutes), sigma_minutes=COALESCE(?,sigma_minutes), depends=COALESCE(?,depends), parallelizable=COALESCE(?,parallelizable), allows_parallel=COALESCE(?,allows_parallel), abandonability=COALESCE(?,abandonability), status=?, habit_id=COALESCE(?,habit_id), updated_at=datetime('now') WHERE id = ?"
        )
        .bind(body.title.as_ref())
        .bind(body.description.as_ref())
        .bind(body.start_at.as_ref())
        .bind(body.end_at.as_ref())
        .bind(body.avg_minutes)
        .bind(body.sigma_minutes)
        .bind(depends_json.as_ref())
        .bind(body.parallelizable)
        .bind(body.allows_parallel)
        .bind(body.abandonability)
        .bind(status)
        .bind(body.habit_id.as_ref())
        .bind(&full)
        .execute(&self.pool)
        .await
        .map_err(map_err)?;

        sqlx::query_as::<_, TaskRow>("SELECT * FROM tasks WHERE id = ?")
            .bind(&full)
            .fetch_one(&self.pool)
            .await
            .map_err(map_err)
    }

    async fn replace_task(&self, id: &str, body: &CreateTask) -> StorageResult<TaskRow> {
        let full = resolve_task_id(&self.pool, id).await?;
        let depends_json = serde_json::to_string(&body.depends.clone().unwrap_or_default())
            .unwrap_or_else(|_| "[]".into());
        let sigma = body.sigma_minutes.unwrap_or(0);
        let parallelizable = body.parallelizable.unwrap_or(false);
        let allows_parallel = body.allows_parallel.unwrap_or(false);
        let abandonability = body.abandonability.unwrap_or(0.5);
        sqlx::query(
            "UPDATE tasks SET title=?, description=?, start_at=?, end_at=?, avg_minutes=?, sigma_minutes=?, depends=?, parallelizable=?, allows_parallel=?, abandonability=?, habit_id=COALESCE(?,habit_id), updated_at=datetime('now') WHERE id = ?"
        )
        .bind(&body.title)
        .bind(&body.description)
        .bind(&body.start_at)
        .bind(&body.end_at)
        .bind(body.avg_minutes)
        .bind(sigma)
        .bind(&depends_json)
        .bind(parallelizable)
        .bind(allows_parallel)
        .bind(abandonability)
        .bind(&body.habit_id)
        .bind(&full)
        .execute(&self.pool)
        .await
        .map_err(map_err)?;
        sqlx::query_as::<_, TaskRow>("SELECT * FROM tasks WHERE id = ?")
            .bind(&full)
            .fetch_one(&self.pool)
            .await
            .map_err(map_err)
    }

    async fn delete_task(&self, id: &str) -> StorageResult<()> {
        let full = resolve_task_id(&self.pool, id).await?;
        sqlx::query("DELETE FROM tasks WHERE id = ?")
            .bind(&full)
            .execute(&self.pool)
            .await
            .map_err(map_err)?;
        Ok(())
    }

    async fn list_habits(&self) -> StorageResult<Vec<HabitRow>> {
        sqlx::query_as::<_, HabitRow>("SELECT * FROM habits ORDER BY created_at DESC")
            .fetch_all(&self.pool)
            .await
            .map_err(map_err)
    }

    async fn get_habit(&self, id: &str) -> StorageResult<HabitRow> {
        sqlx::query_as::<_, HabitRow>("SELECT * FROM habits WHERE id = ?")
            .bind(id)
            .fetch_one(&self.pool)
            .await
            .map_err(|e| match e {
                sqlx::Error::RowNotFound => StorageError::NotFound(format!("habit {id} not found")),
                other => StorageError::Internal(other.to_string()),
            })
    }

    async fn create_habit(&self, body: &CreateHabit) -> StorageResult<HabitRow> {
        let id = uuid::Uuid::now_v7().to_string();
        let sigma = body.sigma_minutes.unwrap_or(0);
        let parallelizable = body.parallelizable.unwrap_or(false);
        let allows_parallel = body.allows_parallel.unwrap_or(false);
        let abandonability = body.abandonability.unwrap_or(0.5);
        sqlx::query(
            "INSERT INTO habits (id, title, description, recurrence, start_time, end_time, avg_minutes, sigma_minutes, parallelizable, allows_parallel, abandonability, active) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 1)"
        )
        .bind(&id)
        .bind(&body.title)
        .bind(&body.description)
        .bind(&body.recurrence)
        .bind(&body.start_time)
        .bind(&body.end_time)
        .bind(body.avg_minutes)
        .bind(sigma)
        .bind(parallelizable)
        .bind(allows_parallel)
        .bind(abandonability)
        .execute(&self.pool)
        .await
        .map_err(map_err)?;
        sqlx::query_as::<_, HabitRow>("SELECT * FROM habits WHERE id = ?")
            .bind(&id)
            .fetch_one(&self.pool)
            .await
            .map_err(map_err)
    }

    async fn update_habit(&self, id: &str, body: &UpdateHabit) -> StorageResult<HabitRow> {
        sqlx::query(
            "UPDATE habits SET title=COALESCE(?,title), description=COALESCE(?,description), recurrence=COALESCE(?,recurrence), start_time=COALESCE(?,start_time), end_time=COALESCE(?,end_time), avg_minutes=COALESCE(?,avg_minutes), sigma_minutes=COALESCE(?,sigma_minutes), parallelizable=COALESCE(?,parallelizable), allows_parallel=COALESCE(?,allows_parallel), abandonability=COALESCE(?,abandonability), active=COALESCE(?,active), updated_at=datetime('now') WHERE id = ?"
        )
        .bind(body.title.as_ref())
        .bind(body.description.as_ref())
        .bind(body.recurrence.as_ref())
        .bind(body.start_time.as_ref())
        .bind(body.end_time.as_ref())
        .bind(body.avg_minutes)
        .bind(body.sigma_minutes)
        .bind(body.parallelizable)
        .bind(body.allows_parallel)
        .bind(body.abandonability)
        .bind(body.active)
        .bind(id)
        .execute(&self.pool)
        .await
        .map_err(map_err)?;
        sqlx::query_as::<_, HabitRow>("SELECT * FROM habits WHERE id = ?")
            .bind(id)
            .fetch_one(&self.pool)
            .await
            .map_err(|e| match e {
                sqlx::Error::RowNotFound => StorageError::NotFound(format!("habit {id} not found")),
                other => StorageError::Internal(other.to_string()),
            })
    }

    async fn replace_habit(&self, id: &str, body: &CreateHabit) -> StorageResult<HabitRow> {
        let sigma = body.sigma_minutes.unwrap_or(0);
        let parallelizable = body.parallelizable.unwrap_or(false);
        let allows_parallel = body.allows_parallel.unwrap_or(false);
        let abandonability = body.abandonability.unwrap_or(0.5);
        sqlx::query(
            "UPDATE habits SET title=?, description=?, recurrence=?, start_time=?, end_time=?, avg_minutes=?, sigma_minutes=?, parallelizable=?, allows_parallel=?, abandonability=?, updated_at=datetime('now') WHERE id = ?"
        )
        .bind(&body.title)
        .bind(&body.description)
        .bind(&body.recurrence)
        .bind(&body.start_time)
        .bind(&body.end_time)
        .bind(body.avg_minutes)
        .bind(sigma)
        .bind(parallelizable)
        .bind(allows_parallel)
        .bind(abandonability)
        .bind(id)
        .execute(&self.pool)
        .await
        .map_err(map_err)?;
        sqlx::query_as::<_, HabitRow>("SELECT * FROM habits WHERE id = ?")
            .bind(id)
            .fetch_one(&self.pool)
            .await
            .map_err(map_err)
    }

    async fn delete_habit(&self, id: &str) -> StorageResult<()> {
        sqlx::query("DELETE FROM habits WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(map_err)?;
        Ok(())
    }

    async fn get_schedule(&self) -> StorageResult<Option<ScheduleRow>> {
        sqlx::query_as::<_, ScheduleRow>("SELECT * FROM schedules WHERE id = 'active'")
            .fetch_optional(&self.pool)
            .await
            .map_err(map_err)
    }

    async fn save_schedule(&self, req: &SaveScheduleRequest) -> StorageResult<ScheduleRow> {
        let schedule_json = serde_json::to_string(&req.entries)
            .map_err(|e| StorageError::Internal(format!("serialize schedule: {e}")))?;
        let now = jiff::Timestamp::now().to_string();
        sqlx::query(
            "INSERT INTO schedules (id, created_at, updated_at, schedule) VALUES ('active', ?, ?, ?) ON CONFLICT(id) DO UPDATE SET schedule=excluded.schedule, updated_at=excluded.updated_at"
        )
        .bind(&now)
        .bind(&now)
        .bind(&schedule_json)
        .execute(&self.pool)
        .await
        .map_err(map_err)?;
        for id in &req.mark_scheduled_task_ids {
            sqlx::query(
                "UPDATE tasks SET status = 'scheduled', updated_at = datetime('now') WHERE id = ?",
            )
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(map_err)?;
        }
        sqlx::query_as::<_, ScheduleRow>("SELECT * FROM schedules WHERE id = 'active'")
            .fetch_one(&self.pool)
            .await
            .map_err(map_err)
    }

    async fn clear_schedule(&self) -> StorageResult<()> {
        sqlx::query("DELETE FROM schedules WHERE id = 'active'")
            .execute(&self.pool)
            .await
            .map_err(map_err)?;
        Ok(())
    }

    async fn create_token(&self, label: Option<&str>) -> StorageResult<TokenCreateResponse> {
        let new_token = format!("tsk_{}", uuid::Uuid::now_v7());
        let hash = crate::auth::hash_token(&new_token);
        let label_opt = label.filter(|s| !s.is_empty());
        sqlx::query(
            "INSERT INTO tokens (token_hash, label, created_by) VALUES (?, ?, 'authenticated')",
        )
        .bind(&hash)
        .bind(label_opt)
        .execute(&self.pool)
        .await
        .map_err(map_err)?;
        let row: TokenRow =
            sqlx::query_as::<_, TokenRow>("SELECT * FROM tokens WHERE token_hash = ?")
                .bind(&hash)
                .fetch_one(&self.pool)
                .await
                .map_err(map_err)?;
        Ok(TokenCreateResponse {
            id: row.id,
            token: new_token,
            label: row.label,
            created_at: row.created_at,
        })
    }

    async fn list_tokens(&self) -> StorageResult<Vec<TokenRow>> {
        sqlx::query_as::<_, TokenRow>("SELECT * FROM tokens ORDER BY created_at DESC")
            .fetch_all(&self.pool)
            .await
            .map_err(map_err)
    }

    async fn revoke_token(&self, id: i64) -> StorageResult<()> {
        let result = sqlx::query(
            "UPDATE tokens SET revoked_at = datetime('now') WHERE id = ? AND revoked_at IS NULL",
        )
        .bind(id)
        .execute(&self.pool)
        .await
        .map_err(map_err)?;
        if result.rows_affected() == 0 {
            return Err(StorageError::NotFound(format!(
                "token {id} not found or already revoked"
            )));
        }
        Ok(())
    }

    async fn get_settings(&self) -> StorageResult<SettingsRow> {
        sqlx::query_as::<_, SettingsRow>("SELECT * FROM settings WHERE id = 'active'")
            .fetch_one(&self.pool)
            .await
            .map_err(|e| match e {
                sqlx::Error::RowNotFound => StorageError::NotFound("settings not found".into()),
                other => StorageError::Internal(other.to_string()),
            })
    }

    async fn update_settings(&self, body: &UpdateSettings) -> StorageResult<SettingsRow> {
        let existing = self.get_settings().await?;
        let tz = body.tz.clone().unwrap_or(existing.tz);
        let sleep_start = body.sleep_start.clone().unwrap_or(existing.sleep_start);
        let sleep_end = body.sleep_end.clone().unwrap_or(existing.sleep_end);
        sqlx::query(
            "UPDATE settings SET tz = ?, sleep_start = ?, sleep_end = ?, updated_at = datetime('now') WHERE id = 'active'",
        )
        .bind(&tz)
        .bind(&sleep_start)
        .bind(&sleep_end)
        .execute(&self.pool)
        .await
        .map_err(map_err)?;
        Ok(SettingsRow {
            id: existing.id,
            tz,
            sleep_start,
            sleep_end,
            created_at: existing.created_at,
            updated_at: String::new(),
        })
    }

    async fn get_gcal_settings(&self) -> StorageResult<GoogleCalSettingsRow> {
        let row = sqlx::query_as::<_, GoogleCalSettingsRow>(
            "SELECT * FROM google_cal_settings WHERE id = 'active'",
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(map_err)?;
        Ok(row.unwrap_or_else(|| GoogleCalSettingsRow {
            id: "active".to_string(),
            enabled: false,
            calendar_id: "primary".to_string(),
            client_id: String::new(),
            client_secret: String::new(),
            refresh_token: None,
            created_at: String::new(),
            updated_at: String::new(),
        }))
    }

    async fn update_gcal_settings(
        &self,
        body: &UpdateGoogleCalSettings,
    ) -> StorageResult<GoogleCalSettingsRow> {
        let existing = self.get_gcal_settings().await?;
        let enabled = body.enabled.unwrap_or(existing.enabled);
        let calendar_id = body
            .calendar_id
            .clone()
            .unwrap_or_else(|| existing.calendar_id.clone());
        let client_id = body
            .client_id
            .clone()
            .unwrap_or_else(|| existing.client_id.clone());
        let client_secret = body
            .client_secret
            .clone()
            .unwrap_or_else(|| existing.client_secret.clone());
        let refresh_token = body
            .refresh_token
            .clone()
            .or_else(|| existing.refresh_token.clone());

        sqlx::query(
            "INSERT INTO google_cal_settings (id, enabled, calendar_id, client_id, client_secret, refresh_token) VALUES ('active', ?, ?, ?, ?, ?) ON CONFLICT(id) DO UPDATE SET enabled=excluded.enabled, calendar_id=excluded.calendar_id, client_id=excluded.client_id, client_secret=excluded.client_secret, refresh_token=excluded.refresh_token, updated_at=datetime('now')"
        )
        .bind(enabled)
        .bind(&calendar_id)
        .bind(&client_id)
        .bind(&client_secret)
        .bind(&refresh_token)
        .execute(&self.pool)
        .await
        .map_err(map_err)?;

        Ok(GoogleCalSettingsRow {
            id: "active".to_string(),
            enabled,
            calendar_id,
            client_id,
            client_secret,
            refresh_token,
            created_at: existing.created_at,
            updated_at: String::new(),
        })
    }

    async fn list_gcal_mappings(&self) -> StorageResult<Vec<GoogleCalEventRow>> {
        sqlx::query_as::<_, GoogleCalEventRow>("SELECT * FROM google_cal_events")
            .fetch_all(&self.pool)
            .await
            .map_err(map_err)
    }

    async fn upsert_gcal_mappings(&self, mappings: &[(String, String)]) -> StorageResult<()> {
        for (task_id, event_id) in mappings {
            sqlx::query(
                "INSERT INTO google_cal_events (task_id, google_event_id) VALUES (?, ?) ON CONFLICT(task_id) DO UPDATE SET google_event_id=excluded.google_event_id, updated_at=datetime('now')"
            )
            .bind(task_id)
            .bind(event_id)
            .execute(&self.pool)
            .await
            .map_err(map_err)?;
        }
        Ok(())
    }

    async fn delete_gcal_mappings(&self, task_ids: &[String]) -> StorageResult<()> {
        for task_id in task_ids {
            sqlx::query("DELETE FROM google_cal_events WHERE task_id = ?")
                .bind(task_id)
                .execute(&self.pool)
                .await
                .map_err(map_err)?;
        }
        Ok(())
    }

    async fn clear_gcal_mappings(&self) -> StorageResult<()> {
        sqlx::query("DELETE FROM google_cal_events")
            .execute(&self.pool)
            .await
            .map_err(map_err)?;
        Ok(())
    }

    async fn health_check(&self) -> StorageResult<String> {
        // A cheap round-trip to the DB confirms the connection is alive.
        let v: String = sqlx::query_scalar("SELECT sqlite_version()")
            .fetch_one(&self.pool)
            .await
            .map_err(map_err)?;
        Ok(format!("sqlite ok (v{v})"))
    }
}

async fn resolve_task_id(pool: &SqlitePool, id: &str) -> StorageResult<String> {
    if id.contains('-') {
        let exists: bool = sqlx::query_scalar("SELECT COUNT(*) > 0 FROM tasks WHERE id = ?")
            .bind(id)
            .fetch_one(pool)
            .await
            .map_err(map_err)?;
        if exists {
            return Ok(id.to_string());
        }
    } else {
        let matches: Vec<String> =
            sqlx::query_scalar("SELECT id FROM tasks WHERE id LIKE ? || '%'")
                .bind(id)
                .fetch_all(pool)
                .await
                .map_err(map_err)?;
        match matches.len() {
            0 => {}
            1 => return Ok(matches.into_iter().next().unwrap()),
            _ => {
                return Err(StorageError::BadRequest(format!(
                    "ambiguous task id prefix: {id}"
                )));
            }
        }
    }
    Err(StorageError::NotFound(format!("task {id} not found")))
}
