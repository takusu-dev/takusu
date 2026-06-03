//! # Handler modules
//!
//! 各APIエンドポイントに対応するハンドラ関数を提供する。
//!
//! - `task` — Task CRUD + iCalインポート (`/api/tasks`, `/api/tasks/import/ical`)
//! - `habit` — Habit CRUD (`/api/habits`)
//! - `schedule` — Schedule generate/reschedule/move/clear (`/api/schedule/*`)
//! - `token` — Token issue/list/revoke (`/api/tokens`)

pub mod habit;
pub mod schedule;
pub mod task;
pub mod token;
