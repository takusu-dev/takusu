# takusu-serve 仕様書

## 全体アーキテクチャ

```
crates/
├── takusu-core/       # スケジューリングコア (既存 + plan_partial/plan_in_range 追加)
├── takusu-serve/      # REST APIサーバー (axum + SQLite)
├── takusu-ical/       # iCalendarパーサー (独立クレート)
├── takusu-audio/      # 音声処理 (既存)
└── google-cal/        # Google Calendar APIクライアント (OAuth2 + 差分同期)
```

## 依存クレート

| クレート | 用途 |
|---|---|
| `axum` | HTTPフレームワーク |
| `tokio` | 非同期ランタイム |
| `sqlx` (sqlite) | DBアクセス |
| `serde` / `serde_json` | シリアライズ |
| `uuid` (v7) | ID生成 |
| `sha2` | トークンハッシュ |
| `takusu-core` | スケジューリング |
| `takusu-ical` | iCalパース |
| `google-cal` | Google Calendar API連携 |
| `tower-http` (cors, trace) | ミドルウェア |
| `tracing` | ログ |

## 認証

- ルートトークン: 環境変数 `TAKUSU_ROOT_TOKEN` (フォーマット: `tsk_` + UUID v7)
- 発行済みトークン: `tokens` テーブルにSHA-256ハッシュで保存
- 任意の有効トークンが新規トークン発行可能 (信頼チェーン)
- 取り消しはカスケードしない (各トークン独立)
- 全 `/api/*` に認証ミドルウェア適用、`/health` は認証なし

### トークンフォーマット

```
tsk_<UUID v7>
```

例: `tsk_0196d5a05c3a7f2eb91d4a8e3c2d1f00`

### トークン検証フロー

1. `Authorization: Bearer <token>` ヘッダを取得
2. ルートトークンと直接比較
3. ルートでなければ SHA-256 ハッシュ → `tokens` テーブル照合 (`revoked_at IS NULL`)
4. 不正 → `401 Unauthorized`

## DBスキーマ (SQLite)

```sql
CREATE TABLE tokens (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    token_hash  TEXT NOT NULL UNIQUE,
    label       TEXT,
    created_by  TEXT NOT NULL,
    created_at  TEXT NOT NULL DEFAULT (datetime('now')),
    revoked_at  TEXT
);

CREATE TABLE habits (
    id          TEXT PRIMARY KEY,
    title       TEXT NOT NULL,
    description TEXT,
    recurrence  TEXT NOT NULL,
    start_time  TEXT NOT NULL,
    end_time    TEXT NOT NULL,
    avg_minutes INTEGER NOT NULL,
    sigma_minutes INTEGER NOT NULL DEFAULT 0,
    parallelizable   BOOLEAN NOT NULL DEFAULT 0,
    allows_parallel  BOOLEAN NOT NULL DEFAULT 0,
    abandonability   REAL NOT NULL DEFAULT 0.0,
    active      BOOLEAN NOT NULL DEFAULT 1,
    created_at  TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at  TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE tasks (
    id          TEXT PRIMARY KEY,
    title       TEXT NOT NULL,
    description TEXT,
    start_at    TEXT,
    end_at      TEXT NOT NULL,
    avg_minutes INTEGER NOT NULL,
    sigma_minutes INTEGER NOT NULL DEFAULT 0,
    depends     TEXT NOT NULL DEFAULT '[]',
    parallelizable   BOOLEAN NOT NULL DEFAULT 0,
    allows_parallel  BOOLEAN NOT NULL DEFAULT 0,
    abandonability   REAL NOT NULL DEFAULT 0.5,
    status      TEXT NOT NULL DEFAULT 'pending'
                 CHECK(status IN ('pending','scheduled','in_progress','completed','skipped')),
    habit_id    TEXT REFERENCES habits(id),
    ical_uid    TEXT,
    created_at  TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at  TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE UNIQUE INDEX idx_tasks_ical_uid ON tasks(ical_uid) WHERE ical_uid IS NOT NULL;

CREATE TABLE schedules (
    id          TEXT PRIMARY KEY DEFAULT 'active',
    created_at  TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at  TEXT NOT NULL DEFAULT (datetime('now')),
    schedule    TEXT NOT NULL
);

CREATE TABLE google_cal_settings (
    id            TEXT PRIMARY KEY DEFAULT 'active',
    enabled       BOOLEAN NOT NULL DEFAULT 0,
    calendar_id   TEXT NOT NULL DEFAULT 'primary',
    client_id     TEXT NOT NULL DEFAULT '',
    client_secret TEXT NOT NULL DEFAULT '',
    refresh_token TEXT,
    created_at    TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at    TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE google_cal_events (
    task_id         TEXT PRIMARY KEY REFERENCES tasks(id) ON DELETE CASCADE,
    google_event_id TEXT NOT NULL,
    updated_at      TEXT NOT NULL DEFAULT (datetime('now'))
);
```

