use async_trait::async_trait;
use sqlx::SqlitePool;
use sqlx::sqlite::SqlitePoolOptions;
use takusu_storage::{
    CreateHabit, CreateHabitScheduledSpan, CreateMemory, CreateSkill, CreateTask,
    GoogleCalEventRow, GoogleCalSettingsRow, HabitRow, HabitScheduledSpanRow, HabitStepInput,
    HabitStepRow, MemoryQuery, MemoryRow, ProgressEventRow, ProgressResult, RecordProgress,
    SaveScheduleRequest, ScheduleRow, SettingsRow, SimilarTaskQuery, SimilarTaskRow, SkillRow,
    SplitResult, SplitTask, Storage, StorageError, TaskProgress, TaskQuery, TaskRow,
    TaskWorkSessionRow, TokenCreateResponse, TokenRow, UpdateGoogleCalSettings, UpdateHabit,
    UpdateMemory, UpdateSettings, UpdateSkill, UpdateTask, storage::StorageResult,
};

use crate::config::LocalConfig;

/// SQL predicate for tasks whose deadline has passed but are not finished.
const OVERDUE_SQL: &str =
    "status NOT IN ('completed', 'skipped') AND datetime(end_at) < datetime('now')";
/// SQL predicate that excludes overdue tasks (completed/skipped or end_at is now or later).
const NOT_OVERDUE_SQL: &str =
    "(status IN ('completed', 'skipped') OR datetime(end_at) >= datetime('now'))";
