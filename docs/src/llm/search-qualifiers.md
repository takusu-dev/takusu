# 検索修飾子

`list_tasks` や `memory_search` の検索クエリで使える修飾子です。

## ブール構文

- `AND` または 空白: 両方の用語に一致
- `OR`: どちらかに一致
- `-` または `NOT`: 除外
- `()`: グループ化

## タスク検索修飾子

| 修飾子 | 例 | 説明 |
|--------|-----|------|
| `status:` | `status:pending` | 状態で絞り込み |
| `title:` | `title:レポート` | タイトルに含まれる |
| `desc:` | `desc:締切` | 説明に含まれる |
| `start:` | `start:2026-07-25` | 開始時刻 |
| `end:` | `end:tomorrow` | 締切 |
| `scheduled-start:` | `scheduled-start:today` | スケジュール開始 |
| `scheduled-end:` | `scheduled-end:2026-07-30` | スケジュール終了 |
| `from:` | `from:2026-07-25` | `end:>=` のエイリアス |
| `until:` | `until:tomorrow` | `start:<=` のエイリアス |
| `habit:` | `habit:h1` | 習慣に紐づくタスク |
| `depends:` | `depends:#42` | 指定タスクに依存する |
| `dependents:` | `dependents:#42` | 指定タスクを依存に持つ |
| `deps_count:` | `deps_count:>0` | 依存数で絞り込み |
| `is:` | `is:overdue` | フラグによる絞り込み |
| `has:` | `has:description` | 指定フィールドを持つ |

### `status` の値

- `pending`
- `scheduled`
- `in_progress`
- `completed`
- `skipped`
- `overdue`

### `is:` の値

- `fixed`
- `parallelizable`
- `allows_parallel`
- `overdue`

### `has:` の値

- `description`
- `completed_at`
- `schedule`
- `depends`

## 日付表現

- `YYYY-MM-DD`
- `today`, `tomorrow`, `yesterday`
- `Nd`（相対日数）
- 演算子: `>=2026-07-25`, `<2026-07-30`

## 例

```text
status:pending 買い物
end:today OR end:tomorrow
-habit:h1
depends:#42
deps_count:>0
研究室 大学
研究*大学
```

## 記憶検索構文

- 複数キーワードは AND
- `*` は任意の文字列にマッチ
- `kind` はカンマ区切り OR: `proper_noun,fact,task_note`

```text
kind=proper_noun,fact
研究*大学
```
