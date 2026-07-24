# タスク関連ツール

## `list_tasks`

タスクを一覧取得します。検索修飾子で絞り込めます。

```json
{
  "status": "pending",
  "q": "status:pending 買い物",
  "limit": 10
}
```

| パラメータ | 型 | 説明 |
|------------|-----|------|
| `status` | string | `pending`, `scheduled`, `in_progress`, `completed`, `skipped`, `overdue` |
| `from` | string | 範囲開始 |
| `until` | string | 範囲終了 |
| `no_overdue` | boolean | `true` で期限切れを除外 |
| `habit_id` | string | 習慣参照（例: `h1`） |
| `q` | string | 検索クエリ |
| `limit` | integer | 最大件数 |

## `get_task`

タスクを参照で取得します。依存関係も含みます。

```json
{
  "task_ref": "#42"
}
```

| パラメータ | 型 | 説明 |
|------------|-----|------|
| `task_ref` | string / array | `#42` または `["#1", "h1#3"]` |

返り値:

- `tasks`: 指定したタスク
- `dependencies`: 推移的な依存タスク
- `missing_dependencies`: 見つからなかった依存 ID

## `create_task`

タスク作成を提案します。承認後に確定します。

必須:

- `title`
- `end_at`
- `avg_minutes`

任意:

- `description`, `start_at`, `sigma_minutes`, `depends`, `parallelizable`, `allows_parallel`, `abandonability`, `inferred_fields`, `why`, `warnings`

## `update_task`

タスク更新を提案します。

必須:

- `task_ref`

任意:

- `title`, `description`, `start_at`, `end_at`, `avg_minutes`, `sigma_minutes`, `depends`, `parallelizable`, `allows_parallel`, `abandonability`, `status`, `inferred_fields`, `why`, `warnings`

## `delete_task`

タスク削除を提案します。

必須:

- `task_ref`

## `move_task`

スケジュール済みタスクの開始時刻を移動する提案を生成します。

必須:

- `task_ref`
- `start_at`

任意:

- `force`（デフォルト false）
- `fixed`（デフォルト true）
- `why`, `warnings`, `inferred_fields`
