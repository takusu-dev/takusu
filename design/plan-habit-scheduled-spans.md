# habit scheduled spans 設計 (#503)

## Summary

`habits.active = false`（無効）の habit にも、指定した日付範囲だけタスクを生成できる **scheduled span** を追加する。
既存の `habit_pauses` テーブル/概念を中立的な `habit_scheduled_spans` に全面リネームし、`habits.active` フラグでその意味を反転させる。

- `active = true` の habit：scheduled span は「休止期間」として動作。期間内の occurrence は生成しない。
- `active = false` の habit：scheduled span は「アクティブ期間」として動作。期間内の occurrence のみ生成する。

これにより disabled habit への scheduled activation span が実現し、active フラグを反転させても日付範囲データが孤立しない。

## Naming (old → new)

| 対象 | 旧 | 新 |
|---|---|---|
| DB テーブル | `habit_pauses` | `habit_scheduled_spans` |
| インデックス | `idx_habit_pauses_habit` | `idx_habit_scheduled_spans_habit` |
| Rust / TypeScript 型 | `HabitPauseRow` / `CreateHabitPause` | `HabitScheduledSpanRow` / `CreateHabitScheduledSpan` |
| Storage trait methods | `list_habit_pauses` 等 | `list_habit_scheduled_spans` 等 |
| REST API | `/habits/:id/pauses` 等 | `/habits/:id/scheduled-spans` 等 |
| CLI | `habit pause` | `habit scheduled-spans`（alias `spans`, `pause`） |
| Mobile API methods | `listHabitPauses` 等 | `listHabitScheduledSpans` 等 |

## Data model

```sql
CREATE TABLE IF NOT EXISTS habit_scheduled_spans (
    id         TEXT PRIMARY KEY,
    habit_id   TEXT NOT NULL REFERENCES habits(id) ON DELETE CASCADE,
    start_date TEXT NOT NULL,  -- YYYY-MM-DD, 両端含む, ユーザーのローカルタイムゾーン日付
    end_date   TEXT NOT NULL,
    reason     TEXT,
    created_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_habit_scheduled_spans_habit ON habit_scheduled_spans(habit_id);
```

- 型は `HabitScheduledSpanRow` / `CreateHabitScheduledSpan`。
- `habit.active` フラグが span の解釈を決定する。span 自体には "pause" か "activation" かのフラグは不要。

## Migration

### SQLite（`takusu-local-lib`）

`crates/takusu-local-lib/src/storage_sqlite.rs` の `init` に、legacy `habit_pauses` テーブルが存在すれば `habit_scheduled_spans` へリネームする処理を追加する。

```rust
let has_old: bool = sqlx::query_scalar(
    "SELECT COUNT(*) > 0 FROM sqlite_master WHERE type='table' AND name='habit_pauses'",
)
.fetch_one(&pool)
.await?;

if has_old {
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
```

- 新規 DB：`010_habit_pauses.sql` で `habit_pauses` テーブルを作成 → 上記リネーム
- 既存 DB：`010` は no-op → 上記リネーム
- 2 回目以降の起動は legacy テーブルが存在しないため no-op

### D1（`takusu-worker`）

`crates/takusu-worker/migrations/016_rename_habit_pauses_to_scheduled_spans.sql` を追加。

```sql
ALTER TABLE habit_pauses RENAME TO habit_scheduled_spans;
CREATE INDEX IF NOT EXISTS idx_habit_scheduled_spans_habit ON habit_scheduled_spans(habit_id);
DROP INDEX IF EXISTS idx_habit_pauses_habit;
```

- wrangler migration は 1 回のみ実行される。
- 新規 D1：`010` で `habit_pauses` を作成 → `016` でリネーム
- 既存 D1：`016` でリネーム

## Sync logic

`crates/takusu-local-lib/src/app.rs` の `sync_habit_tasks` で、全 habit を対象にし、`habit.active` によって span 内 occurrence の扱いを反転させる。

```rust
let in_span = spans
    .iter()
    .any(|(s, e)| date >= s.as_str() && date <= e.as_str());

// active habit  → span 内はスキップ（pause）
// disabled habit → span 内のみ生成（activation span）
if habit.active && in_span {
    continue;
}
if !habit.active && !in_span {
    continue;
}
```

