# コンセプト

takusu を使う上で重要な概念を整理します。

- [タスク](task.md): やること。締切・見積り・依存関係・並列性・諦めやすさを持つ。
- [習慣](habit.md): 繰り返し発生する予定。`RecurrenceRule`（JSON）で表現する。
- [スケジュール](schedule.md): タスクや習慣を時間軸に並べた結果。
- [Solver](solver.md): スケジュールを作る計算エンジン。
- [カレンダー連携](calendar-sync.md): Google Calendar や iCal との連携。
- [音声アシスタント](voice-assistant.md): STT/TTS を使った LLM アシスタント。

全体として、takusu は「表現力のあるタスクモデル」と「自動スケジューリング」、そして「音声で手軽に操作する UI」で構成されています。
