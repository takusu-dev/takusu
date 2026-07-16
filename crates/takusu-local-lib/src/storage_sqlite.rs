use async_trait::async_trait;
use sqlx::SqlitePool;
use sqlx::sqlite::SqlitePoolOptions;
use takusu_storage::{
    CreateHabit, CreateHabitPause, CreateSkill, CreateTask, GoogleCalEventRow,
    GoogleCalSettingsRow, HabitPauseRow, HabitRow, HabitStepInput, HabitStepRow,
    SaveScheduleRequest, ScheduleRow, SettingsRow, SkillRow, Storage, StorageError, TaskQuery,
    TaskRow, TokenCreateResponse, TokenRow, UpdateGoogleCalSettings, UpdateHabit, UpdateSettings,
    UpdateSkill, UpdateTask, storage::StorageResult,
};

use crate::config::LocalConfig;

const MIGRATION_001: &str = include_str!("../migrations/001_init.sql");
const MIGRATION_002: &str = include_str!("../migrations/002_google_cal.sql");
const MIGRATION_003: &str = include_str!("../migrations/003_settings.sql");
const MIGRATION_004: &str = include_str!("../migrations/004_indexes.sql");
const MIGRATION_005: &str = include_str!("../migrations/005_task_display_id.sql");
const MIGRATION_006: &str = include_str!("../migrations/006_user_edited.sql");
const MIGRATION_007: &str = include_str!("../migrations/007_task_display_id_seq.sql");
const MIGRATION_010: &str = include_str!("../migrations/010_habit_pauses.sql");
const MIGRATION_012: &str = include_str!("../migrations/012_window_mode.sql");
const MIGRATION_013: &str = include_str!("../migrations/013_habit_task_display_id.sql");
const MIGRATION_014: &str = include_str!("../migrations/014_workload.sql");
const MIGRATION_015: &str = include_str!("../migrations/015_skills.sql");
// Migration 013 one-time backfill: drops the old global unique index, renumbers
// existing habit tasks to start from 1 per habit, and seeds the per-habit
// sequences. Non-idempotent (DROP + UPDATE renumber) — guarded by a check
// in `init` that only runs this when habit_task_display_id_seq is empty.
const MIGRATION_013_BACKFILL: &str = "
-- Drop the old global unique index so habit tasks can use per-habit sequences.
DROP INDEX IF EXISTS idx_tasks_display_id;