/// Static `SELECT ... FROM tasks` fragments for queries that require an
/// audited `&'static str` (`SqlSafeStr`) and avoid `SELECT *` brittleness.
const SELECT_TASKS: &str = "SELECT id, display_id, title, description, start_at, end_at, avg_minutes, sigma_minutes, depends, parallelizable, allows_parallel, abandonability, status, habit_id, ical_uid, user_edited, fixed, habit_step_id, quantity_total, quantity_done, quantity_unit, completed_at, split_from_task_id, original_quantity_total, created_at, updated_at FROM tasks";
const SELECT_TASK_BY_ID: &str = "SELECT id, display_id, title, description, start_at, end_at, avg_minutes, sigma_minutes, depends, parallelizable, allows_parallel, abandonability, status, habit_id, ical_uid, user_edited, fixed, habit_step_id, quantity_total, quantity_done, quantity_unit, completed_at, split_from_task_id, original_quantity_total, created_at, updated_at FROM tasks WHERE id = ?";

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
const MIGRATION_016: &str = include_str!("../migrations/016_memory.sql");
const MIGRATION_017: &str = include_str!("../migrations/017_solver.sql");
const MIGRATION_018: &str = include_str!("../migrations/018_progress.sql");
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

        // Migration 010 creates the legacy habit_pauses table (idempotent — uses
        // IF NOT EXISTS for both the table and the index).
        sqlx::raw_sql(MIGRATION_010).execute(&pool).await?;

        // Rename the legacy habit_pauses table to habit_scheduled_spans (#503).
        // This runs once per database and is idempotent: it only acts when the
        // old table exists and the new one does not.
        let has_old_habit_pauses: bool = sqlx::query_scalar(
            "SELECT COUNT(*) > 0 FROM sqlite_master WHERE type='table' AND name='habit_pauses'",
        )
        .fetch_one(&pool)
        .await?;
        let has_new_habit_scheduled_spans: bool = sqlx::query_scalar(
            "SELECT COUNT(*) > 0 FROM sqlite_master WHERE type='table' AND name='habit_scheduled_spans'",
        )
        .fetch_one(&pool)
        .await?;
        if has_old_habit_pauses && !has_new_habit_scheduled_spans {
            sqlx::query("ALTER TABLE habit_pauses RENAME TO habit_scheduled_spans")
                .execute(&pool)
                .await?;
            sqlx::query(
                "CREATE INDEX IF NOT EXISTS idx_habit_scheduled_spans_habit ON habit_scheduled_spans(habit_id)",
            )
            .execute(&pool)
            .await?;
            sqlx::query("DROP INDEX IF EXISTS idx_habit_pauses_habit")
                .execute(&pool)
                .await?;
        }

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

        // Migration 016 creates the memory tables (idempotent).
        sqlx::raw_sql(MIGRATION_016).execute(&pool).await?;

        // Migration 017 adds solver/time_budget_ms/seed/warm_start to settings
        // (idempotent, guarded by a column check).
        let has_solver: bool = sqlx::query_scalar(
            "SELECT COUNT(*) > 0 FROM pragma_table_info('settings') WHERE name = 'solver'",
        )
        .fetch_one(&pool)
        .await?;
        if !has_solver {
            sqlx::raw_sql(MIGRATION_017).execute(&pool).await?;
        }

        // Migration 018 adds progress columns and tables (idempotent, guarded
        // by a column check).
        let has_quantity_total: bool = sqlx::query_scalar(
            "SELECT COUNT(*) > 0 FROM pragma_table_info('tasks') WHERE name = 'quantity_total'",
        )
        .fetch_one(&pool)
        .await?;
        if !has_quantity_total {
            sqlx::raw_sql(MIGRATION_018).execute(&pool).await?;
        }

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
        let mut sql = format!("{SELECT_TASKS} WHERE 1=1");
        let mut bindings: Vec<String> = Vec::new();
        if let Some(ref v) = query.status {
            if v == "overdue" {
                sql.push_str(" AND ");
                sql.push_str(OVERDUE_SQL);
            } else {
                sql.push_str(" AND status = ?");
                bindings.push(v.clone());
            }
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
        if query.no_overdue == Some(true) {
            sql.push_str(" AND ");
            sql.push_str(NOT_OVERDUE_SQL);
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
        sqlx::query_as::<_, TaskRow>(SELECT_TASK_BY_ID)
            .bind(&full)
            .fetch_one(&self.pool)
            .await
            .map_err(|e| match e {
                sqlx::Error::RowNotFound => StorageError::NotFound(format!("task {id} not found")),
                other => StorageError::Internal(other.to_string()),
            })
    }

    async fn create_task(&self, body: &CreateTask) -> StorageResult<TaskRow> {
        validate_quantity(
            body.quantity_total,
            body.quantity_done,
            body.original_quantity_total,
        )?;
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
        let quantity_total = body.quantity_total;
        let quantity_done = body.quantity_done.unwrap_or(0);
        let quantity_unit = body.quantity_unit.as_deref();
        let original_quantity_total = body.original_quantity_total;
        sqlx::query(
            "INSERT INTO tasks (id, display_id, title, description, start_at, end_at, avg_minutes, sigma_minutes, depends, parallelizable, allows_parallel, abandonability, status, ical_uid, habit_id, fixed, habit_step_id, quantity_total, quantity_done, quantity_unit, completed_at, split_from_task_id, original_quantity_total) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 'pending', ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"
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
        .bind(quantity_total)
        .bind(quantity_done)
        .bind(quantity_unit)
        .bind(None::<String>)
        .bind(None::<String>)
        .bind(original_quantity_total)
        .execute(&self.pool)
        .await
        .map_err(map_err)?;
        sqlx::query_as::<_, TaskRow>(SELECT_TASK_BY_ID)
            .bind(&id)
            .fetch_one(&self.pool)
            .await
            .map_err(map_err)
    }

    async fn update_task(&self, id: &str, body: &UpdateTask) -> StorageResult<TaskRow> {
        let full = resolve_task_id(&self.pool, id).await?;
        let existing = sqlx::query_as::<_, TaskRow>(SELECT_TASK_BY_ID)
            .bind(&full)
            .fetch_one(&self.pool)
            .await
            .map_err(|e| match e {
                sqlx::Error::RowNotFound => StorageError::NotFound(format!("task {id} not found")),
                other => StorageError::Internal(other.to_string()),
            })?;

        validate_quantity(
            body.quantity_total.or(existing.quantity_total),
            body.quantity_done.or(Some(existing.quantity_done)),
            body.original_quantity_total,
        )?;

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
            "UPDATE tasks SET title=COALESCE(?,title), description=COALESCE(?,description), start_at=COALESCE(?,start_at), end_at=COALESCE(?,end_at), avg_minutes=COALESCE(?,avg_minutes), sigma_minutes=COALESCE(?,sigma_minutes), depends=COALESCE(?,depends), parallelizable=COALESCE(?,parallelizable), allows_parallel=COALESCE(?,allows_parallel), abandonability=COALESCE(?,abandonability), status=?, habit_id=COALESCE(?,habit_id), user_edited=COALESCE(?,user_edited), fixed=COALESCE(?,fixed), habit_step_id=COALESCE(?,habit_step_id), quantity_total=COALESCE(?,quantity_total), quantity_done=COALESCE(?,quantity_done), quantity_unit=COALESCE(?,quantity_unit), original_quantity_total=COALESCE(?,original_quantity_total), updated_at=datetime('now') WHERE id = ?"
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
        .bind(body.quantity_total)
        .bind(body.quantity_done)
        .bind(body.quantity_unit.as_ref())
        .bind(body.original_quantity_total)
        .bind(&full)
        .execute(&self.pool)
        .await
        .map_err(map_err)?;

        // completed_at must follow explicit status transitions: set on
        // completion, clear when leaving completed.
        if body.status.is_some() {
            let completed_at = if status == "completed" {
                existing
                    .completed_at
                    .clone()
                    .or(Some(jiff::Timestamp::now().to_string()))
            } else if existing.status == "completed" {
                None
            } else {
                existing.completed_at.clone()
            };
            sqlx::query("UPDATE tasks SET completed_at = ? WHERE id = ?")
                .bind(&completed_at)
                .bind(&full)
                .execute(&self.pool)
                .await
                .map_err(map_err)?;
        }

        sqlx::query_as::<_, TaskRow>(SELECT_TASK_BY_ID)
            .bind(&full)
            .fetch_one(&self.pool)
            .await
            .map_err(map_err)
    }

    async fn replace_task(&self, id: &str, body: &CreateTask) -> StorageResult<TaskRow> {
        validate_quantity(
            body.quantity_total,
            body.quantity_done,
            body.original_quantity_total,
        )?;
        let full = resolve_task_id(&self.pool, id).await?;
        let resolved_depends = resolve_depends(&self.pool, body.depends.as_deref()).await?;
        let depends_json = serde_json::to_string(&resolved_depends).unwrap_or_else(|_| "[]".into());
        let sigma = body.sigma_minutes.unwrap_or((body.avg_minutes / 5).max(1));
        let parallelizable = body.parallelizable.unwrap_or(false);
        let allows_parallel = body.allows_parallel.unwrap_or(false);
        let abandonability = body.abandonability.unwrap_or(0.5);
        let fixed = body.fixed.unwrap_or(false);
        let quantity_total = body.quantity_total;
        let quantity_done = body.quantity_done;
        let quantity_unit = body.quantity_unit.as_deref();
        let original_quantity_total = body.original_quantity_total;
        sqlx::query(
            "UPDATE tasks SET title=?, description=?, start_at=?, end_at=?, avg_minutes=?, sigma_minutes=?, depends=?, parallelizable=?, allows_parallel=?, abandonability=?, habit_id=COALESCE(?,habit_id), fixed=?, habit_step_id=?, quantity_total=COALESCE(?, quantity_total), quantity_done=COALESCE(?, quantity_done), quantity_unit=COALESCE(?, quantity_unit), completed_at=COALESCE(?, completed_at), split_from_task_id=COALESCE(?, split_from_task_id), original_quantity_total=COALESCE(?, original_quantity_total), updated_at=datetime('now') WHERE id = ?"
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
        .bind(quantity_total)
        .bind(quantity_done)
        .bind(quantity_unit)
        .bind(None::<String>)
        .bind(None::<String>)
        .bind(original_quantity_total)
        .bind(&full)
        .execute(&self.pool)
        .await
        .map_err(map_err)?;
        sqlx::query_as::<_, TaskRow>(SELECT_TASK_BY_ID)
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
        // habit_scheduled_spans: same reason as above — FK cascade does not fire
        // without PRAGMA foreign_keys = ON, so delete explicitly (#303).
        sqlx::query("DELETE FROM habit_scheduled_spans WHERE habit_id = ?")
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

    async fn list_habit_scheduled_spans(
        &self,
        habit_id: &str,
    ) -> StorageResult<Vec<HabitScheduledSpanRow>> {
        let full = resolve_habit_id(&self.pool, habit_id).await?;
        sqlx::query_as::<_, HabitScheduledSpanRow>(
            "SELECT * FROM habit_scheduled_spans WHERE habit_id = ? ORDER BY start_date ASC, created_at ASC",
        )
        .bind(&full)
        .fetch_all(&self.pool)
        .await
        .map_err(map_err)
    }

    async fn list_all_habit_scheduled_spans(&self) -> StorageResult<Vec<HabitScheduledSpanRow>> {
        sqlx::query_as::<_, HabitScheduledSpanRow>(
            "SELECT * FROM habit_scheduled_spans ORDER BY habit_id, start_date ASC, created_at ASC",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(map_err)
    }

    async fn create_habit_scheduled_span(
        &self,
        habit_id: &str,
        body: &CreateHabitScheduledSpan,
    ) -> StorageResult<HabitScheduledSpanRow> {
        validate_scheduled_span_dates(&body.start_date, &body.end_date)?;
        let full = resolve_habit_id(&self.pool, habit_id).await?;
        let id = uuid::Uuid::now_v7().to_string();
        let now = jiff::Timestamp::now().to_string();
        sqlx::query(
            "INSERT INTO habit_scheduled_spans (id, habit_id, start_date, end_date, reason, created_at) VALUES (?, ?, ?, ?, ?, ?)",
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
        sqlx::query_as::<_, HabitScheduledSpanRow>(
            "SELECT * FROM habit_scheduled_spans WHERE id = ?",
        )
        .bind(&id)
        .fetch_one(&self.pool)
        .await
        .map_err(map_err)
    }

    async fn delete_habit_scheduled_span(
        &self,
        habit_id: &str,
        span_id: &str,
    ) -> StorageResult<()> {
        let full = resolve_habit_id(&self.pool, habit_id).await?;
        let result = sqlx::query("DELETE FROM habit_scheduled_spans WHERE id = ? AND habit_id = ?")
            .bind(span_id)
            .bind(&full)
            .execute(&self.pool)
            .await
            .map_err(map_err)?;
        if result.rows_affected() == 0 {
            return Err(StorageError::NotFound(format!(
                "scheduled span {span_id} not found for habit {habit_id}"
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
        let solver = body.solver.clone().unwrap_or(existing.solver);
        let time_budget_ms = body.time_budget_ms.or(existing.time_budget_ms);
        let seed = body.seed.or(existing.seed);
        let warm_start = body.warm_start.unwrap_or(existing.warm_start);
        sqlx::query(
            "UPDATE settings SET tz = ?, sleep_start = ?, sleep_end = ?, comfortable_minutes = ?, maximum_minutes = ?, solver = ?, time_budget_ms = ?, seed = ?, warm_start = ?, updated_at = datetime('now') WHERE id = 'active'",
        )
        .bind(&tz)
        .bind(&sleep_start)
        .bind(&sleep_end)
        .bind(comfortable_minutes)
        .bind(maximum_minutes)
        .bind(&solver)
        .bind(time_budget_ms)
        .bind(seed)
        .bind(warm_start)
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

    async fn get_memory(&self, id: &str) -> StorageResult<MemoryRow> {
        sqlx::query_as::<_, MemoryRow>("SELECT * FROM memories WHERE id = ?")
            .bind(id)
            .fetch_one(&self.pool)
            .await
            .map_err(|e| match e {
                sqlx::Error::RowNotFound => {
                    StorageError::NotFound(format!("memory {id} not found"))
                }
                other => StorageError::Internal(other.to_string()),
            })
    }

    async fn create_memory(
        &self,
        body: &CreateMemory,
        operation_id: Option<&str>,
    ) -> StorageResult<MemoryRow> {
        let normalized_key = takusu_util::memory::normalize_key(&body.key)
            .map_err(|e| StorageError::BadRequest(format!("invalid key: {e}")))?;
        let normalized_content = takusu_util::memory::normalize_content(&body.content)
            .map_err(|e| StorageError::BadRequest(format!("invalid content: {e}")))?;
        let subject_type = body.subject_type.clone().unwrap_or_default();
        let subject_id = body.subject_id.clone().unwrap_or_default();

        let mut tx = self.pool.begin().await.map_err(map_err)?;
        let payload = serde_json::to_string(body).unwrap_or_default();
        let hash = memory_request_hash(&payload, operation_id);
        if let Some(op_id) = operation_id
            && let Some(stored) = Self::check_idempotency(&mut *tx, op_id, &hash).await?
        {
            return stored;
        }

        let existing: Option<MemoryRow> = sqlx::query_as::<_, MemoryRow>(
            "SELECT * FROM memories WHERE kind = ? AND normalized_key = ? AND subject_type = ? AND subject_id = ?",
        )
        .bind(&body.kind)
        .bind(&normalized_key)
        .bind(&subject_type)
        .bind(&subject_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(map_err)?;

        if let Some(existing) = existing {
            if body.upsert {
                let id = existing.id;
                let new_revision = existing.revision + 1;
                let result = sqlx::query(
                    "UPDATE memories SET content = ?, normalized_content = ?, revision = ?, updated_at = datetime('now') WHERE id = ? AND revision = ?",
                )
                .bind(&body.content)
                .bind(&normalized_content)
                .bind(new_revision)
                .bind(&id)
                .bind(existing.revision)
                .execute(&mut *tx)
                .await
                .map_err(map_err)?;
                if result.rows_affected() == 0 {
                    return Err(StorageError::Conflict(
                        "memory changed after proposal".into(),
                    ));
                }
                let row: MemoryRow =
                    sqlx::query_as::<_, MemoryRow>("SELECT * FROM memories WHERE id = ?")
                        .bind(&id)
                        .fetch_one(&mut *tx)
                        .await
                        .map_err(map_err)?;
                if let Some(op_id) = operation_id {
                    Self::record_operation(&mut *tx, op_id, &hash, &row).await?;
                }
                tx.commit().await.map_err(map_err)?;
                return Ok(row);
            }
            return Err(StorageError::Conflict(format!(
                "memory {} already exists",
                body.key
            )));
        }

        let id = uuid::Uuid::now_v7().to_string();
        let source = "user_confirmed";
        sqlx::query(
            "INSERT INTO memories (id, kind, key, normalized_key, content, normalized_content, subject_type, subject_id, source, revision) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, 1)",
        )
        .bind(&id)
        .bind(&body.kind)
        .bind(&body.key)
        .bind(&normalized_key)
        .bind(&body.content)
        .bind(&normalized_content)
        .bind(&subject_type)
        .bind(&subject_id)
        .bind(source)
        .execute(&mut *tx)
        .await
        .map_err(map_err)?;

        let row: MemoryRow = sqlx::query_as::<_, MemoryRow>("SELECT * FROM memories WHERE id = ?")
            .bind(&id)
            .fetch_one(&mut *tx)
            .await
            .map_err(map_err)?;
        if let Some(op_id) = operation_id {
            Self::record_operation(&mut *tx, op_id, &hash, &row).await?;
        }
        tx.commit().await.map_err(map_err)?;
        Ok(row)
    }

    async fn update_memory(
        &self,
        id: &str,
        body: &UpdateMemory,
        operation_id: Option<&str>,
    ) -> StorageResult<MemoryRow> {
        let content = body
            .content
            .as_ref()
            .ok_or_else(|| StorageError::BadRequest("content is required".into()))?;
        let normalized_content = takusu_util::memory::normalize_content(content)
            .map_err(|e| StorageError::BadRequest(format!("invalid content: {e}")))?;

        let mut tx = self.pool.begin().await.map_err(map_err)?;
        let payload = format!(
            "update:{id}:{}:{}",
            body.observed_revision,
            body.content.as_deref().unwrap_or("")
        );
        let hash = memory_request_hash(&payload, operation_id);
        if let Some(op_id) = operation_id
            && let Some(stored) = Self::check_idempotency(&mut *tx, op_id, &hash).await?
        {
            return stored;
        }

        let existing: Option<MemoryRow> =
            sqlx::query_as::<_, MemoryRow>("SELECT * FROM memories WHERE id = ?")
                .bind(id)
                .fetch_optional(&mut *tx)
                .await
                .map_err(map_err)?;
        let existing =
            existing.ok_or_else(|| StorageError::NotFound(format!("memory {id} not found")))?;
        if existing.revision != body.observed_revision {
            return Err(StorageError::Conflict(
                "memory changed after proposal".into(),
            ));
        }
        let new_revision = existing.revision + 1;

        let result = sqlx::query(
            "UPDATE memories SET content = ?, normalized_content = ?, revision = ?, updated_at = datetime('now') WHERE id = ? AND revision = ?",
        )
        .bind(content)
        .bind(&normalized_content)
        .bind(new_revision)
        .bind(id)
        .bind(body.observed_revision)
        .execute(&mut *tx)
        .await
        .map_err(map_err)?;

        if result.rows_affected() == 0 {
            return Err(StorageError::Conflict(
                "memory changed after proposal".into(),
            ));
        }

        let row: MemoryRow = sqlx::query_as::<_, MemoryRow>("SELECT * FROM memories WHERE id = ?")
            .bind(id)
            .fetch_one(&mut *tx)
            .await
            .map_err(map_err)?;
        if let Some(op_id) = operation_id {
            Self::record_operation(&mut *tx, op_id, &hash, &row).await?;
        }
        tx.commit().await.map_err(map_err)?;
        Ok(row)
    }

    async fn delete_memory(
        &self,
        id: &str,
        observed_revision: i64,
        operation_id: Option<&str>,
    ) -> StorageResult<()> {
        let mut tx = self.pool.begin().await.map_err(map_err)?;
        let payload = format!("delete:{id}:{observed_revision}");
        let hash = memory_request_hash(&payload, operation_id);
        if let Some(op_id) = operation_id
            && let Some(stored) = Self::check_idempotency::<_, ()>(&mut *tx, op_id, &hash).await?
        {
            let _ = stored?;
            return Ok(());
        }

        let existing: Option<MemoryRow> =
            sqlx::query_as::<_, MemoryRow>("SELECT * FROM memories WHERE id = ?")
                .bind(id)
                .fetch_optional(&mut *tx)
                .await
                .map_err(map_err)?;
        let existing =
            existing.ok_or_else(|| StorageError::NotFound(format!("memory {id} not found")))?;
        if existing.revision != observed_revision {
            return Err(StorageError::Conflict(
                "memory changed after proposal".into(),
            ));
        }

        let result = sqlx::query("DELETE FROM memories WHERE id = ? AND revision = ?")
            .bind(id)
            .bind(observed_revision)
            .execute(&mut *tx)
            .await
            .map_err(map_err)?;

        if result.rows_affected() == 0 {
            return Err(StorageError::Conflict(
                "memory changed after proposal".into(),
            ));
        }

        if let Some(op_id) = operation_id {
            Self::record_operation_raw(&mut *tx, op_id, &hash, "null").await?;
        }
        tx.commit().await.map_err(map_err)?;
        Ok(())
    }

    async fn search_memories(&self, query: &MemoryQuery) -> StorageResult<Vec<MemoryRow>> {
        let q = takusu_util::memory::normalize_query(&query.q)
            .map_err(|e| StorageError::BadRequest(format!("invalid query: {e}")))?;
        let pattern = format!("%{}%", takusu_util::memory::escape_like_pattern(&q));

        let mut sql = String::from(
            "SELECT * FROM memories WHERE (normalized_key LIKE ? ESCAPE '\\' OR normalized_content LIKE ? ESCAPE '\\')",
        );
        let mut bindings: Vec<String> = vec![pattern.clone(), pattern];

        if let Some(ref kind) = query.kind {
            sql.push_str(" AND kind = ?");
            bindings.push(kind.clone());
        }
        if let Some(ref subject_type) = query.subject_type {
            sql.push_str(" AND subject_type = ?");
            bindings.push(subject_type.clone());
        }
        if let Some(ref subject_id) = query.subject_id {
            sql.push_str(" AND subject_id = ?");
            bindings.push(subject_id.clone());
        }
        sql.push_str(" LIMIT 1000");

        let mut q = sqlx::query_as::<_, MemoryRow>(sqlx::AssertSqlSafe(sql.as_str()));
        for b in &bindings {
            q = q.bind(b);
        }
        let mut rows: Vec<MemoryRow> = q.fetch_all(&self.pool).await.map_err(map_err)?;

        takusu_util::memory::sort_memories(&query.q, &mut rows);

        let limit = query.limit.map_or(10, |n| n.clamp(1, 50) as usize);
        rows.truncate(limit);
        Ok(rows)
    }

    async fn find_similar_tasks(
        &self,
        query: &SimilarTaskQuery,
    ) -> StorageResult<Vec<SimilarTaskRow>> {
        let normalized_title = takusu_util::memory::normalize_text(
            &query.title,
            Some(takusu_util::memory::MAX_QUERY_SCALARS),
        )
        .map_err(|e| StorageError::BadRequest(format!("invalid title: {e}")))?;
        let rows: Vec<SimilarTaskRow> = sqlx::query_as::<_, SimilarTaskRow>(
            "SELECT id AS task_id, display_id, title, avg_minutes, sigma_minutes, NULL AS actual_minutes, completed_at, updated_at, 'title_overlap' AS similarity FROM tasks WHERE status = 'completed' ORDER BY updated_at DESC LIMIT 1000",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(map_err)?;

        let mut scored: Vec<(f64, SimilarTaskRow)> = rows
            .into_iter()
            .filter_map(|row| {
                takusu_util::memory::similar_task_score_pre_normalized(
                    &normalized_title,
                    &row.title,
                )
                .map(|score| (score, row))
            })
            .collect();

        scored.sort_by(|(sa, a), (sb, b)| {
            sa.total_cmp(sb)
                .reverse()
                .then_with(|| {
                    takusu_util::memory::compare_optional_desc(&a.completed_at, &b.completed_at)
                })
                .then_with(|| b.updated_at.cmp(&a.updated_at))
                .then_with(|| a.task_id.cmp(&b.task_id))
        });

        let limit = query.limit.map_or(10, |n| n.clamp(1, 50)) as usize;
        let mut out: Vec<SimilarTaskRow> = scored
            .into_iter()
            .map(|(_, mut row)| {
                row.similarity = "title_overlap".to_string();
                row
            })
            .collect();
        out.truncate(limit);
        Ok(out)
    }

    async fn start_task_work(
        &self,
        id: &str,
        operation_id: Option<&str>,
    ) -> StorageResult<TaskRow> {
        let payload = serde_json::json!({"op": "start", "id": id}).to_string();
        let request_hash = progress_request_hash(&payload, operation_id);

        let mut tx = self.pool.begin().await.map_err(map_err)?;
        if let Some(op_id) = operation_id
            && let Some(stored) =
                Self::check_progress_idempotency(&mut *tx, op_id, &request_hash).await?
        {
            return stored;
        }

        let full = resolve_task_id(&mut *tx, id).await?;

        let status: String = sqlx::query_scalar("SELECT status FROM tasks WHERE id = ?")
            .bind(&full)
            .fetch_one(&mut *tx)
            .await
            .map_err(map_err)?;
        if status == "completed" || status == "skipped" {
            return Err(StorageError::BadRequest(format!(
                "cannot start work on a {status} task"
            )));
        }

        let session_id = uuid::Uuid::now_v7().to_string();
        let now = jiff::Timestamp::now().to_string();
        sqlx::query(
            "INSERT OR IGNORE INTO task_work_sessions (id, task_id, started_at) VALUES (?, ?, ?)",
        )
        .bind(&session_id)
        .bind(&full)
        .bind(&now)
        .execute(&mut *tx)
        .await
        .map_err(map_err)?;

        sqlx::query(
            "UPDATE tasks SET status = 'in_progress', updated_at = datetime('now') WHERE id = ?",
        )
        .bind(&full)
        .execute(&mut *tx)
        .await
        .map_err(map_err)?;

        let task: TaskRow = sqlx::query_as(SELECT_TASK_BY_ID)
            .bind(&full)
            .fetch_one(&mut *tx)
            .await
            .map_err(map_err)?;

        if let Some(op_id) = operation_id {
            Self::record_progress_operation(&mut *tx, op_id, &request_hash, &task).await?;
        }
        tx.commit().await.map_err(map_err)?;
        Ok(task)
    }

    async fn pause_task_work(
        &self,
        id: &str,
        operation_id: Option<&str>,
    ) -> StorageResult<TaskRow> {
        let payload = serde_json::json!({"op": "pause", "id": id}).to_string();
        let request_hash = progress_request_hash(&payload, operation_id);

        let mut tx = self.pool.begin().await.map_err(map_err)?;
        if let Some(op_id) = operation_id
            && let Some(stored) =
                Self::check_progress_idempotency(&mut *tx, op_id, &request_hash).await?
        {
            return stored;
        }

        let full = resolve_task_id(&mut *tx, id).await?;

        let status: String = sqlx::query_scalar("SELECT status FROM tasks WHERE id = ?")
            .bind(&full)
            .fetch_one(&mut *tx)
            .await
            .map_err(map_err)?;
        if status == "completed" || status == "skipped" {
            return Err(StorageError::BadRequest(format!(
                "cannot pause work on a {status} task"
            )));
        }

        let now = jiff::Timestamp::now().to_string();
        sqlx::query(
            "UPDATE task_work_sessions SET ended_at = ? WHERE task_id = ? AND ended_at IS NULL",
        )
        .bind(&now)
        .bind(&full)
        .execute(&mut *tx)
        .await
        .map_err(map_err)?;

        sqlx::query(
            "UPDATE tasks SET status = 'scheduled', updated_at = datetime('now') WHERE id = ?",
        )
        .bind(&full)
        .execute(&mut *tx)
        .await
        .map_err(map_err)?;

        let task: TaskRow = sqlx::query_as(SELECT_TASK_BY_ID)
            .bind(&full)
            .fetch_one(&mut *tx)
            .await
            .map_err(map_err)?;

        if let Some(op_id) = operation_id {
            Self::record_progress_operation(&mut *tx, op_id, &request_hash, &task).await?;
        }
        tx.commit().await.map_err(map_err)?;
        Ok(task)
    }

    async fn record_progress(
        &self,
        id: &str,
        body: &RecordProgress,
        operation_id: Option<&str>,
    ) -> StorageResult<ProgressResult> {
        if body.quantity_done < 0 {
            return Err(StorageError::BadRequest(
                "quantity_done cannot be negative".into(),
            ));
        }
        let payload = serde_json::json!({"op": "progress", "id": id, "body": body}).to_string();
        let request_hash = progress_request_hash(&payload, operation_id);

        let mut tx = self.pool.begin().await.map_err(map_err)?;
        if let Some(op_id) = operation_id
            && let Some(stored) =
                Self::check_progress_idempotency(&mut *tx, op_id, &request_hash).await?
        {
            return stored;
        }

        let full = resolve_task_id(&mut *tx, id).await?;

        let task: TaskRow = sqlx::query_as(SELECT_TASK_BY_ID)
            .bind(&full)
            .fetch_one(&mut *tx)
            .await
            .map_err(|e| match e {
                sqlx::Error::RowNotFound => StorageError::NotFound(format!("task {id} not found")),
                other => StorageError::Internal(other.to_string()),
            })?;

        if task.status == "completed" || task.status == "skipped" {
            return Err(StorageError::BadRequest(format!(
                "cannot record progress on a {} task",
                task.status
            )));
        }
        if let Some(total) = task.quantity_total
            && body.quantity_done > total
        {
            return Err(StorageError::BadRequest(format!(
                "quantity_done cannot exceed quantity_total ({} > {})",
                body.quantity_done, total
            )));
        }

        let open: Option<TaskWorkSessionRow> = sqlx::query_as(
            "SELECT id, task_id, started_at, ended_at, created_at FROM task_work_sessions WHERE task_id = ? AND ended_at IS NULL",
        )
        .bind(&full)
        .fetch_optional(&mut *tx)
        .await
        .map_err(map_err)?;

        // Increasing progress requires an open session to measure active time.
        // Corrections (decreasing or keeping quantity_done) are allowed without one.
        if open.is_none() && body.quantity_done > task.quantity_done {
            return Err(StorageError::BadRequest(
                "no open work session; start work first".into(),
            ));
        }

        let delta_quantity = body.quantity_done - task.quantity_done;

        if delta_quantity == 0 {
            let result = ProgressResult {
                task: task.clone(),
                event: None,
                suggests_completion: false,
            };
            if let Some(op_id) = operation_id {
                Self::record_progress_operation(&mut *tx, op_id, &request_hash, &result).await?;
            }
            tx.commit().await.map_err(map_err)?;
            return Ok(result);
        }

        let now = jiff::Timestamp::now().to_string();

        // Active minutes are measured from the later of the open session start
        // and the most recent progress event, so repeated progress updates in
        // the same session do not accumulate the same time.
        let last_event: Option<ProgressEventRow> = sqlx::query_as(
            "SELECT id, task_id, at, quantity_done, delta_quantity, active_minutes, note FROM progress_events WHERE task_id = ? ORDER BY id DESC LIMIT 1",
        )
        .bind(&full)
        .fetch_optional(&mut *tx)
        .await
        .map_err(map_err)?;

        let active_minutes = if let Some(ref session) = open {
            let base = if let Some(ref ev) = last_event {
                takusu_util::later_timestamp(&session.started_at, &ev.at)
            } else {
                &session.started_at
            };
            takusu_util::minutes_between(base, &now)
        } else {
            0
        };

        let event_id = uuid::Uuid::now_v7().to_string();
        sqlx::query(
            "INSERT INTO progress_events (id, task_id, at, quantity_done, delta_quantity, active_minutes, note) VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&event_id)
        .bind(&full)
        .bind(&now)
        .bind(body.quantity_done)
        .bind(delta_quantity)
        .bind(active_minutes)
        .bind(body.note.as_ref())
        .execute(&mut *tx)
        .await
        .map_err(map_err)?;

        let mut new_avg = task.avg_minutes;
        let mut new_sigma = task.sigma_minutes;
        if delta_quantity > 0 && active_minutes > 0 {
            let (avg, sigma) = compute_updated_estimate(
                &mut *tx,
                &full,
                task.avg_minutes,
                task.sigma_minutes,
                task.quantity_total,
                active_minutes,
                delta_quantity,
            )
            .await?;
            new_avg = avg;
            new_sigma = sigma;
        }

        let status = if task.status == "completed" {
            "completed".to_string()
        } else if delta_quantity < 0 {
            task.status.clone()
        } else {
            "in_progress".to_string()
        };

        let suggests_completion = task
            .quantity_total
            .map(|total| body.quantity_done >= total)
            .unwrap_or(false);

        sqlx::query(
            "UPDATE tasks SET quantity_done = ?, avg_minutes = ?, sigma_minutes = ?, status = ?, updated_at = datetime('now') WHERE id = ?",
        )
        .bind(body.quantity_done)
        .bind(new_avg)
        .bind(new_sigma)
        .bind(&status)
        .bind(&full)
        .execute(&mut *tx)
        .await
        .map_err(map_err)?;

        let event: ProgressEventRow = sqlx::query_as("SELECT id, task_id, at, quantity_done, delta_quantity, active_minutes, note FROM progress_events WHERE id = ?")
            .bind(&event_id)
            .fetch_one(&mut *tx)
            .await
            .map_err(map_err)?;
        let task: TaskRow = sqlx::query_as(SELECT_TASK_BY_ID)
            .bind(&full)
            .fetch_one(&mut *tx)
            .await
            .map_err(map_err)?;

        let result = ProgressResult {
            task,
            event: Some(event),
            suggests_completion,
        };
        if let Some(op_id) = operation_id {
            Self::record_progress_operation(&mut *tx, op_id, &request_hash, &result).await?;
        }
        tx.commit().await.map_err(map_err)?;
        Ok(result)
    }

    async fn complete_task_work(
        &self,
        id: &str,
        operation_id: Option<&str>,
    ) -> StorageResult<TaskRow> {
        let payload = serde_json::json!({"op": "complete", "id": id}).to_string();
        let request_hash = progress_request_hash(&payload, operation_id);

        let mut tx = self.pool.begin().await.map_err(map_err)?;
        if let Some(op_id) = operation_id
            && let Some(stored) =
                Self::check_progress_idempotency(&mut *tx, op_id, &request_hash).await?
        {
            return stored;
        }

        let full = resolve_task_id(&mut *tx, id).await?;

        let status: String = sqlx::query_scalar("SELECT status FROM tasks WHERE id = ?")
            .bind(&full)
            .fetch_one(&mut *tx)
            .await
            .map_err(map_err)?;
        if status == "completed" || status == "skipped" {
            return Err(StorageError::BadRequest(format!(
                "cannot complete a {status} task"
            )));
        }

        let now = jiff::Timestamp::now().to_string();
        sqlx::query(
            "UPDATE task_work_sessions SET ended_at = ? WHERE task_id = ? AND ended_at IS NULL",
        )
        .bind(&now)
        .bind(&full)
        .execute(&mut *tx)
        .await
        .map_err(map_err)?;

        let task_before: TaskRow = sqlx::query_as(SELECT_TASK_BY_ID)
            .bind(&full)
            .fetch_one(&mut *tx)
            .await
            .map_err(map_err)?;

        let sessions: Vec<TaskWorkSessionRow> = sqlx::query_as(
            "SELECT id, task_id, started_at, ended_at, created_at FROM task_work_sessions WHERE task_id = ? ORDER BY started_at ASC",
        )
        .bind(&full)
        .fetch_all(&mut *tx)
        .await
        .map_err(map_err)?;
        let total_active_minutes: i64 = sessions.iter().map(session_minutes).sum();

        let quantity_done = task_before
            .quantity_total
            .unwrap_or(task_before.quantity_done);
        let delta_quantity = quantity_done - task_before.quantity_done;

        let (new_avg, new_sigma) = if delta_quantity > 0 && total_active_minutes > 0 {
            compute_updated_estimate(
                &mut *tx,
                &full,
                task_before.avg_minutes,
                task_before.sigma_minutes,
                task_before.quantity_total,
                total_active_minutes,
                delta_quantity,
            )
            .await?
        } else if task_before.quantity_total.is_none() && total_active_minutes > 0 {
            (total_active_minutes, task_before.sigma_minutes)
        } else {
            (task_before.avg_minutes, task_before.sigma_minutes)
        };

        sqlx::query(
            "UPDATE tasks SET status = 'completed', completed_at = ?, quantity_done = ?, avg_minutes = ?, sigma_minutes = ?, updated_at = datetime('now') WHERE id = ?",
        )
        .bind(&now)
        .bind(quantity_done)
        .bind(new_avg)
        .bind(new_sigma)
        .bind(&full)
        .execute(&mut *tx)
        .await
        .map_err(map_err)?;

        if total_active_minutes > 0 {
            let event_id = uuid::Uuid::now_v7().to_string();
            sqlx::query(
                "INSERT INTO progress_events (id, task_id, at, quantity_done, delta_quantity, active_minutes, note) VALUES (?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(&event_id)
            .bind(&full)
            .bind(&now)
            .bind(quantity_done)
            .bind(delta_quantity)
            .bind(total_active_minutes)
            .bind("completed")
            .execute(&mut *tx)
            .await
            .map_err(map_err)?;
        }

        let task: TaskRow = sqlx::query_as(SELECT_TASK_BY_ID)
            .bind(&full)
            .fetch_one(&mut *tx)
            .await
            .map_err(map_err)?;

        if let Some(op_id) = operation_id {
            Self::record_progress_operation(&mut *tx, op_id, &request_hash, &task).await?;
        }
        tx.commit().await.map_err(map_err)?;
        Ok(task)
    }

    async fn get_task_progress(&self, id: &str) -> StorageResult<TaskProgress> {
        let full = resolve_task_id(&self.pool, id).await?;
        let task: TaskRow = sqlx::query_as(SELECT_TASK_BY_ID)
            .bind(&full)
            .fetch_one(&self.pool)
            .await
            .map_err(|e| match e {
                sqlx::Error::RowNotFound => StorageError::NotFound(format!("task {id} not found")),
                other => StorageError::Internal(other.to_string()),
            })?;

        let sessions: Vec<TaskWorkSessionRow> = sqlx::query_as(
            "SELECT id, task_id, started_at, ended_at, created_at FROM task_work_sessions WHERE task_id = ? ORDER BY started_at ASC",
        )
        .bind(&full)
        .fetch_all(&self.pool)
        .await
        .map_err(map_err)?;

        let events: Vec<ProgressEventRow> =
            sqlx::query_as("SELECT id, task_id, at, quantity_done, delta_quantity, active_minutes, note FROM progress_events WHERE task_id = ? ORDER BY id ASC")
                .bind(&full)
                .fetch_all(&self.pool)
                .await
                .map_err(map_err)?;

        let open_session = sessions.iter().find(|s| s.ended_at.is_none()).cloned();
        let total_active_minutes = sessions.iter().map(session_minutes).sum();

        Ok(TaskProgress {
            task,
            open_session,
            sessions,
            events,
            total_active_minutes,
        })
    }

    async fn split_task(
        &self,
        id: &str,
        body: &SplitTask,
        operation_id: Option<&str>,
    ) -> StorageResult<SplitResult> {
        if body.retained_quantity < 0 {
            return Err(StorageError::BadRequest(
                "retained_quantity cannot be negative".into(),
            ));
        }

        let payload = serde_json::json!({"op": "split", "id": id, "body": body}).to_string();
        let request_hash = progress_request_hash(&payload, operation_id);

        let mut tx = self.pool.begin().await.map_err(map_err)?;
        if let Some(op_id) = operation_id
            && let Some(stored) =
                Self::check_progress_idempotency(&mut *tx, op_id, &request_hash).await?
        {
            return stored;
        }

        let full = resolve_task_id(&mut *tx, id).await?;

        let original: TaskRow = sqlx::query_as(SELECT_TASK_BY_ID)
            .bind(&full)
            .fetch_one(&mut *tx)
            .await
            .map_err(|e| match e {
                sqlx::Error::RowNotFound => StorageError::NotFound(format!("task {id} not found")),
                other => StorageError::Internal(other.to_string()),
            })?;

        if original.status == "completed" || original.status == "skipped" {
            return Err(StorageError::BadRequest(format!(
                "cannot split a {} task",
                original.status
            )));
        }

        let total = original.quantity_total.ok_or_else(|| {
            StorageError::BadRequest("cannot split a task with no quantity_total".into())
        })?;
        if body.retained_quantity <= 0 {
            return Err(StorageError::BadRequest(
                "retained_quantity must be greater than 0".into(),
            ));
        }
        if body.retained_quantity > total {
            return Err(StorageError::BadRequest(
                "retained_quantity cannot exceed quantity_total".into(),
            ));
        }
        if body.retained_quantity == total {
            return Err(StorageError::BadRequest(
                "retained_quantity must be less than quantity_total".into(),
            ));
        }
        if body.retained_quantity < original.quantity_done {
            return Err(StorageError::BadRequest(
                "retained_quantity cannot be less than quantity_done".into(),
            ));
        }
        let remainder_quantity = total - body.retained_quantity;
        let original_quantity_total = original.original_quantity_total.unwrap_or(total);

        // Allocate a display_id for the remainder task.
        let display_id: i64 = sqlx::query_scalar(
            "UPDATE task_display_id_seq SET next_id = next_id + 1 RETURNING next_id - 1",
        )
        .fetch_one(&mut *tx)
        .await
        .map_err(map_err)?;

        let remainder_id = uuid::Uuid::now_v7().to_string();
        let depends = if body.set_dependency.unwrap_or(false) {
            vec![full.clone()]
        } else {
            Vec::new()
        };
        let depends_json = serde_json::to_string(&depends).unwrap_or_else(|_| "[]".into());

        sqlx::query(
            "INSERT INTO tasks (id, display_id, title, description, start_at, end_at, avg_minutes, sigma_minutes, depends, parallelizable, allows_parallel, abandonability, status, ical_uid, habit_id, fixed, habit_step_id, quantity_total, quantity_done, quantity_unit, completed_at, split_from_task_id, original_quantity_total, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 'pending', ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, datetime('now'), datetime('now'))",
        )
        .bind(&remainder_id)
        .bind(display_id)
        .bind(body.title.as_ref().unwrap_or(&original.title))
        .bind(body.description.as_ref().or(original.description.as_ref()))
        .bind(&original.start_at)
        .bind(body.end_at.as_ref().unwrap_or(&original.end_at))
        .bind(original.avg_minutes)
        .bind(original.sigma_minutes)
        .bind(&depends_json)
        .bind(original.parallelizable)
        .bind(original.allows_parallel)
        .bind(original.abandonability)
        .bind(None::<String>) // ical_uid
        .bind(None::<String>) // habit_id
        .bind(original.fixed)
        .bind(None::<String>) // habit_step_id
        .bind(remainder_quantity)
        .bind(0i64)
        .bind(original.quantity_unit.as_ref())
        .bind(None::<String>) // completed_at
        .bind(&full)
        .bind(Some(original_quantity_total))
        .execute(&mut *tx)
        .await
        .map_err(map_err)?;

        let new_done = original.quantity_done.min(body.retained_quantity);
        sqlx::query(
            "UPDATE tasks SET quantity_total = ?, quantity_done = ?, original_quantity_total = ?, updated_at = datetime('now') WHERE id = ?",
        )
        .bind(body.retained_quantity)
        .bind(new_done)
        .bind(Some(original_quantity_total))
        .bind(&full)
        .execute(&mut *tx)
        .await
        .map_err(map_err)?;

        let original: TaskRow = sqlx::query_as(SELECT_TASK_BY_ID)
            .bind(&full)
            .fetch_one(&mut *tx)
            .await
            .map_err(map_err)?;
        let remainder: TaskRow = sqlx::query_as(SELECT_TASK_BY_ID)
            .bind(&remainder_id)
            .fetch_one(&mut *tx)
            .await
            .map_err(map_err)?;

        let result = SplitResult {
            original,
            remainder,
        };
        if let Some(op_id) = operation_id {
            Self::record_progress_operation(&mut *tx, op_id, &request_hash, &result).await?;
        }
        tx.commit().await.map_err(map_err)?;
        Ok(result)
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

impl SqliteStorage {
    async fn check_idempotency<'a, E, T: serde::de::DeserializeOwned>(
        executor: E,
        operation_id: &str,
        request_hash: &str,
    ) -> StorageResult<Option<StorageResult<T>>>
    where
        E: sqlx::Executor<'a, Database = sqlx::Sqlite>,
    {
        #[derive(sqlx::FromRow)]
        struct OpRow {
            request_hash: String,
            response_json: String,
        }
        let row: Option<OpRow> = sqlx::query_as(
            "SELECT request_hash, response_json FROM memory_operations WHERE operation_id = ?",
        )
        .bind(operation_id)
        .fetch_optional(executor)
        .await
        .map_err(map_err)?;

        if let Some(row) = row {
            if row.request_hash != request_hash {
                return Err(StorageError::Conflict(
                    "idempotency key reused with different request".into(),
                ));
            }
            let value: T = serde_json::from_str(&row.response_json).map_err(|e| {
                StorageError::Internal(format!("corrupt idempotency response: {e}"))
            })?;
            return Ok(Some(Ok(value)));
        }
        Ok(None)
    }

    async fn record_operation<'a, E, T: serde::Serialize>(
        executor: E,
        operation_id: &str,
        request_hash: &str,
        value: &T,
    ) -> StorageResult<()>
    where
        E: sqlx::Executor<'a, Database = sqlx::Sqlite>,
    {
        let response_json = serde_json::to_string(value)
            .map_err(|e| StorageError::Internal(format!("serialize idempotency response: {e}")))?;
        Self::record_operation_raw(executor, operation_id, request_hash, &response_json).await
    }

    async fn record_operation_raw<'a, E>(
        executor: E,
        operation_id: &str,
        request_hash: &str,
        response_json: &str,
    ) -> StorageResult<()>
    where
        E: sqlx::Executor<'a, Database = sqlx::Sqlite>,
    {
        sqlx::query(
            "INSERT INTO memory_operations (operation_id, request_hash, response_json) VALUES (?, ?, ?)",
        )
        .bind(operation_id)
        .bind(request_hash)
        .bind(response_json)
        .execute(executor)
        .await
        .map_err(map_err)?;
        Ok(())
    }

    async fn check_progress_idempotency<'a, E, T: serde::de::DeserializeOwned>(
        executor: E,
        operation_id: &str,
        request_hash: &str,
    ) -> StorageResult<Option<StorageResult<T>>>
    where
        E: sqlx::Executor<'a, Database = sqlx::Sqlite>,
    {
        #[derive(sqlx::FromRow)]
        struct OpRow {
            request_hash: String,
            response_json: String,
        }
        let row: Option<OpRow> = sqlx::query_as(
            "SELECT request_hash, response_json FROM progress_operations WHERE operation_id = ?",
        )
        .bind(operation_id)
        .fetch_optional(executor)
        .await
        .map_err(map_err)?;

        if let Some(row) = row {
            if row.request_hash != request_hash {
                return Err(StorageError::Conflict(
                    "idempotency key reused with different request".into(),
                ));
            }
            let value: T = serde_json::from_str(&row.response_json).map_err(|e| {
                StorageError::Internal(format!("corrupt idempotency response: {e}"))
            })?;
            return Ok(Some(Ok(value)));
        }
        Ok(None)
    }

    async fn record_progress_operation<'a, E, T: serde::Serialize>(
        executor: E,
        operation_id: &str,
        request_hash: &str,
        value: &T,
    ) -> StorageResult<()>
    where
        E: sqlx::Executor<'a, Database = sqlx::Sqlite>,
    {
        let response_json = serde_json::to_string(value)
            .map_err(|e| StorageError::Internal(format!("serialize idempotency response: {e}")))?;
        sqlx::query(
            "INSERT INTO progress_operations (operation_id, request_hash, response_json) VALUES (?, ?, ?)",
        )
        .bind(operation_id)
        .bind(request_hash)
        .bind(response_json)
        .execute(executor)
        .await
        .map_err(map_err)?;
        Ok(())
    }
}

fn memory_request_hash(payload: &str, operation_id: Option<&str>) -> String {
    crate::auth::hash_token(&format!("{}:{}", payload, operation_id.unwrap_or("")))
}

fn progress_request_hash(payload: &str, operation_id: Option<&str>) -> String {
    crate::auth::hash_token(&format!("{}:{}", payload, operation_id.unwrap_or("")))
}

/// Reject nonsensical quantity values and ensure `done <= total` when both
/// sides are provided.
fn validate_quantity(
    total: Option<i64>,
    done: Option<i64>,
    original: Option<i64>,
) -> StorageResult<()> {
    if let Some(t) = total
        && t < 0
    {
        return Err(StorageError::BadRequest(format!(
            "quantity_total must be >= 0 (got {t})"
        )));
    }
    if let Some(d) = done
        && d < 0
    {
        return Err(StorageError::BadRequest(format!(
            "quantity_done must be >= 0 (got {d})"
        )));
    }
    if let Some(o) = original
        && o < 0
    {
        return Err(StorageError::BadRequest(format!(
            "original_quantity_total must be >= 0 (got {o})"
        )));
    }
    if let (Some(t), Some(d)) = (total, done)
        && d > t
    {
        return Err(StorageError::BadRequest(format!(
            "quantity_done cannot exceed quantity_total ({d} > {t})"
        )));
    }
    Ok(())
}

/// Active minutes for a work session (closed or open).
fn session_minutes(session: &TaskWorkSessionRow) -> i64 {
    match session.ended_at.as_deref() {
        Some(end) => takusu_util::minutes_between(&session.started_at, end),
        None => {
            takusu_util::minutes_between(&session.started_at, &jiff::Timestamp::now().to_string())
        }
    }
}

/// Compute updated avg_minutes / sigma_minutes from a new positive progress
/// observation. See design/proposal.md WI-9 for the estimate-update formula.
async fn compute_updated_estimate<'a, E>(
    executor: E,
    task_id: &str,
    avg_minutes: i64,
    sigma_minutes: i64,
    quantity_total: Option<i64>,
    active_minutes: i64,
    delta_quantity: i64,
) -> StorageResult<(i64, i64)>
where
    E: sqlx::Executor<'a, Database = sqlx::Sqlite>,
{
    const MIN_MINUTES: f64 = 5.0;
    const MAX_MINUTES: f64 = 24.0 * 60.0;

    let total = match quantity_total {
        Some(t) if t > 0 => t as f64,
        _ => return Ok((avg_minutes, sigma_minutes)),
    };

    let minutes_per_unit = active_minutes as f64 / delta_quantity as f64;
    let projected = (minutes_per_unit * total).clamp(MIN_MINUTES, MAX_MINUTES);
    let new_avg_f = 0.5 * avg_minutes as f64 + 0.5 * projected;
    let new_avg = new_avg_f.round() as i64;

    // Collect all positive progress observations for this task.
    let events: Vec<ProgressEventRow> = sqlx::query_as(
        "SELECT id, task_id, at, quantity_done, delta_quantity, active_minutes, note FROM progress_events WHERE task_id = ? AND delta_quantity > 0 AND active_minutes > 0 ORDER BY id ASC",
    )
    .bind(task_id)
    .fetch_all(executor)
    .await
    .map_err(map_err)?;

    let projections: Vec<f64> = events
        .iter()
        .map(|e| {
            (e.active_minutes as f64 / e.delta_quantity.unwrap_or(1) as f64 * total)
                .clamp(MIN_MINUTES, MAX_MINUTES)
        })
        .collect();
    if projections.len() < 2 {
        return Ok((new_avg, sigma_minutes));
    }

    let mean = projections.iter().sum::<f64>() / projections.len() as f64;
    let variance = projections.iter().map(|x| (x - mean).powi(2)).sum::<f64>()
        / (projections.len() - 1) as f64;
    let stddev = variance.sqrt().clamp(MIN_MINUTES, MAX_MINUTES);
    let new_sigma = stddev.round() as i64;

    Ok((new_avg, new_sigma.max(1)))
}

async fn resolve_task_id<'c, E>(executor: E, id: &str) -> StorageResult<String>
where
    E: sqlx::Executor<'c, Database = sqlx::Sqlite>,
{
    // Allow display ids with a leading `#` (e.g. `#42`) written by the LLM.
    let id = id.strip_prefix('#').unwrap_or(id);

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
        .fetch_optional(executor)
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
        .fetch_optional(executor)
        .await
        .map_err(map_err)?
        .ok_or_else(|| StorageError::NotFound(format!("task {id} not found")));
    }
    if id.contains('-') {
        let exists: bool = sqlx::query_scalar("SELECT COUNT(*) > 0 FROM tasks WHERE id = ?")
            .bind(id)
            .fetch_one(executor)
            .await
            .map_err(map_err)?;
        if exists {
            return Ok(id.to_string());
        }
    } else {
        let matches: Vec<String> =
            sqlx::query_scalar("SELECT id FROM tasks WHERE id LIKE ? || '%'")
                .bind(id)
                .fetch_all(executor)
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
/// that `start <= end`. Mirrors the worker-side `validate_scheduled_span_dates`.
fn validate_scheduled_span_dates(start: &str, end: &str) -> Result<(), StorageError> {
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
