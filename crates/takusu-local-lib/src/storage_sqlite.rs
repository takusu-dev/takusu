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
const MIGRATION_004: &str = include_str!("../migrations/004_indexes.sql");
const MIGRATION_005: &str = include_str!("../migrations/005_task_display_id.sql");
const MIGRATION_006: &str = include_str!("../migrations/006_user_edited.sql");
const MIGRATION_007: &str = include_str!("../migrations/007_task_display_id_seq.sql");

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
        sqlx::raw_sql(MIGRATION_004).execute(&pool).await?;

        // Migration 005 uses ALTER TABLE ADD COLUMN which is not idempotent
        // (SQLite has no IF NOT EXISTS for ADD COLUMN). Check whether the
        // display_id column already exists before running it.
        let has_display_id: bool = sqlx::query_scalar(
            "SELECT COUNT(*) > 0 FROM pragma_table_info('tasks') WHERE name = 'display_id'",
        )
        .fetch_one(&pool)
        .await?;
        if !has_display_id {
            sqlx::raw_sql(MIGRATION_005).execute(&pool).await?;
        }
        let has_user_edited: bool = sqlx::query_scalar(
            "SELECT COUNT(*) > 0 FROM pragma_table_info('tasks') WHERE name = 'user_edited'",
        )
        .fetch_one(&pool)
        .await?;
        if !has_user_edited {
            sqlx::raw_sql(MIGRATION_006).execute(&pool).await?;
        }

        // Migration 007 creates the display_id sequence table (idempotent).
        sqlx::raw_sql(MIGRATION_007).execute(&pool).await?;

        // Migration 008 adds fixed column to habits and tasks (not idempotent).
        // SQLite has no IF NOT EXISTS for ADD COLUMN, so check each table
        // separately and run only the missing ALTER. This recovers from a
        // partial migration 008 failure where one table was altered but the
        // other was not (e.g. a crash mid-migration).
        let has_tasks_fixed: bool = sqlx::query_scalar(
            "SELECT COUNT(*) > 0 FROM pragma_table_info('tasks') WHERE name = 'fixed'",
        )
        .fetch_one(&pool)
        .await?;
        if !has_tasks_fixed {
            sqlx::query("ALTER TABLE tasks ADD COLUMN fixed BOOLEAN NOT NULL DEFAULT 0")
                .execute(&pool)
                .await?;
        }
        let has_habits_fixed: bool = sqlx::query_scalar(
            "SELECT COUNT(*) > 0 FROM pragma_table_info('habits') WHERE name = 'fixed'",
        )
        .fetch_one(&pool)
        .await?;
        if !has_habits_fixed {
            sqlx::query("ALTER TABLE habits ADD COLUMN fixed BOOLEAN NOT NULL DEFAULT 0")
                .execute(&pool)
                .await?;
        }

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
            // start_at is nullable: NULL <= value evaluates to NULL
            // (excluded). Include tasks with no explicit start time so
            // range queries don't silently drop them.
            sql.push_str(" AND (start_at IS NULL OR start_at <= ?)");
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
        let resolved_depends = resolve_depends(&self.pool, body.depends.as_deref()).await?;
        let depends_json =
            serde_json::to_string(&resolved_depends).unwrap_or_else(|_| "[]".to_string());
        // sigma 未指定時は avg の 20% をデフォルトにする (確定タスクでない限りある程度バッファを見込む)
        let sigma = body.sigma_minutes.unwrap_or((body.avg_minutes / 5).max(1));
        let parallelizable = body.parallelizable.unwrap_or(false);
        let allows_parallel = body.allows_parallel.unwrap_or(false);
        let abandonability = body.abandonability.unwrap_or(0.5);
        let fixed = body.fixed.unwrap_or(false);
        // Atomically reserve a monotonic display_id from the sequence table.
        // This prevents display_id reuse after task deletion (#186).
        let display_id: i64 = sqlx::query_scalar(
            "UPDATE task_display_id_seq SET next_id = next_id + 1 RETURNING next_id - 1",
        )
        .fetch_one(&self.pool)
        .await
        .map_err(map_err)?;
        sqlx::query(
            "INSERT INTO tasks (id, display_id, title, description, start_at, end_at, avg_minutes, sigma_minutes, depends, parallelizable, allows_parallel, abandonability, status, ical_uid, habit_id, fixed) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 'pending', ?, ?, ?)"
        )
        .bind(&id)
        .bind(display_id)
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
        .bind(fixed)
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

        let depends_json = if let Some(ref deps) = body.depends {
            let resolved = resolve_depends(&self.pool, Some(deps)).await?;
            Some(serde_json::to_string(&resolved).unwrap_or_else(|_| "[]".into()))
        } else {
            None
        };
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
            "UPDATE tasks SET title=COALESCE(?,title), description=COALESCE(?,description), start_at=COALESCE(?,start_at), end_at=COALESCE(?,end_at), avg_minutes=COALESCE(?,avg_minutes), sigma_minutes=COALESCE(?,sigma_minutes), depends=COALESCE(?,depends), parallelizable=COALESCE(?,parallelizable), allows_parallel=COALESCE(?,allows_parallel), abandonability=COALESCE(?,abandonability), status=?, habit_id=COALESCE(?,habit_id), user_edited=COALESCE(?,user_edited), fixed=COALESCE(?,fixed), updated_at=datetime('now') WHERE id = ?"
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
        .bind(body.user_edited)
        .bind(body.fixed)
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
        let resolved_depends = resolve_depends(&self.pool, body.depends.as_deref()).await?;
        let depends_json = serde_json::to_string(&resolved_depends).unwrap_or_else(|_| "[]".into());
        let sigma = body.sigma_minutes.unwrap_or((body.avg_minutes / 5).max(1));
        let parallelizable = body.parallelizable.unwrap_or(false);
        let allows_parallel = body.allows_parallel.unwrap_or(false);
        let abandonability = body.abandonability.unwrap_or(0.5);
        let fixed = body.fixed.unwrap_or(false);
        sqlx::query(
            "UPDATE tasks SET title=?, description=?, start_at=?, end_at=?, avg_minutes=?, sigma_minutes=?, depends=?, parallelizable=?, allows_parallel=?, abandonability=?, habit_id=COALESCE(?,habit_id), fixed=?, updated_at=datetime('now') WHERE id = ?"
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
        .bind(fixed)
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
        let sigma = body.sigma_minutes.unwrap_or((body.avg_minutes / 5).max(1));
        let parallelizable = body.parallelizable.unwrap_or(false);
        let allows_parallel = body.allows_parallel.unwrap_or(false);
        let abandonability = body.abandonability.unwrap_or(0.5);
        let fixed = body.fixed.unwrap_or(false);
        sqlx::query(
            "INSERT INTO habits (id, title, description, recurrence, start_time, end_time, avg_minutes, sigma_minutes, parallelizable, allows_parallel, abandonability, active, fixed) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 1, ?)"
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
        .bind(fixed)
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
            "UPDATE habits SET title=COALESCE(?,title), description=COALESCE(?,description), recurrence=COALESCE(?,recurrence), start_time=COALESCE(?,start_time), end_time=COALESCE(?,end_time), avg_minutes=COALESCE(?,avg_minutes), sigma_minutes=COALESCE(?,sigma_minutes), parallelizable=COALESCE(?,parallelizable), allows_parallel=COALESCE(?,allows_parallel), abandonability=COALESCE(?,abandonability), active=COALESCE(?,active), fixed=COALESCE(?,fixed), updated_at=datetime('now') WHERE id = ?"
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
        .bind(body.fixed)
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
        let sigma = body.sigma_minutes.unwrap_or((body.avg_minutes / 5).max(1));
        let parallelizable = body.parallelizable.unwrap_or(false);
        let allows_parallel = body.allows_parallel.unwrap_or(false);
        let abandonability = body.abandonability.unwrap_or(0.5);
        let fixed = body.fixed.unwrap_or(false);
        sqlx::query(
            "UPDATE habits SET title=?, description=?, recurrence=?, start_time=?, end_time=?, avg_minutes=?, sigma_minutes=?, parallelizable=?, allows_parallel=?, abandonability=?, fixed=?, updated_at=datetime('now') WHERE id = ?"
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
        .bind(fixed)
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
        // Delete tasks referencing this habit before deleting the habit,
        // so the foreign-key constraint (enforced on D1) does not block
        // deletion of habits that have already generated tasks (#240).
        // The client confirms with the user before issuing the delete
        // when there are associated tasks. All statements run in a
        // single transaction so a partial failure cannot leave the
        // database with tasks deleted but the habit still present.
        // google_cal_events mappings for those tasks are also cleaned
        // up because foreign keys are not enabled at runtime (the
        // ON DELETE CASCADE in the schema only fires with
        // PRAGMA foreign_keys = ON, which this codebase does not set).
        let mut tx = self.pool.begin().await.map_err(map_err)?;
        sqlx::query("DELETE FROM google_cal_events WHERE task_id IN (SELECT id FROM tasks WHERE habit_id = ?)")
            .bind(id)
            .execute(&mut *tx)
            .await
            .map_err(map_err)?;
        sqlx::query("DELETE FROM tasks WHERE habit_id = ?")
            .bind(id)
            .execute(&mut *tx)
            .await
            .map_err(map_err)?;
        sqlx::query("DELETE FROM habits WHERE id = ?")
            .bind(id)
            .execute(&mut *tx)
            .await
            .map_err(map_err)?;
        tx.commit().await.map_err(map_err)?;
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
        // Wrap the schedule upsert and the task status updates in a single
        // transaction so a failure mid-way cannot leave the schedule saved
        // but some tasks still marked pending (#289).
        let mut tx = self.pool.begin().await.map_err(map_err)?;
        sqlx::query(
            "INSERT INTO schedules (id, created_at, updated_at, schedule) VALUES ('active', ?, ?, ?) ON CONFLICT(id) DO UPDATE SET schedule=excluded.schedule, updated_at=excluded.updated_at"
        )
        .bind(&now)
        .bind(&now)
        .bind(&schedule_json)
        .execute(&mut *tx)
        .await
        .map_err(map_err)?;
        for id in &req.mark_scheduled_task_ids {
            sqlx::query(
                "UPDATE tasks SET status = 'scheduled', updated_at = datetime('now') WHERE id = ?",
            )
            .bind(id)
            .execute(&mut *tx)
            .await
            .map_err(map_err)?;
        }
        tx.commit().await.map_err(map_err)?;
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
        // Re-query to return the actual updated_at the DB set (#290).
        sqlx::query_as::<_, SettingsRow>("SELECT * FROM settings WHERE id = 'active'")
            .fetch_one(&self.pool)
            .await
            .map_err(map_err)
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

        // Re-query to return the actual updated_at the DB set (#290).
        sqlx::query_as::<_, GoogleCalSettingsRow>(
            "SELECT * FROM google_cal_settings WHERE id = 'active'",
        )
        .fetch_one(&self.pool)
        .await
        .map_err(map_err)
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
    // Numeric input → display_id lookup only (no UUID prefix fallthrough).
    if let Ok(num) = id.parse::<i64>() {
        return sqlx::query_scalar::<_, String>("SELECT id FROM tasks WHERE display_id = ?")
            .bind(num)
            .fetch_optional(pool)
            .await
            .map_err(map_err)?
            .ok_or_else(|| StorageError::NotFound(format!("task {id} not found")));
    }
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

/// Resolve a list of dependency references (display_id numbers or UUIDs/prefixes)
/// to full UUID strings. Entries that are already full UUIDs are passed through.
async fn resolve_depends(pool: &SqlitePool, deps: Option<&[String]>) -> StorageResult<Vec<String>> {
    let Some(deps) = deps else {
        return Ok(Vec::new());
    };
    let mut resolved = Vec::with_capacity(deps.len());
    for d in deps {
        resolved.push(resolve_task_id(pool, d).await?);
    }
    Ok(resolved)
}
