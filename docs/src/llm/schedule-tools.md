# スケジュール関連ツール

## `get_schedule`

現在のスケジュールを取得します。日付範囲で絞り込めます。

```json
{
  "from": "today",
  "to": "7d",
  "no_overdue": false
}
```

| パラメータ | 型 | 説明 |
|------------|-----|------|
| `from` | string | 範囲開始（絶対/相対日付） |
| `to` | string | 範囲終了 |
| `no_overdue` | boolean | `true` で期限切れセクションを省略 |

返り値:

- `id`, `created_at`, `updated_at`
- `entries`: スケジュールエントリ一覧
- `overdue`: 期限切れタスク（`no_overdue=false` の場合）

## `preview_schedule`

スケジュールを生成するが、まだ `active` スケジュールを置き換えない。移動・未スケジュール・睡眠影響を含むプレビューを返す。

| パラメータ | 型 | 説明 |
|------------|-----|------|
| `mode` | string | `full`, `range`, `tasks` 等 |
| `from` | string | 範囲開始 |
| `until` | string | 範囲終了 |
| `task_ids` | array | 対象タスク参照 |
| `pinned` | array | 固定するタスク参照 |
| `sleep` | string | 睡眠設定 |

## `generate_schedule`

スケジュール生成を提案します。承認後に `active` スケジュールを更新します。

任意:

- `task_ids`, `sleep`
- `why`, `warnings`, `inferred_fields`

## `reschedule`

部分再スケジュールを提案します。

必須:

- `mode`

任意:

- `from`, `until`, `task_ids`, `pinned`, `sleep`
- `why`, `warnings`, `inferred_fields`

## `get_settings`

サーバーのタイムゾーン・睡眠設定を取得します。引数なし。

## `day_details`

1 日または複数の日について、曜日、日本の祝日情報、スケジュールを取得します。

```json
{
  "dates": ["today", "tomorrow", "2026-07-29"],
  "include_schedule": true
}
```

| パラメータ | 型 | 説明 |
|------------|-----|------|
| `dates` | array | 日付表現の配列（必須） |
| `include_schedule` | boolean | 各日のスケジュールを含めるか |

返り値:

- `date`: ISO 日付
- `weekday`: 日本語曜日
- `is_holiday`: 祝日かどうか
- `holiday_name`: 祝日名（該当する場合）
- `schedule`: スケジュールエントリ（`include_schedule=true` の場合）
