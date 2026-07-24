# 習慣関連ツール

## `list_habits`

登録済みの習慣を一覧取得します。引数は不要です。

## `get_habit`

習慣を参照で取得します。詳細とステップ情報を含みます。

```json
{
  "habit_ref": "h1"
}
```

| パラメータ | 型 | 説明 |
|------------|-----|------|
| `habit_ref` | string / array | `h1` または `["h1", "h2"]` |

## `create_habit`

習慣作成を提案します。承認後に確定します。

必須:

- `title`
- `recurrence`（`RecurrenceRule` の JSON）
- `start_time`
- `end_time`
- `avg_minutes`

`recurrence` の例:

```json
{"freq":"daily","interval":1,"by_day":[],"by_month":[],"by_month_day":[],"count":null,"exdates":[]}
```

時刻は `start_time` / `end_time`（`HH:MM`）で指定します。`recurrence` に `BYHOUR` / `BYMINUTE` は含めません。

任意:

- `description`, `sigma_minutes`, `parallelizable`, `allows_parallel`, `abandonability`, `inferred_fields`, `why`, `warnings`

## `update_habit`

習慣更新を提案します。

必須:

- `habit_ref`

任意:

- `title`, `description`, `recurrence`, `start_time`, `end_time`, `avg_minutes`, `sigma_minutes`, `parallelizable`, `allows_parallel`, `abandonability`, `active`, `inferred_fields`, `why`, `warnings`

## `delete_habit`

習慣削除を提案します。

必須:

- `habit_ref`
