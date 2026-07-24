# REST API 概要

`takusu-local` は axum で実装された REST API サーバーです。すべての `/api/*` エンドポイントは `Authorization: Bearer <token>` を必要とします（`/health` を除く）。

## 認証

```http
Authorization: Bearer <token>
```

トークンは `tokens` テーブルに SHA-256 ハッシュで保存されます。ルートトークン `TAKUSU_ROOT_TOKEN` はすべての操作が可能です。

## エンドポイント

### Task

```
GET    /api/tasks
GET    /api/tasks/:id
PUT    /api/tasks/:id
PATCH  /api/tasks/:id
DELETE /api/tasks/:id
POST   /api/tasks
GET    /api/tasks/complete
GET    /api/tasks/similar
POST   /api/tasks/import/ical
GET    /api/tasks/dependency-analysis
POST   /api/tasks/:id/work/start
POST   /api/tasks/:id/work/pause
POST   /api/tasks/:id/work/complete
POST   /api/tasks/:id/progress
GET    /api/tasks/:id/progress
POST   /api/tasks/:id/split
```

### Habit

```
GET    /api/habits
GET    /api/habits/:id
PUT    /api/habits/:id
PATCH  /api/habits/:id
DELETE /api/habits/:id
POST   /api/habits
POST   /api/habits/:id/estimate
GET    /api/habits/scheduled-spans
GET    /api/habits/steps
GET    /api/habits/:id/scheduled-spans
POST   /api/habits/:id/scheduled-spans
DELETE /api/habits/:id/scheduled-spans/:span_id
GET    /api/habits/:id/steps
PUT    /api/habits/:id/steps
GET    /api/habits/:id/steps/dependency-analysis
```

### Schedule

```
GET    /api/schedule
POST   /api/schedule/generate
POST   /api/schedule/preview
POST   /api/schedule/replace
POST   /api/schedule/reschedule
PATCH  /api/schedule/entries/:task_id
DELETE /api/schedule
```

### Settings

```
GET  /api/settings
PUT  /api/settings
PUT  /api/workers/config
GET  /api/workers/health
```

### Token

```
POST   /api/tokens
GET    /api/tokens
DELETE /api/tokens/:id
```

### Sync

```
GET  /api/sync/settings
PUT  /api/sync/settings
POST /api/sync/oauth/url
POST /api/sync/oauth/callback
POST /api/sync/trigger
POST /api/sync/delete-all
GET  /api/sync/mappings
```

### Skills

```
GET    /api/skills
POST   /api/skills
GET    /api/skills/:slug
PATCH  /api/skills/:slug
DELETE /api/skills/:slug
```

### Memory

```
POST   /api/memory
GET    /api/memory/search
GET    /api/memory/:id
PATCH  /api/memory/:id
DELETE /api/memory/:id
```

## クライアントライブラリ

`takusu-client` クレートは上記エンドポイントを Rust から呼び出すためのライブラリです。

```rust
use takusu_client::Client;

let client = Client::new("http://127.0.0.1:3000", "tsk_xxx");
let tasks = client.list_tasks(None).await?;
```

## 詳細

各エンドポイントのリクエスト/レスポンス型は `takusu-storage` クレートの `model.rs` を参照してください。
