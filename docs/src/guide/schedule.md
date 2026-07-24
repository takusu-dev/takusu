# スケジュールを扱う

## スケジュールを生成する

```sh
cargo run -p takusu-cli -- schedule generate
```

`pending` / `scheduled` のタスクと、習慣から展開されたタスクを対象に、Solver が最適な配置を計算します。

## 現在のスケジュールを確認する

```sh
cargo run -p takusu-cli -- schedule get
```

`rich` モードでは、テーブル形式で表示されます。

## 部分再スケジュール

特定の時間範囲内のタスクだけを再計算します。`--mode` は必須です。`range` を使う場合は `--from` / `--until` も必要です。

```sh
cargo run -p takusu-cli -- schedule reschedule \
  --mode range \
  --from "2026-07-28T09:00:00" \
  --until "2026-07-28T18:00:00"
```

範囲外のタスクは位置が固定されます。

## タスクを手動で移動する

```sh
cargo run -p takusu-cli -- schedule move <task_id> \
  --start-at "2026-07-28T14:00:00"
```

制約違反がある場合は警告が出ます。`--force` を付けると無視して適用できます。

## スケジュールをクリアする

```sh
cargo run -p takusu-cli -- schedule clear
```

`schedule clear` はスケジュールを削除しますが、タスクの `status` は `scheduled` のままです。再度 `schedule generate` するまで `pending` に戻しません。
