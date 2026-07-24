# LLM / Agent 向け

このセクションは、takusu-agent やその他の LLM コーディングエージェントが takusu の機能を正しく使えるようにするためのリファレンスです。

## ページ一覧

- [概要](overview.md)
- [タスク関連ツール](task-tools.md)
- [スケジュール関連ツール](schedule-tools.md)
- [習慣関連ツール](habit-tools.md)
- [進捗関連ツール](progress-tools.md)
- [記憶関連ツール](memory-tools.md)
- [スキル関連ツール](skills-tools.md)
- [RRULE ツール](rrule-tools.md)
- [日付詳細ツール](day-details.md)
- [検索修飾子](search-qualifiers.md)
- [ユーザー入力ツール](user-input-tools.md)

## 基本原則

1. 永続的な変更はすべて承認を経て確定する。
2. ツール呼び出しは `tool`/`argument` 形式で行う。
3. タスク参照は `#<display_id>` または `h<habit_display_id>#<task_display_id>` を使用する。
4. 日時はサーバータイムゾーンで解釈される。
