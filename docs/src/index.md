# takusu ドキュメント

**takusu** は、ユーザーのタスクと予定からスケジュールを自動構築するプランナーと、LLM を UI として使う音声アシスタントです。

このドキュメントは **自分で 1 ユーザー 1 デプロイ** を前提としたエンドユーザー向けガイドです。ビルド方法、デプロイ手順、操作の仕方、音声アシスタントの使い方を順を追って説明します。

## 主な機能

- 締切・見積り・依存関係・並列性・諦めやすさを考慮した自動スケジューリング
- 焼きなまし法 (SA) と Priority/ALNS Solver の切り替え
- REST API サーバー (`takusu-local`) と Cloudflare Worker (`takusu-worker`)
- CLI クライアント (`takusu-cli`)
- Expo / React Native のモバイルアプリ
- 音声アシスタント: 録音 + STT + TTS
- iCalendar インポート・Google Calendar 同期
- 習慣の RecurrenceRule（JSON）展開

## 構成

takusu は次のクレート・アプリで構成されています。

- `takusu-core`: スケジューリングエンジン
- `takusu-local`: ローカル REST API サーバー (axum + SQLite)
- `takusu-local-lib`: ビジネスロジック
- `takusu-client`: HTTP クライアントライブラリ
- `takusu-cli`: CLI クライアント
- `takusu-worker`: Cloudflare Worker (Rust/WASM + D1)
- `takusu-audio`: 録音・STT・TTS ライブラリ
- `takusu-audio-cli`: 音声機能の内部テスト用 CLI（エンドユーザー向けではありません）
- `mobile/`: Expo / React Native アプリ
- `google-cal`: Google Calendar API クライアント

## 次のステップ

- [クイックスタート](quick-start.md) — 最短で動かす
- [コンセプト](concepts/index.md) — タスク・習慣・スケジュールの考え方
- [セットアップ](setup/index.md) — ビルドからデプロイまで
