# スケジュール

**スケジュール** は、タスクと習慣を時間軸に配置した結果です。`takusu-core` は 5 分単位の離散時間スロットで計算します。

## 5 分スロット

`takusu` では 1 スロット = 5 分です。すべての開始時刻・所要時間は 5 分の倍数に揃えられます。

- 例: 13:00 〜 14:30 のタスクは 18 スロット
- 日付の境界や睡眠時間もスロット単位で処理されます

## スケジュールの生成

```sh
cargo run -p takusu-cli -- schedule generate
```

またはモバイルアプリの「同期」ボタンでも実行できます。

生成フロー:

1. `pending` / `scheduled` のタスクを取得
2. 習慣から展開されたタスクも含める
3. Solver に渡して最適化
4. 結果を `schedules` テーブルに保存（`id = 'active'`）
5. Google Calendar 同期を実行（設定済みの場合）

## 部分再スケジュール

一部のタスクだけを固定し、残りを再計算できます。`--mode` は必須です。

```sh
cargo run -p takusu-cli -- schedule reschedule \
  --mode range \
  --from "2026-07-28T09:00:00" \
  --until "2026-07-28T18:00:00"
```

`--mode` の選択肢:

- `full`: 全再スケジュール
- `range`: 範囲指定。`--from` と `--until` が必要
- `tasks`: 指定タスクのみ。`--task-ids` が必要

## タスクの移動

手動でタスクを移動できます。

```sh
cargo run -p takusu-cli -- schedule move <task_id> \
  --start-at "2026-07-28T14:00:00"
```

制約違反がある場合は警告が表示されます。`--force` で無視して適用できます。

## シングルアクティブスケジュール

`schedules` テーブルには常に 1 つの `id='active'` の行が存在します。スケジュール生成のたびに UPSERT されます。