`schedules` テーブルは常に1行 (`id = 'active'`)。UPSERTで更新。

## API一覧

### Task

| メソッド | パス | 説明 |
|---|---|---|
| `POST` | `/api/tasks` | 作成 |
| `GET` | `/api/tasks` | 一覧 (フィルタ可) |
| `GET` | `/api/tasks/:id` | 取得 |
| `PUT` | `/api/tasks/:id` | 全文置き換え |
| `PATCH` | `/api/tasks/:id` | 部分更新 |
| `DELETE` | `/api/tasks/:id` | 削除 |
| `POST` | `/api/tasks/import/ical` | iCalインポート |

#### GET /api/tasks クエリパラメータ

| パラメータ | 型 | 説明 |
|---|---|---|
| `status` | string | ステータスフィルタ (pending, scheduled, in_progress, completed, skipped) |
| `from` | ISO 8601 | `end_at >= from` |
| `until` | ISO 8601 | `start_at <= until` |
| `habit_id` | UUID | 習慣由来のみ |

#### Task CRUD レスポンス

タスクCRUDのレスポンスには `unscheduled_count` を含める:
アクティブスケジュールに含まれていない `status=pending` タスクの数。

```json
{
  "task": { "id": "...", "title": "...", ... },
  "unscheduled_count": 3
}
```

### Habit

| メソッド | パス | 説明 |
|---|---|---|
| `POST` | `/api/habits` | 作成 |
| `GET` | `/api/habits` | 一覧 |
| `GET` | `/api/habits/:id` | 取得 |
| `PUT` | `/api/habits/:id` | 全文置き換え |
| `PATCH` | `/api/habits/:id` | 部分更新 |
| `DELETE` | `/api/habits/:id` | 削除 |

#### Habit recurrence 表現

- `"daily"` — 毎日
- `"weekdays"` — 月〜金
- `"Mon,Wed,Fri"` — 特定曜日
- 将来: cron式拡張用

#### Habit → Task 自動生成

`POST /api/schedule/generate` 実行時に:
1. `habits` から `active=true` を取得
2. `recurrence` ルールに基づきスケジュール期間内に該当するインスタンスを生成
3. 生成したタスクの `habit_id` に元habitのIDを記録
4. 生成タスクを `task_ids` に追加して Planner に渡す

### Schedule

| メソッド | パス | 説明 |
|---|---|---|
| `GET` | `/api/schedule` | アクティブスケジュール取得 |
| `POST` | `/api/schedule/generate` | 全タスクでスケジュール再生成 |
| `POST` | `/api/schedule/reschedule` | 部分再スケジュール |
| `PATCH` | `/api/schedule/entries/:task_id` | タスク位置の手動調整 |
| `DELETE` | `/api/schedule` | スケジュールクリア |

#### POST /api/schedule/generate

