# タスクを管理する

## タスクを作成する

```sh
cargo run -p takusu-cli -- task create \
  --title "レポートを書く" \
  --end-at "2026-07-28T18:00:00" \
  --avg-time 2h \
  --sigma-time 30m
```

## タスクを更新する

```sh
cargo run -p takusu-cli -- task update <id> --status in_progress
```

## タスクを編集する

```sh
cargo run -p takusu-cli -- task edit <id>
```

`$EDITOR` で開かれたファイルを保存すると、CLI がパースして PATCH リクエストを送信します。

## タスクを削除する

```sh
cargo run -p takusu-cli -- task delete <id>
```

## タスクの状態

| 状態 | 説明 |
|------|------|
| `pending` | まだスケジュールされていない |
| `scheduled` | スケジュール済み |
| `in_progress` | 作業中 |
| `completed` | 完了 |
| `skipped` | スキップ |

## 検索

CLI では `--status` 等でフィルタできます。詳細な検索修飾子は [検索修飾子](../llm/search-qualifiers.md) を参照してください。

## 注意点

- `schedule generate` には `pending` と `scheduled` のタスクのみが含まれます。
- `completed` や `skipped` のタスクはスケジュールに含まれません。
