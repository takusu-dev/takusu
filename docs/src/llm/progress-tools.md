# 進捗関連ツール

タスクの開始・一時停止・進捗・完了・分割を扱います。これらのツールも承認フローを経ます。

## `task_start`

タスクの作業開始を提案します。`in_progress` 状態に変更されます。

```json
{
  "task_ref": "#42"
}
```

`task_ref` を省略すると、`scheduled` / `pending` のタスクから選択を促す `focused_clarification` を返します。

## `task_pause`

タスクの一時停止を提案します。作業セッションを閉じ、実働時間を記録します。

```json
{
  "task_ref": "#42"
}
```

## `task_progress`

タスクの累積進捗を記録します。`quantity_done` を更新します。

```json
{
  "task_ref": "#42",
  "quantity_done": 15,
  "note": "第 3 章まで終了"
}
```

| パラメータ | 型 | 説明 |
|------------|-----|------|
| `task_ref` | string | タスク参照 |
| `quantity_done` | integer | 累積完了量 |
| `note` | string | 進捗メモ |

## `task_complete`

タスクの完了を提案します。`completed` 状態に変更し、実績時間を記録します。

```json
{
  "task_ref": "#42"
}
```

## `task_split`

タスクを「元のタスクに残す量」と「新しい残りタスク」に分割します。

```json
{
  "task_ref": "#42",
  "retained_quantity": 10,
  "set_dependency": true,
  "title": "レポート（残り）",
  "end_at": "2026-07-30T18:00:00"
}
```

| パラメータ | 型 | 説明 |
|------------|-----|------|
| `task_ref` | string | タスク参照 |
| `retained_quantity` | integer | 元タスクに残す量（必須） |
| `set_dependency` | boolean | 残りタスクが元タスクに依存するか（デフォルト true） |
| `title` | string | 残りタスクのタイトル |
| `description` | string | 残りタスクの説明 |
| `end_at` | string | 残りタスクの締切 |