```json
{
  "until": "2026-06-06T23:59:59+09:00",
  "sleep": "recommended"
}
```

`task_ids` 省略時は `status IN ('pending', 'scheduled')` のタスクを対象。
開始時刻は常に現在時刻 (`now`) が使用される。

#### POST /api/schedule/reschedule

**mode: range** — 指定期間内のタスクを再スケジュール:

```json
{
  "mode": "range",
  "from": "2026-06-05T00:00:00+09:00",
  "until": "2026-06-05T23:59:59+09:00",

`from` / `until` に `"now"` を指定すると現在時刻が使われる。
  "pinned": ["0196d5a0-..."],
  "sleep": "recommended"
}
```

**mode: tasks** — 指定タスクのみ再スケジュール:

```json
{
  "mode": "tasks",
  "task_ids": ["0196d5a1-...", "0196d5a2-..."],
  "pinned": [],
  "sleep": "recommended"
}
```

`pinned`: 動かさないタスクID。

#### PATCH /api/schedule/entries/:task_id

```json
{
  "start_at": "2026-06-05T14:00:00+09:00",
  "force": false
}
```

- `force: false` (デフォルト): 違反があれば `409 Conflict`
- `force: true`: warnings付きで保存 (`200`)

**409 Conflict レスポンス:**

```json
{
  "message": "schedule violations detected",
  "warnings": ["dependency_violation", "sleep_violation"],
  "preview": {
    "task_id": "...",
    "start_at": "2026-06-05T14:00:00+09:00",
    "end_at": "2026-06-05T16:00:00+09:00"
  }
}
```

**force: true 時の200レスポンス:**

```json
{
  "task_id": "...",
  "start_at": "2026-06-05T14:00:00+09:00",
  "end_at": "2026-06-05T16:00:00+09:00",
  "warnings": ["dependency_violation"]
}
```

**warnings 一覧:**

| 値 | 説明 |
|---|---|
| `dependency_violation` | 依存タスクがまだ終了していない時刻に配置 |
| `sleep_violation` | 睡眠時間帯に配置 |
| `parallel_violation` | 並列不可タスクと重複 |

#### DELETE /api/schedule

アクティブスケジュールをクリア。タスクの `status` は `pending` に戻さない (手動で変更する必要あり)。

### Token

| メソッド | パス | 説明 |
|---|---|---|
| `POST` | `/api/tokens` | 新規発行 |
| `GET` | `/api/tokens` | 一覧 |
| `DELETE` | `/api/tokens/:id` | 取り消し |

#### POST /api/tokens

```json
{
  "label": "Android"
}
```

#### レスポンス 201

```json
{
  "id": 3,
  "token": "tsk_0196d5a05c3a7f2eb91d4a8e3c2d1f00",
  "label": "Android",
  "created_at": "2026-06-04T12:00:00+09:00"
}
```

トークン全文はこのレスポンスでのみ返却。以後はハッシュのみ保存。

### Health

| メソッド | パス | 説明 |
|---|---|---|
| `GET` | `/health` | 認証なしヘルスチェック |

## iCalインポート

### エンドポイント

`POST /api/tasks/import/ical` (`Content-Type: text/calendar`)

### リクエスト

```
BEGIN:VCALENDAR
BEGIN:VEVENT
DTSTART:20260605T090000Z
DTEND:20260605T110000Z
SUMMARY:企画書作成
END:VEVENT
END:VCALENDAR
```

### レスポンス 200

```json
{
  "imported": 3,
  "task_ids": ["0196d5a0-...", "0196d5a1-...", "0196d5a2-..."]
}
```

### 重複処理

同じ `ical_uid` が既に存在する場合はスキップ。

### takusu-ical IcalTask

```rust
pub struct IcalTask {
    pub title: String,
    pub description: Option<String>,
    pub start_at: OffsetDateTime,
    pub end_at: OffsetDateTime,
    pub uid: Option<String>,
    pub rrule: Option<String>,
}
```