-- Renumber existing habit tasks so each habit starts from 1, ordered by
-- creation time (then id as tiebreaker). This gives the clean h1#1, h1#2, ...
-- numbering instead of retaining old global-sequence values (e.g. h1#47).
UPDATE tasks SET display_id = (
    SELECT COUNT(*) + 1 FROM tasks t2
    WHERE t2.habit_id = tasks.habit_id
      AND (t2.created_at < tasks.created_at
           OR (t2.created_at = tasks.created_at AND t2.id < tasks.id))
) WHERE habit_id IS NOT NULL;

-- Initialize sequences for existing habits based on max display_id.
-- Uses MAX (not COUNT) to avoid reusing display_ids after task deletion (#186).
INSERT OR IGNORE INTO habit_task_display_id_seq (habit_id, next_id)
SELECT
    habit_id,
    COALESCE(MAX(display_id), 0) + 1
FROM tasks
WHERE habit_id IS NOT NULL
GROUP BY habit_id;
";
// Migration 011 creates the habit_steps table (idempotent — uses IF NOT EXISTS
// for both the table and the index). The `ALTER TABLE tasks ADD COLUMN
// habit_step_id` is not idempotent (SQLite has no IF NOT EXISTS for ADD
// COLUMN), so it is run conditionally in `init` below.
const MIGRATION_011_TABLE: &str = "CREATE TABLE IF NOT EXISTS habit_steps (
    id              TEXT PRIMARY KEY,
    habit_id        TEXT NOT NULL REFERENCES habits(id) ON DELETE CASCADE,
    position        INTEGER NOT NULL,
    title           TEXT NOT NULL,
    description     TEXT,
    start_time      TEXT NOT NULL,
    end_time        TEXT NOT NULL,
    avg_minutes     INTEGER NOT NULL,
    sigma_minutes   INTEGER NOT NULL DEFAULT 0,
    parallelizable  BOOLEAN NOT NULL DEFAULT 0,
    allows_parallel BOOLEAN NOT NULL DEFAULT 0,
    abandonability  REAL NOT NULL DEFAULT 0.0,
    fixed           BOOLEAN NOT NULL DEFAULT 0,
    depends_on      TEXT NOT NULL DEFAULT '[]',
    created_at      TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_habit_steps_habit ON habit_steps(habit_id);";

// Migration 009 adds habits.display_id. The ALTER TABLE is not idempotent
// (SQLite has no IF NOT EXISTS for ADD COLUMN), so it is run conditionally
// in `init`. The backfill, index, and sequence-table statements below are
// idempotent and run unconditionally. See `MIGRATION_009_BACKFILL`.
const MIGRATION_009_BACKFILL: &str = "
-- Backfill existing rows with sequential numbers ordered by creation time.
UPDATE habits SET display_id = (
    SELECT COUNT(*) + 1 FROM habits h2
    WHERE h2.created_at < habits.created_at
       OR (h2.created_at = habits.created_at AND h2.id < habits.id)
) WHERE display_id = 0;

-- Unique only for real (non-zero) display_ids.
CREATE UNIQUE INDEX IF NOT EXISTS idx_habits_display_id ON habits(display_id) WHERE display_id != 0;

-- Monotonic display_id sequence — prevents reuse after habit deletion.
CREATE TABLE IF NOT EXISTS habit_display_id_seq (
    next_id INTEGER NOT NULL
);

-- Initialize from the current maximum display_id (or 1 if no habits exist).
INSERT INTO habit_display_id_seq (next_id)
SELECT COALESCE(MAX(display_id), 0) + 1 FROM habits
WHERE (SELECT COUNT(*) FROM habit_display_id_seq) = 0;
";

pub struct SqliteStorage {
    pool: SqlitePool,
}

impl SqliteStorage {
    pub async fn init(cfg: &LocalConfig) -> Result<Self, Box<dyn std::error::Error>> {
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

        // Migration 009 adds habits.display_id (not idempotent — SQLite has no
        // IF NOT EXISTS for ADD COLUMN). The backfill, index, and sequence
        // table statements in 009 are idempotent, but the ALTER is not, so we
        // split the migration: run the ALTER only when the column is missing,
        // then run the rest of 009 unconditionally.
        let has_habits_display_id: bool = sqlx::query_scalar(
            "SELECT COUNT(*) > 0 FROM pragma_table_info('habits') WHERE name = 'display_id'",
        )
        .fetch_one(&pool)
        .await?;
        if !has_habits_display_id {
            sqlx::query("ALTER TABLE habits ADD COLUMN display_id INTEGER NOT NULL DEFAULT 0")
                .execute(&pool)
                .await?;
        }
        // Backfill + index + sequence table (idempotent statements).
        sqlx::raw_sql(MIGRATION_009_BACKFILL).execute(&pool).await?;

        // Migration 010 creates the habit_pauses table (idempotent — uses
        // IF NOT EXISTS for both the table and the index).
        sqlx::raw_sql(MIGRATION_010).execute(&pool).await?;

        // Migration 011 creates the habit_steps table (idempotent) and adds
        // tasks.habit_step_id (not idempotent — guarded by a column check).
        sqlx::raw_sql(MIGRATION_011_TABLE).execute(&pool).await?;
        let has_habit_step_id: bool = sqlx::query_scalar(
            "SELECT COUNT(*) > 0 FROM pragma_table_info('tasks') WHERE name = 'habit_step_id'",
        )
        .fetch_one(&pool)
        .await?;
        if !has_habit_step_id {
            sqlx::query("ALTER TABLE tasks ADD COLUMN habit_step_id TEXT")
                .execute(&pool)
                .await?;
        }

        // Migration 012 adds habits.window_mode (not idempotent — SQLite has
        // no IF NOT EXISTS for ADD COLUMN). The column defaults to 'day' so
        // existing habits keep the legacy per-day window behavior.
        let has_window_mode: bool = sqlx::query_scalar(
            "SELECT COUNT(*) > 0 FROM pragma_table_info('habits') WHERE name = 'window_mode'",
        )
        .fetch_one(&pool)
        .await?;
        if !has_window_mode {
            sqlx::raw_sql(MIGRATION_012).execute(&pool).await?;
        }

        // Migration 013 creates habit_task_display_id_seq table and scoped
        // indexes (idempotent). The one-time backfill (drop old index, renumber
        // habit tasks, seed sequences) is non-idempotent and guarded by a
        // check: only run when the seq table exists but has no rows.
        sqlx::raw_sql(MIGRATION_013).execute(&pool).await?;
        let seq_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM habit_task_display_id_seq")
            .fetch_one(&pool)
            .await?;
        if seq_count == 0 {
            sqlx::raw_sql(MIGRATION_013_BACKFILL).execute(&pool).await?;
        }

        // Migration 014 adds workload columns to settings (idempotent).
        let has_workload: bool = sqlx::query_scalar(
            "SELECT COUNT(*) FROM pragma_table_info('settings') WHERE name = 'comfortable_minutes'",
        )
        .fetch_one(&pool)
        .await?;
        if !has_workload {
            sqlx::raw_sql(MIGRATION_014).execute(&pool).await?;
        }

        // Migration 015 creates the skills table (idempotent).
        sqlx::raw_sql(MIGRATION_015).execute(&pool).await?;

        Ok(Self { pool })
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
    /// DB 内のトークンは SHA-256 でハッシュ化して保存、比較は hash vs hash。
    async fn verify_token(&self, token: &str) -> StorageResult<bool> {
        let hash = crate::auth::hash_token(token);
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
        if let Some(ref v) = query.ical_uid {
            sql.push_str(" AND ical_uid = ?");
            bindings.push(v.clone());
        }
        sql.push_str(" ORDER BY created_at DESC");
        let mut q = sqlx::query_as::<_, TaskRow>(sqlx::AssertSqlSafe(sql.as_str()));
        for b in &bindings {
            q = q.bind(b);
        }
        q.fetch_all(&self.pool).await.map_err(map_err)
    }

    async fn task_exists_by_ical_uid(&self, uid: &str) -> StorageResult<bool> {
        let exists: Option<i64> =
            sqlx::query_scalar("SELECT 1 FROM tasks WHERE ical_uid = ? LIMIT 1")
                .bind(uid)
                .fetch_optional(&self.pool)
                .await
                .map_err(map_err)?;
        Ok(exists.is_some())
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
        // For habit tasks, use a habit-specific sequence (#380).
        let display_id: i64 = if let Some(ref habit_id) = body.habit_id {
            // Use habit-specific sequence. Ensure the sequence entry exists first.
            sqlx::query(
                "INSERT OR IGNORE INTO habit_task_display_id_seq (habit_id, next_id) VALUES (?1, 1)",
            )
            .bind(habit_id)
            .execute(&self.pool)
            .await
            .map_err(map_err)?;
            sqlx::query_scalar(
                "UPDATE habit_task_display_id_seq SET next_id = next_id + 1 WHERE habit_id = ?1 RETURNING next_id - 1",
            )
            .bind(habit_id)
            .fetch_one(&self.pool)
            .await
            .map_err(map_err)?
        } else {
            // Use global task sequence
            sqlx::query_scalar(
                "UPDATE task_display_id_seq SET next_id = next_id + 1 RETURNING next_id - 1",
            )
            .fetch_one(&self.pool)
            .await
            .map_err(map_err)?
        };
        sqlx::query(
            "INSERT INTO tasks (id, display_id, title, description, start_at, end_at, avg_minutes, sigma_minutes, depends, parallelizable, allows_parallel, abandonability, status, ical_uid, habit_id, fixed, habit_step_id) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 'pending', ?, ?, ?, ?)"
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
        .bind(&body.habit_step_id)
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
            "UPDATE tasks SET title=COALESCE(?,title), description=COALESCE(?,description), start_at=COALESCE(?,start_at), end_at=COALESCE(?,end_at), avg_minutes=COALESCE(?,avg_minutes), sigma_minutes=COALESCE(?,sigma_minutes), depends=COALESCE(?,depends), parallelizable=COALESCE(?,parallelizable), allows_parallel=COALESCE(?,allows_parallel), abandonability=COALESCE(?,abandonability), status=?, habit_id=COALESCE(?,habit_id), user_edited=COALESCE(?,user_edited), fixed=COALESCE(?,fixed), habit_step_id=COALESCE(?,habit_step_id), updated_at=datetime('now') WHERE id = ?"
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
        .bind(body.habit_step_id.as_ref())
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
            "UPDATE tasks SET title=?, description=?, start_at=?, end_at=?, avg_minutes=?, sigma_minutes=?, depends=?, parallelizable=?, allows_parallel=?, abandonability=?, habit_id=COALESCE(?,habit_id), fixed=?, habit_step_id=?, updated_at=datetime('now') WHERE id = ?"
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
        .bind(&body.habit_step_id)
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
        let full = resolve_habit_id(&self.pool, id).await?;
        sqlx::query_as::<_, HabitRow>("SELECT * FROM habits WHERE id = ?")
            .bind(&full)
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
        let window_mode = body.window_mode.as_deref().unwrap_or("day");
        // Atomically reserve a monotonic display_id from the sequence table
        // (mirrors tasks.display_id, issue #186 / #305).
        let display_id: i64 = sqlx::query_scalar(
            "UPDATE habit_display_id_seq SET next_id = next_id + 1 RETURNING next_id - 1",
        )
        .fetch_one(&self.pool)
        .await
        .map_err(map_err)?;
        sqlx::query(
            "INSERT INTO habits (id, display_id, title, description, recurrence, start_time, end_time, avg_minutes, sigma_minutes, parallelizable, allows_parallel, abandonability, active, fixed, window_mode) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 1, ?, ?)"
        )
        .bind(&id)
        .bind(display_id)
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
        .bind(window_mode)
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
        let full = resolve_habit_id(&self.pool, id).await?;
        sqlx::query(
            "UPDATE habits SET title=COALESCE(?,title), description=COALESCE(?,description), recurrence=COALESCE(?,recurrence), start_time=COALESCE(?,start_time), end_time=COALESCE(?,end_time), avg_minutes=COALESCE(?,avg_minutes), sigma_minutes=COALESCE(?,sigma_minutes), parallelizable=COALESCE(?,parallelizable), allows_parallel=COALESCE(?,allows_parallel), abandonability=COALESCE(?,abandonability), active=COALESCE(?,active), fixed=COALESCE(?,fixed), window_mode=COALESCE(?,window_mode), updated_at=datetime('now') WHERE id = ?"
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
        .bind(body.window_mode.as_ref())
        .bind(&full)
        .execute(&self.pool)
        .await
        .map_err(map_err)?;
        sqlx::query_as::<_, HabitRow>("SELECT * FROM habits WHERE id = ?")
            .bind(&full)
            .fetch_one(&self.pool)
            .await
            .map_err(|e| match e {
                sqlx::Error::RowNotFound => StorageError::NotFound(format!("habit {id} not found")),
                other => StorageError::Internal(other.to_string()),
            })
    }

    async fn replace_habit(&self, id: &str, body: &CreateHabit) -> StorageResult<HabitRow> {
        let full = resolve_habit_id(&self.pool, id).await?;
        let sigma = body.sigma_minutes.unwrap_or((body.avg_minutes / 5).max(1));
        let parallelizable = body.parallelizable.unwrap_or(false);
        let allows_parallel = body.allows_parallel.unwrap_or(false);
        let abandonability = body.abandonability.unwrap_or(0.5);
        let fixed = body.fixed.unwrap_or(false);
        let window_mode = body.window_mode.as_deref().unwrap_or("day");
        sqlx::query(
            "UPDATE habits SET title=?, description=?, recurrence=?, start_time=?, end_time=?, avg_minutes=?, sigma_minutes=?, parallelizable=?, allows_parallel=?, abandonability=?, fixed=?, window_mode=?, updated_at=datetime('now') WHERE id = ?"
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
        .bind(window_mode)
        .bind(&full)
        .execute(&self.pool)
        .await
        .map_err(map_err)?;
        sqlx::query_as::<_, HabitRow>("SELECT * FROM habits WHERE id = ?")
            .bind(&full)
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
        let full = resolve_habit_id(&self.pool, id).await?;
        let mut tx = self.pool.begin().await.map_err(map_err)?;
        sqlx::query("DELETE FROM google_cal_events WHERE task_id IN (SELECT id FROM tasks WHERE habit_id = ?)")
            .bind(&full)
            .execute(&mut *tx)
            .await
            .map_err(map_err)?;
        sqlx::query("DELETE FROM tasks WHERE habit_id = ?")
            .bind(&full)
            .execute(&mut *tx)
            .await
            .map_err(map_err)?;
        // habit_pauses: same reason as above — FK cascade does not fire
        // without PRAGMA foreign_keys = ON, so delete explicitly (#303).
        sqlx::query("DELETE FROM habit_pauses WHERE habit_id = ?")
            .bind(&full)
            .execute(&mut *tx)
            .await
            .map_err(map_err)?;
        // habit_steps: same reason (#95). Tasks referencing the habit were
        // already deleted above, so the habit_step_id FK is no longer
        // referenced.
        sqlx::query("DELETE FROM habit_steps WHERE habit_id = ?")
            .bind(&full)
            .execute(&mut *tx)
            .await
            .map_err(map_err)?;
        // habit_task_display_id_seq: clean up the per-habit sequence (#380).
        sqlx::query("DELETE FROM habit_task_display_id_seq WHERE habit_id = ?")
            .bind(&full)
            .execute(&mut *tx)
            .await
            .map_err(map_err)?;
        sqlx::query("DELETE FROM habits WHERE id = ?")
            .bind(&full)
            .execute(&mut *tx)
            .await
            .map_err(map_err)?;
        tx.commit().await.map_err(map_err)?;
        Ok(())
    }

    async fn list_habit_pauses(&self, habit_id: &str) -> StorageResult<Vec<HabitPauseRow>> {
        let full = resolve_habit_id(&self.pool, habit_id).await?;
        sqlx::query_as::<_, HabitPauseRow>(
            "SELECT * FROM habit_pauses WHERE habit_id = ? ORDER BY start_date ASC, created_at ASC",
        )
        .bind(&full)
        .fetch_all(&self.pool)
        .await
        .map_err(map_err)
    }

    async fn list_all_habit_pauses(&self) -> StorageResult<Vec<HabitPauseRow>> {
        sqlx::query_as::<_, HabitPauseRow>(
            "SELECT * FROM habit_pauses ORDER BY habit_id, start_date ASC, created_at ASC",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(map_err)
    }

    async fn create_habit_pause(
        &self,
        habit_id: &str,
        body: &CreateHabitPause,
    ) -> StorageResult<HabitPauseRow> {
        validate_pause_dates(&body.start_date, &body.end_date)?;
        let full = resolve_habit_id(&self.pool, habit_id).await?;
        let id = uuid::Uuid::now_v7().to_string();
        let now = jiff::Timestamp::now().to_string();
        sqlx::query(
            "INSERT INTO habit_pauses (id, habit_id, start_date, end_date, reason, created_at) VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(&id)
        .bind(&full)
        .bind(&body.start_date)
        .bind(&body.end_date)
        .bind(body.reason.as_deref())
        .bind(&now)
        .execute(&self.pool)
        .await
        .map_err(map_err)?;
        sqlx::query_as::<_, HabitPauseRow>("SELECT * FROM habit_pauses WHERE id = ?")
            .bind(&id)
            .fetch_one(&self.pool)
            .await
            .map_err(map_err)
    }

    async fn delete_habit_pause(&self, habit_id: &str, pause_id: &str) -> StorageResult<()> {
        let full = resolve_habit_id(&self.pool, habit_id).await?;
        let result = sqlx::query("DELETE FROM habit_pauses WHERE id = ? AND habit_id = ?")
            .bind(pause_id)
            .bind(&full)
            .execute(&self.pool)
            .await
            .map_err(map_err)?;
        if result.rows_affected() == 0 {
            return Err(StorageError::NotFound(format!(
                "pause {pause_id} not found for habit {habit_id}"
            )));
        }
        Ok(())
    }

    // ── Habit steps (#95) ─────────────────────────────────────────────────

    async fn list_habit_steps(&self, habit_id: &str) -> StorageResult<Vec<HabitStepRow>> {
        let full = resolve_habit_id(&self.pool, habit_id).await?;
        sqlx::query_as::<_, HabitStepRow>(
            "SELECT * FROM habit_steps WHERE habit_id = ? ORDER BY position ASC, created_at ASC",
        )
        .bind(&full)
        .fetch_all(&self.pool)
        .await
        .map_err(map_err)
    }

    async fn list_all_habit_steps(&self) -> StorageResult<Vec<HabitStepRow>> {
        sqlx::query_as::<_, HabitStepRow>(
            "SELECT * FROM habit_steps ORDER BY habit_id, position ASC, created_at ASC",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(map_err)
    }

    async fn replace_habit_steps(
        &self,
        habit_id: &str,
        steps: &[HabitStepInput],
    ) -> StorageResult<Vec<HabitStepRow>> {
        let full = resolve_habit_id(&self.pool, habit_id).await?;
        let mut tx = self.pool.begin().await.map_err(map_err)?;

        // Fetch existing step ids for this habit.
        let existing_ids: Vec<String> =
            sqlx::query_scalar("SELECT id FROM habit_steps WHERE habit_id = ?")
                .bind(&full)
                .fetch_all(&mut *tx)
                .await
                .map_err(map_err)?;
        let existing_set: std::collections::HashSet<&String> = existing_ids.iter().collect();

        // Track ids present in the input so we can delete the rest.
        let mut input_ids: std::collections::HashSet<String> = std::collections::HashSet::new();
        let now = jiff::Timestamp::now().to_string();

        for s in steps {
            let id =
                s.id.clone()
                    .unwrap_or_else(|| uuid::Uuid::now_v7().to_string());
            input_ids.insert(id.clone());
            let sigma = s.sigma_minutes.unwrap_or((s.avg_minutes / 5).max(1));
            let parallelizable = s.parallelizable.unwrap_or(false);
            let allows_parallel = s.allows_parallel.unwrap_or(false);
            let abandonability = s.abandonability.unwrap_or(0.5);
            let fixed = s.fixed.unwrap_or(false);
            let depends_json =
                serde_json::to_string(&s.depends_on).unwrap_or_else(|_| "[]".to_string());

            if existing_set.contains(&id) {
                sqlx::query(
                    "UPDATE habit_steps SET position=?, title=?, description=?, start_time=?, end_time=?, avg_minutes=?, sigma_minutes=?, parallelizable=?, allows_parallel=?, abandonability=?, fixed=?, depends_on=? WHERE id = ? AND habit_id = ?",
                )
                .bind(s.position)
                .bind(&s.title)
                .bind(s.description.as_ref())
                .bind(&s.start_time)
                .bind(&s.end_time)
                .bind(s.avg_minutes)
                .bind(sigma)
                .bind(parallelizable)
                .bind(allows_parallel)
                .bind(abandonability)
                .bind(fixed)
                .bind(&depends_json)
                .bind(&id)
                .bind(&full)
                .execute(&mut *tx)
                .await
                .map_err(map_err)?;
            } else {
                sqlx::query(
                    "INSERT INTO habit_steps (id, habit_id, position, title, description, start_time, end_time, avg_minutes, sigma_minutes, parallelizable, allows_parallel, abandonability, fixed, depends_on, created_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
                )
                .bind(&id)
                .bind(&full)
                .bind(s.position)
                .bind(&s.title)
                .bind(s.description.as_ref())
                .bind(&s.start_time)
                .bind(&s.end_time)
                .bind(s.avg_minutes)
                .bind(sigma)
                .bind(parallelizable)
                .bind(allows_parallel)
                .bind(abandonability)
                .bind(fixed)
                .bind(&depends_json)
                .bind(&now)
                .execute(&mut *tx)
                .await
                .map_err(map_err)?;
            }
        }

        // Delete existing steps not present in the input. Tasks referencing
        // them (via habit_step_id) are left in place; sync_habit_tasks cleans
        // them up on the next sync (pending + unedited only).
        for old_id in &existing_ids {
            if !input_ids.contains(old_id) {
                sqlx::query("DELETE FROM habit_steps WHERE id = ? AND habit_id = ?")
                    .bind(old_id)
                    .bind(&full)
                    .execute(&mut *tx)
                    .await
                    .map_err(map_err)?;
            }
        }

        tx.commit().await.map_err(map_err)?;

        sqlx::query_as::<_, HabitStepRow>(
            "SELECT * FROM habit_steps WHERE habit_id = ? ORDER BY position ASC, created_at ASC",
        )
        .bind(&full)
        .fetch_all(&self.pool)
        .await
        .map_err(map_err)
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
        let comfortable_minutes = body.comfortable_minutes.or(existing.comfortable_minutes);
        let maximum_minutes = body.maximum_minutes.or(existing.maximum_minutes);
        sqlx::query(
            "UPDATE settings SET tz = ?, sleep_start = ?, sleep_end = ?, comfortable_minutes = ?, maximum_minutes = ?, updated_at = datetime('now') WHERE id = 'active'",
        )
        .bind(&tz)
        .bind(&sleep_start)
        .bind(&sleep_end)
        .bind(comfortable_minutes)
        .bind(maximum_minutes)
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

    async fn list_skills(&self) -> StorageResult<Vec<SkillRow>> {
        sqlx::query_as::<_, SkillRow>("SELECT slug, name, description, body, built_in, created_at, updated_at FROM skills ORDER BY created_at DESC")
            .fetch_all(&self.pool)
            .await
            .map_err(map_err)
    }

    async fn get_skill(&self, slug: &str) -> StorageResult<SkillRow> {
        sqlx::query_as::<_, SkillRow>("SELECT slug, name, description, body, built_in, created_at, updated_at FROM skills WHERE slug = ?")
            .bind(slug)
            .fetch_one(&self.pool)
            .await
            .map_err(|e| match e {
                sqlx::Error::RowNotFound => StorageError::NotFound(format!("skill {slug} not found")),
                other => StorageError::Internal(other.to_string()),
            })
    }

    async fn create_skill(&self, body: &CreateSkill) -> StorageResult<SkillRow> {
        let built_in = body.built_in.unwrap_or(false);
        sqlx::query(
            "INSERT INTO skills (slug, name, description, body, built_in) VALUES (?, ?, ?, ?, ?)",
        )
        .bind(&body.slug)
        .bind(&body.name)
        .bind(&body.description)
        .bind(&body.body)
        .bind(built_in)
        .execute(&self.pool)
        .await
        .map_err(map_err)?;
        self.get_skill(&body.slug).await
    }

    async fn update_skill(&self, slug: &str, body: &UpdateSkill) -> StorageResult<SkillRow> {
        sqlx::query(
            "UPDATE skills SET name=COALESCE(?,name), description=COALESCE(?,description), body=COALESCE(?,body), updated_at=datetime('now') WHERE slug = ?"
        )
        .bind(body.name.as_ref())
        .bind(body.description.as_ref())
        .bind(body.body.as_ref())
        .bind(slug)
        .execute(&self.pool)
        .await
        .map_err(map_err)?;
        self.get_skill(slug).await
    }

    async fn delete_skill(&self, slug: &str) -> StorageResult<()> {
        sqlx::query("DELETE FROM skills WHERE slug = ?")
            .bind(slug)
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
    // `h{habit_display_id}#{task_display_id}` → habit task lookup (#380).
    if let Some(rest) = id.strip_prefix(['h', 'H'])
        && let Some((hdisp, tdisp)) = rest.split_once('#')
        && let (Ok(hnum), Ok(tnum)) = (hdisp.parse::<i64>(), tdisp.parse::<i64>())
    {
        return sqlx::query_scalar::<_, String>(
            "SELECT t.id FROM tasks t JOIN habits h ON t.habit_id = h.id \
             WHERE h.display_id = ? AND t.display_id = ?",
        )
        .bind(hnum)
        .bind(tnum)
        .fetch_optional(pool)
        .await
        .map_err(map_err)?
        .ok_or_else(|| StorageError::NotFound(format!("task {id} not found")));
    }
    // Numeric input → display_id lookup for non-habit tasks only (#380).
    if let Ok(num) = id.parse::<i64>() {
        return sqlx::query_scalar::<_, String>(
            "SELECT id FROM tasks WHERE display_id = ? AND habit_id IS NULL",
        )
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

/// Resolve a habit reference to its full UUID.
/// Accepts `h<N>` (habit display_id, e.g. `h1`), a full UUID, or a UUID prefix.
async fn resolve_habit_id(pool: &SqlitePool, id: &str) -> StorageResult<String> {
    // `h<N>` → habit display_id lookup (#305).
    if let Some(rest) = id.strip_prefix(['h', 'H'])
        && let Ok(num) = rest.parse::<i64>()
    {
        return sqlx::query_scalar::<_, String>("SELECT id FROM habits WHERE display_id = ?")
            .bind(num)
            .fetch_optional(pool)
            .await
            .map_err(map_err)?
            .ok_or_else(|| StorageError::NotFound(format!("habit {id} not found")));
    }
    if id.contains('-') {
        let exists: bool = sqlx::query_scalar("SELECT COUNT(*) > 0 FROM habits WHERE id = ?")
            .bind(id)
            .fetch_one(pool)
            .await
            .map_err(map_err)?;
        if exists {
            return Ok(id.to_string());
        }
    } else {
        let matches: Vec<String> =
            sqlx::query_scalar("SELECT id FROM habits WHERE id LIKE ? || '%'")
                .bind(id)
                .fetch_all(pool)
                .await
                .map_err(map_err)?;
        match matches.len() {
            0 => {}
            1 => return Ok(matches.into_iter().next().unwrap()),
            _ => {
                return Err(StorageError::BadRequest(format!(
                    "ambiguous habit id prefix: {id}"
                )));
            }
        }
    }
    Err(StorageError::NotFound(format!("habit {id} not found")))
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

/// Validate that `start` and `end` are real `YYYY-MM-DD` calendar dates and
/// that `start <= end`. Mirrors the worker-side `validate_pause_dates`.
fn validate_pause_dates(start: &str, end: &str) -> Result<(), StorageError> {
    let s = parse_calendar_date(start)
        .ok_or_else(|| StorageError::BadRequest(format!("invalid start_date: {start}")))?;
    let e = parse_calendar_date(end)
        .ok_or_else(|| StorageError::BadRequest(format!("invalid end_date: {end}")))?;
    if s > e {
        return Err(StorageError::BadRequest(format!(
            "start_date ({start}) must be <= end_date ({end})"
        )));
    }
    Ok(())
}

/// Parse a `YYYY-MM-DD` string into a `(year, month, day)` tuple if it is a
/// real calendar date, else `None`.
///
/// Enforces zero-padded fields (4-digit year, 2-digit month/day) so that
/// lexicographic comparison against `jiff`'s zero-padded `Date::to_string()`
/// works correctly during pause matching (#303).
fn parse_calendar_date(s: &str) -> Option<(i64, u32, u32)> {
    let parts: Vec<&str> = s.split('-').collect();
    if parts.len() != 3 {
        return None;
    }
    if parts[0].len() != 4 || parts[1].len() != 2 || parts[2].len() != 2 {
        return None;
    }
    let y: i64 = parts[0].parse().ok()?;
    let m: u32 = parts[1].parse().ok()?;
    let d: u32 = parts[2].parse().ok()?;
    if !(1..=12).contains(&m) {
        return None;
    }
    let leap = (y % 4 == 0 && y % 100 != 0) || y % 400 == 0;
    let max_day = match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap => 29,
        2 => 28,
        _ => return None,
    };
    if !(1..=max_day).contains(&d) {
        return None;
    }
    Some((y, m, d))
}