- `day` モード、`period` モードの両方で同じ日付判定を使う。
- period モードの deadline クランプは行わない。
- disabled habit かつ span が 0 件の場合は一切生成しない。
- cleanup ループが「期待されなくなった pending・未編集タスク」を自動削除するため、active 反転後も整合性が保たれる。

## API

新しいエンドポイント：

- `GET /habits/:id/scheduled-spans`
- `GET /habits/scheduled-spans`
- `POST /habits/:id/scheduled-spans`
- `DELETE /habits/:id/scheduled-spans/:span_id`

旧 `/habits/:id/pauses` 系は廃止する。mobile / CLI も新しいエンドポイントに移行する。

## CLI

`habit scheduled-spans` を canonical とし、aliases として `spans` と `pause` を残す。

- `habit scheduled-spans add <id> --from YYYY-MM-DD --to YYYY-MM-DD [--reason ...]`
- `habit scheduled-spans list <id>`
- `habit scheduled-spans rm <id> <span_id>`

`habit show` の span 一覧ラベルは `habit.active` で切り替える。

- active habit: `scheduled spans (paused):`
- disabled habit: `scheduled spans (active):`

## Mobile UI

### `HabitDetailView`

active 状態に応じて section ラベルとアイコンを切り替える。

- `active = true`：
  - section タイトル: 「休止期間」
  - add ボタン: 「休止期間を追加」
  - kebab menu: 「休止期間を追加...」
  - アイコン: `pause-circle` 系
- `active = false`：
  - section タイトル: 「アクティブ期間 (scheduled)」
  - add ボタン: 「アクティブ期間を追加」
  - kebab menu: 「アクティブ期間を追加...」
  - アイコン: `play-circle` 系

追加 / 削除は `createHabitScheduledSpan` / `deleteHabitScheduledSpan` を呼ぶ。undo/redo スタックにも積む。

### `HabitView` chip design

`n steps` chip と同じデザイン感で、scheduled span 状態を表示する。

- active habit + 今日が span 内：
  - 赤 chip: `⏸ 〜8/7`
- disabled habit + 今日が span 内：
  - brand 色 chip: `▶ scheduled 〜8/7`
  - カードは通常の active 表示（グレーアウト・打ち消し線を解除）
- disabled habit + span あり & 今日は外：
  - brand 色 chip: `scheduled`
  - カードは disabled 表示のまま
- disabled habit + span なし：
  - chip なし、disabled 表示

## Tests

- `sync_habit_tasks`
  - disabled habit + span で期間内の occurrence のみ生成
  - disabled habit + span なしでは一切生成しない
  - active habit + span で期間内の occurrence がスキップされる（後方互換）
  - active 反転後に cleanup が正しく動作する
- API / CLI
  - CRUD、日付の逆転・形式不正バリデーション
  - CLI alias (`spans`, `pause`) の動作
- Migration
  - SQLite init で legacy `habit_pauses` からのリネームが 2 回目以降も idempotent
  - D1 migration `016` でリネーム後にアプリが動作
- Mobile
  - `npm run lint` / `fmt:check` / typecheck
  - `HabitView` / `HabitDetailView` の表示切り替えを手動または story / jest で確認

## Implementation order

1. DB migration + `storage_sqlite.rs` init リネーム
2. `takusu-storage` 型・trait リネーム
3. `takusu-local-lib/src/app.rs` の `sync_habit_tasks` 反転ロジック + wrapper methods
4. `takusu-local` router / handlers リネーム
5. `takusu-client` + `takusu-cli` リネーム + alias
6. `takusu-worker` models / handlers / router / migration
7. `mobile/src/api/types.ts` / `client.ts` / `HabitDetailView.tsx` / `HabitView.tsx`
8. 既存テスト更新 + 新規テスト追加
9. 検証：`cargo nextest run --workspace`、`cargo clippy`、`cargo fmt`、`npm run lint` / `fmt:check`
10. `jj describe` → `jj git push --change` → `gh pr create`

## Notes

- `active = false` の habit に span を作ってその後 `active = true` に戻すと、その span は pause として解釈される。次回 `sync_habit_tasks` で該当する pending・未編集タスクは削除される。これは「データが孤立しない」ための自然な振る舞い。
- 過去の scheduled span は許可する。`sync_habit_tasks` は今日から 14 日先までしか生成しないので、過去のみの span は無視される。
- span の重複は union とみなす。重複排除や conflict 検出は行わない。
- 全 `habit_pauses` / `HabitPause` / `pauses` の文字列を新しい命名に置換する。