## core変換レイヤー

API ↔ `takusu-core` 変換:

- `Point::from_timestamp(ts, 5)` / `Point → ISO 8601`: 日時とスロットの相互変換
- `avg_minutes` / `sigma_minutes` → `NormalDist::new(avg / 5, sigma / 5)`: 分 → スロット
- `depends` の `Vec<String>` (UUID) → `Vec<usize>` (Planner内部ID): Plannerに渡す際にインデックスにマッピング

### Sync (Google Calendar連携)

| メソッド | パス | 説明 |
|---|---|---|
| `GET` | `/api/sync/settings` | 同期設定取得 |
| `PUT` | `/api/sync/settings` | 同期設定更新 (UPSERT) |
| `POST` | `/api/sync/oauth/url` | OAuth2 認可URL生成 |
| `POST` | `/api/sync/oauth/callback` | OAuth2 コールバック (refresh_token保存) |
| `POST` | `/api/sync/trigger` | 手動同期トリガー |

#### GET /api/sync/settings

```json
{
  "enabled": false,
  "calendar_id": "primary",
  "client_id": "",
  "has_client_secret": false,
  "has_refresh_token": false
}
```

`client_secret` と `refresh_token` はレスポンスに含まず、設定済みかどうかのブール値のみ返す。

#### PUT /api/sync/settings

```json
{
  "enabled": true,
  "calendar_id": "primary",
  "client_id": "xxx.apps.googleusercontent.com",
  "client_secret": "GOCSPX-xxx",
  "refresh_token": null
}
```

省略したフィールドは既存値を維持 (UPSERT)。`refresh_token` は通常OAuth2コールバックで設定されるため、ここでは設定しない。

#### POST /api/sync/oauth/url

```json
{
  "redirect_uri": "https://example.com/callback"
}
```

レスポンス:

```json
{
  "url": "https://accounts.google.com/o/oauth2/v2/auth?..."
}
```

ユーザーをこのURLにリダイレクトし、Google認可後にリダイレクトURIに `code` パラメータが付与される。

#### POST /api/sync/oauth/callback

```json
{
  "code": "4/0AX4X...",
  "redirect_uri": "https://example.com/callback"
}
```

レスポンス:

```json
{
  "refresh_token_set": true
}
```

認可コードをGoogleに送信し、取得したrefresh_tokenをDBに保存。事前に `PUT /api/sync/settings` で `client_id` / `client_secret` を設定しておく必要がある。

#### POST /api/sync/trigger

```json
{
  "status": "sync_triggered"
}
```

手動で同期をトリガー。実際の同期はバックグラウンドで実行され、レスポンスは即座に返却される。

### 同期アーキテクチャ

#### 自動トリガー

スケジュールの再計算時に自動的に同期がトリガーされる:

- `POST /api/schedule/generate` — スケジュール生成後
- `POST /api/schedule/reschedule` — 再スケジュール後
- `PATCH /api/schedule/entries/:task_id` — タスク移動後
- `DELETE /api/schedule` — スケジュールクリア後

いずれも `tokio::spawn` でバックグラウンド実行。レスポンスは同期待たず即時返却。

#### リトライ

失敗時は指数バックオフで最大3回リトライ:

| 試行 | 待機 |
|---|---|
| 1回目 | 即時 |
| 2回目 | 5秒 |
| 3回目 | 10秒 |
| 4回目 | 20秒 |

リトライ間で `tokio::sync::Mutex` ロックを解放し、他の同期要求をブロックしないよう設計。

#### 同期ロック

`AppState` の `sync_lock: Arc<Mutex<()>>` により、同時に1つの同期のみ実行可能。スケジュール操作ごとに `tokio::spawn` で同期タスクを起動するが、ロック取得後に実行されるため並行実行はされない。

#### 差分ベース同期フロー

