# スキル関連ツール

スキルは、エージェントの動作をカスタマイズするためのマークダウン形式の指示書です。TOML front matter + 本文で構成されます。

```markdown
+++
name = "weekly-review"
description = "Run a weekly review to clean up stale tasks and plan the next week."
+++

Run the weekly review by:
1. ...
```

## `skills_list`

利用可能なスキル（built-in + ユーザー定義）を一覧取得します。引数は不要です。

## `skills_read`

スキルの本文を含む詳細を取得します。

```json
{
  "slug": "weekly-review"
}
```

| パラメータ | 型 | 説明 |
|------------|-----|------|
| `slug` | string | スキル slug |

## `skills_propose_add`

新しいスキルの追加を提案します。

```json
{
  "slug": "daily-check",
  "name": "Daily check",
  "description": "Check today's schedule every morning.",
  "body": "1. List pending tasks.\n2. ...",
  "why": "ユーザーからの要望"
}
```

| パラメータ | 型 | 説明 |
|------------|-----|------|
| `slug` | string | URL-safe 識別子 |
| `name` | string | 表示名 |
| `description` | string | 説明 |
| `body` | string | マークダウン本文 |

## `skills_propose_edit`

既存スキルの編集を提案します。built-in スキルは編集不可です。

必須:

- `slug`

任意:

- `name`, `description`, `body`
