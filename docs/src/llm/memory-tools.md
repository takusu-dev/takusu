# 記憶関連ツール

takusu-agent はユーザーの固有名詞、事実、タスクノートを `memory` として保存できます。

## `memory_search`

記憶をキーワード・kind で検索します。

```json
{
  "q": "研究室 大学",
  "kind": "proper_noun,fact",
  "limit": 10
}
```

| パラメータ | 型 | 説明 |
|------------|-----|------|
| `q` | string | 検索クエリ（複数キーワードは AND） |
| `kind` | string | `proper_noun`, `fact`, `task_note`（カンマ区切り OR） |
| `subject_type` | string | 対象タイプ |
| `subject_id` | string | 対象 ID |
| `limit` | integer | 最大件数 |

## `similar_tasks`

タイトルが類似する完了済みタスクを探します。所要時間の推定に利用します。

```json
{
  "title": "レポート",
  "limit": 10
}
```

## `memory_save`

記憶の保存を提案します。承認後に確定します。

```json
{
  "kind": "proper_noun",
  "key": "研究室",
  "content": "大学の研究室。A棟 3 階。",
  "why": "ユーザーが口頭で説明"
}
```

| パラメータ | 型 | 説明 |
|------------|-----|------|
| `kind` | string | `proper_noun`, `fact`, `task_note` |
| `key` | string | 短い識別子 |
| `content` | string | 詳細内容 |
| `subject_type` | string | オプション。`task_note` の場合 `task` |
| `subject_id` | string | オプション。対象タスク ID |
| `why`, `warnings`, `inferred_fields` | | 承認用メタ情報 |

## `memory_update`

既存記憶の更新を提案します。

必須:

- `memory_ref`
- `observed_revision`
- `content`

## `memory_delete`

記憶の削除を提案します。

必須:

- `memory_ref`
- `observed_revision`