1. DBから設定読み込み → `enabled=false` または `refresh_token` がなければ終了
2. アクティブスケジュール取得:
   - **スケジュールあり**: 各タスクの `title` / `description` をDBから取得し、`SyncEntry` 構築
   - **スケジュールなし**: 既存のGoogle Calendarイベントを全削除し、マッピングDBもクリア
3. `google_cal_events` テーブルから既存マッピング (`task_id → google_event_id`) を取得
4. `Client::sync()` を呼び出し:
   - マッピングに存在しないタスク → Google Calendarイベント作成
   - マッピングに存在するタスク → Google Calendarイベント更新
   - スケジュールに存在しないマッピング → Google Calendarイベント削除
   - 更新失敗時 → 新規作成にフォールバック
5. 結果のマッピングを `google_cal_events` に UPSERT / DELETE

#### google-cal クレート

`crates/google-cal/` — Google Calendar APIクライアント。HTTP依存のみ (rustls-tls)。

主要型:

```rust
pub struct Client {
    client_id, client_secret, refresh_token, calendar_id
}

pub struct SyncEntry {
    pub task_id: String,
    pub summary: String,
    pub description: Option<String>,
    pub start: String,   // ISO 8601
    pub end: String,      // ISO 8601
}

pub struct SyncResult {
    pub mappings: Vec<(String, String)>,  // (task_id, google_event_id)
    pub deleted: Vec<String>,              // 削除された google_event_id
}
```

主要メソッド:

| メソッド | 説明 |
|---|---|
| `Client::sync(entries, existing)` | 差分同期 (作成/更新/削除) |
| `Client::delete_all(event_ids)` | 全イベント削除 |
| `oauth_url(client_id, redirect_uri)` | OAuth2 認可URL生成 |
| `exchange_code(client_id, secret, code, redirect_uri)` | 認可コード→トークン交換 |

イベントには `extendedProperties.private.takusuTaskId` でタスクIDを埋め込む。

## 環境変数

| 変数 | デフォルト | 説明 |
|---|---|---|
| `TAKUSU_ROOT_TOKEN` | (必須) | ルートトークン |
| `TAKUSU_DB` | `./takusu.db` | SQLiteパス |
| `TAKUSU_BIND` | `127.0.0.1:3000` | バインドアドレス |
| `TAKUSU_LOG` | `info` | ログレベル |

## takusu-core 変更点

### RescheduleRange

```rust
pub struct RescheduleRange {
    pub from: Point,
    pub until: Point,
}
```

### Planner::plan_partial

```rust
impl Planner {
    /// 固定タスクを保持したまま未固定タスクをスケジュール。
    pub fn plan_partial(&self, pinned: &[(Point, Point, usize)]) -> Plan {
        solver::solve_partial(self, pinned)
    }
}
```

- 固定タスクは評価関数に含むが近傍操作対象外
- 未固定タスクのみSAで配置

### Planner::plan_in_range

```rust
impl Planner {
    /// 指定期間内のタスクのみ再スケジュール。
    pub fn plan_in_range(&self, range: RescheduleRange) -> Plan {
        // 期間外のタスク = pinned, 期間内 = SA対象
    }
}
```

### SAの変更点

- 初期解生成: pinnedタスクを配置済みブロックとして扱い、残りスロットに未固定タスクをランダム配置
- 近傍操作: 未固定タスクのみ対象
- 評価関数: 固定・未固定両方を考慮 (並列違反・依存・睡眠)

## 実装順序

1. **takusu-core**: `plan_partial` / `plan_in_range` 追加 + テスト
2. **takusu-ical**: ICalパーサー実装
3. **takusu-serve**: db → model → auth → handler → main の順
4. **google-cal**: Google Calendar APIクライアント実装 (OAuth2, 差分同期)
5. **takusu-serve (sync)**: DB マイグレーション → handler/sync.rs → scheduleハンドラにトリガー統合