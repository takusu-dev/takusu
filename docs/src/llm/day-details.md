# 日付詳細ツール

## `day_details`

指定した日付の詳細情報を取得します。曜日（日本語）、日本の祝日情報、およびその日のスケジュールを返します。

```json
{
  "dates": ["today", "tomorrow", "2026-07-28"],
  "include_schedule": true
}
```

| パラメータ | 型 | 説明 |
|------------|-----|------|
| `dates` | array of string | 日付表現。`YYYY-MM-DD`、`today`、`tomorrow`、相対表現（`3d`、`-3d`）が使用可能 |
| `include_schedule` | boolean | `true` の場合、指定日のスケジュールも返す（デフォルト `false`） |

## 返り値

各日付ごとに以下のオブジェクトを含む配列。

```json
[
  {
    "date": "2026-07-27",
    "weekday": "月",
    "is_holiday": false,
    "schedule": [
      {
        "task_id": "t1",
        "title": "朝のランニング",
        "start_at": "2026-07-27T06:00:00+09:00",
        "end_at": "2026-07-27T06:30:00+09:00",
        "status": "scheduled"
      }
    ]
  }
]
```

| フィールド | 説明 |
|------------|------|
| `date` | ISO 8601 日付 |
| `weekday` | 曜日（`月` 〜 `日`） |
| `is_holiday` | 日本の祝日の場合 `true` |
| `holiday_name` | `is_holiday` が `true` の場合に含まれる祝日名 |
| `schedule` | `include_schedule=true` の場合のみ含まれるスケジュール配列 |

## 使用例

```json
{
  "dates": ["today", "7d"],
  "include_schedule": true
}
```

このツールは、ユーザーが「明日は何の予定がある？」「来週の月曜は祝日？」のような質問をしたときに利用します。
