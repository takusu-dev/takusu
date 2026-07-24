# RRULE ツール

## `expand_rrule`

RFC 5545 形式の RRULE を指定回数分展開します。時刻は `DTSTART` に含め、BYHOUR/BYMINUTE はサポートされていません。

```json
{
  "rrule": "DTSTART:20260727T090000Z\nRRULE:FREQ=DAILY;COUNT=4;BYDAY=MO,TU,WE,TH,FR",
  "count": 10
}
```

| パラメータ | 型 | 説明 |
|------------|-----|------|
| `rrule` | string | RFC 5545 形式の DTSTART + RRULE（`EXDATE` 行も可） |
| `count` | integer | 取得する日時数（1 〜 1000） |

対応:

- `DTSTART`（日付または日時）
- `RRULE` 内: `FREQ`, `INTERVAL`, `COUNT`, `UNTIL`, `BYDAY`, `BYMONTH`, `BYMONTHDAY`
- `EXDATE`

非対応:

- `BYHOUR`, `BYMINUTE`, `BYSECOND`, `BYSETPOS` 等

返り値:

ISO 8601 形式の日時文字列配列。

## 使用例

```json
{
  "rrule": "DTSTART:20260727T090000Z\nRRULE:FREQ=WEEKLY;BYDAY=MO,WE,FR;COUNT=5",
  "count": 5
}
```

```json
[
  "2026-07-27T09:00:00+00:00",
  "2026-07-29T09:00:00+00:00",
  "2026-07-31T09:00:00+00:00",
  "2026-08-03T09:00:00+00:00",
  "2026-08-05T09:00:00+00:00"
]
```
