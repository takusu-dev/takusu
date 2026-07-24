# Agent 概要

takusu-agent は、LLM が takusu のツールを使ってタスク・スケジュールを操作するためのエージェントです。

## 承認フロー

タスク作成・更新・削除、スケジュール生成・再生成、習慣変更などの **永続的な変更** は、すべて `ProposedChange` としてユーザーに提示されます。ユーザーが承認するまで実際の書き込みは行われません。

```text
LLM: ツール呼び出しで提案生成
→ ToolOutput に proposed_changes を含む
→ UI/音声でユーザーに承認を求める
→ 承認後、実際の API 呼び出しを実行
```

## タスク参照

- `#42`: display_id 42 のタスク
- `h1#3`: habit display_id 1 の中のタスク display_id 3
- 配列で複数指定可能: `["#1", "#2", "h1#3"]`

## 日時表現

多くのツールは次のような日時表現を受け入れます。

- `YYYY-MM-DDTHH:MM:SS`（絶対時刻、タイムゾーン省略時はサーバーTZ）
- `YYYY-MM-DD`（日付）
- `today`, `tomorrow`, `yesterday`
- `7d`, `-3d`（相対日数）
- `now`

## 推測されたフィールド

`inferred_fields` には、曖昧なユーザー入力から LLM が推測したフィールドを記録します。承認 UI で強調表示されます。

## よくあるフロー

### スケジュール確認

```
get_schedule → ユーザーに表示
```

### タスク追加

```
create_task → proposed_change → 承認 → 実行
```

### 作業開始

```
task_start → proposed_change → 承認 → in_progress
```

### 進捗記録

```
task_progress → proposed_change → 承認 → 更新
```

## エラー処理

- `InvalidArgs`: 引数を修正して再試行可能
- `NotFound`: 参照先が存在しない
- `Conflict`: 楽観的ロック競合。再取得して再試行
- `Cancelled`: ユーザーがキャンセル
