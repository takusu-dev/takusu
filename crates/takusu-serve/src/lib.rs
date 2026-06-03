//! # takusu-serve — REST API server for takusu planner
//!
//! axum + SQLite によるスケジュール管理サーバー。
//! 詳細なAPI仕様は `SPEC.md` を参照。
//!
//! ## 起動
//!
//! 環境変数 `TAKUSU_ROOT_TOKEN` が必須。その他はデフォルト値あり。
//!
//! | 変数 | デフォルト | 説明 |
//! |------|-----------|------|
//! | `TAKUSU_ROOT_TOKEN` | (必須) | ルートトークン (`tsk_` + UUID v7) |
//! | `TAKUSU_DB` | `./takusu.db` | SQLiteファイルパス |
//! | `TAKUSU_BIND` | `127.0.0.1:3000` | バインドアドレス |
//! | `TAKUSU_LOG` | `info` | ログレベル |
//!
//! ## モジュール構成
//!
//! - `app` — Router構築、AppState定義
//! - `auth` — Bearer token認証ミドルウェア、SHA-256ハッシュ
//! - `db` — SQLite接続、マイグレーション
//! - `error` — AppError列挙型 (NotFound/BadRequest/Unauthorized/Conflict/Internal)
//! - `model` — DB行構造体、リクエスト/レスポンス型
//! - `handler` — 各エンドポイントのハンドラ関数群

pub mod app;
pub mod auth;
pub mod db;
pub mod error;
pub mod handler;
pub mod model;
