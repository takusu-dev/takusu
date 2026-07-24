# 習慣を管理する

## 習慣を作成する

`--recurrence` には `takusu_habit::RecurrenceRule` の JSON 表現を渡します。時刻は `--start-time` / `--end-time` で指定します。

```sh
cargo run -p takusu-cli -- habit create \
  --title "朝のランニング" \
  --recurrence '{"freq":"daily","interval":1,"by_day":[],"by_month":[],"by_month_day":[],"count":null,"exdates":[]}' \
  --start-time "06:00" \
  --end-time "06:30" \
  --avg-time 30m \
  --sigma-time 10m
```

## 習慣を編集・削除する

```sh
cargo run -p takusu-cli -- habit edit <id>
cargo run -p takusu-cli -- habit update <id> --recurrence '{"freq":"weekly","interval":1,"by_day":[{"weekday":"mon"},{"weekday":"wed"},{"weekday":"fri"}],"by_month":[],"by_month_day":[],"count":null,"exdates":[]}'
cargo run -p takusu-cli -- habit delete <id>
```

## 習慣からタスクへの展開

`schedule generate` を実行すると、RecurrenceRule に基づいてタスクが生成されます。生成されたタスクは `habit_id` を持ち、元の習慣と紐づきます。

## RecurrenceRule JSON の例

| 目的 | JSON |
|------|------|
| 毎日 | `{"freq":"daily","interval":1,"by_day":[],"by_month":[],"by_month_day":[],"count":null,"exdates":[]}` |
| 毎週月・水・金 | `{"freq":"weekly","interval":1,"by_day":[{"weekday":"mon"},{"weekday":"wed"},{"weekday":"fri"}],"by_month":[],"by_month_day":[],"count":null,"exdates":[]}` |
| 毎月第 2 月曜 | `{"freq":"monthly","interval":1,"by_day":[{"n":2,"weekday":"mon"}],"by_month":[],"by_month_day":[],"count":null,"exdates":[]}` |
| 2 週間ごと月曜 | `{"freq":"weekly","interval":2,"by_day":[{"weekday":"mon"}],"by_month":[],"by_month_day":[],"count":null,"exdates":[]}` |
| 5 回だけ | `count` に `5` を設定 |

フィールド:

- `freq`: `daily`, `weekly`, `monthly`, `yearly`
- `interval`: 間隔（1 以上）
- `by_day`: 曜日指定（`{"weekday":"mon"}` または `{"n":2,"weekday":"mon"}`）
- `by_month`: 対象月（1〜12）
- `by_month_day`: 対象日（1〜31、`-1` は月末）
- `count`: 生成回数（`null` なら無制限）
- `exdates`: 除外日（`["2026-07-28"]` など）

## 生成後のタスクを調整する

習慣から生成されたタスクを個別に編集できます。ただし、`status` を `in_progress`, `completed`, `skipped` に変更したタスクは、次回生成時に維持されます。
