# takusu

ユーザーのスケジュールを自動構築するプランナーと、LLM 音声アシスタント。

## 特徴

- 締め切り・見積り・依存関係・並列性・諦めやすさを考慮した自動スケジューリング
- 焼きなまし法 (SA) + LNS + Tabu Search で最適化（並列リスタート・部分再スケジューリング対応）
- REST API サーバー (axum + SQLite) と Cloudflare Worker バックエンド
- CLI クライアント（エディタ編集・リッチテーブル表示）
- モバイルアプリ（Expo + React Native、UniFFI ネイティブモジュール）
- 音声アシスタント: FunASR (SenseVoice-Small) で STT、Irodori-TTS で TTS
- iCalendar インポート・Google Calendar 同期・習慣の RRULE 展開

## 構成

```
takusu/
├── crates/
│   ├── takusu-core/        # プランナー本体（データ型・スケジューリング算法）
│   ├── takusu-local/       # ローカルサーバー (axum + SQLite)
│   ├── takusu-local-lib/   # ビジネスロジック（サーバーと CLI の共通）
│   ├── takusu-storage/     # プラガブル Storage trait + 共有型
│   ├── takusu-ical/        # iCalendar パーサー
│   ├── takusu-habit/       # RRULE 展開エンジン
│   ├── takusu-audio/       # 録音 + STT/TTS バックエンド
│   ├── takusu-audio-cli/   # 音声 CLI
│   ├── takusu-client/      # REST API クライアントライブラリ
│   ├── takusu-cli/         # CLI クライアント (clap)
│   ├── takusu-worker/      # Cloudflare Worker (Rust/WASM + D1)
│   ├── takusu-android/     # Android ネイティブ (UniFFI Kotlin バインディング)
│   ├── takusu-util/        # 共有ユーティリティ
│   └── google-cal/         # Google Calendar API クライアント (OAuth2)
├── funasr_server/          # FunASR STT WebSocket サーバー (Python)
├── mobile/                 # Expo / React Native アプリ
├── scripts/                # ビルド・サーバー起動スクリプト
└── main.typ                # 設計ドキュメント (Typst・日本語)
```

## セットアップ

```sh
nix develop   # または direnv allow
```

## コマンド

```sh
cargo check                              # 型チェック
cargo nextest run --workspace            # 全テスト
cargo nextest run -p takusu-core         # コアプランナーのテスト
cargo bench -p takusu-core               # ベンチマーク
cargo run --example daily                # サンプル実行
cargo run -p takusu-cli -- --help        # CLI クライアント
cargo run -p takusu-local                # ローカルサーバー起動
cargo run -p takusu-audio-cli -- speak --text "こんにちは"  # TTS
```

### 音声サーバー

```sh
cd funasr_server && uv run python -m funasr_server   # FunASR STT
./scripts/irodori-tts-server.sh                       # Irodori-TTS
```

### モバイルアプリ

```sh
# APK ビルド
nix develop --command bash -c "./scripts/build-android.sh"
nix develop --command bash -c "cd mobile && npx expo prebuild --platform android --no-install"
./scripts/post-prebuild-android.sh mobile/android
nix develop --command bash -c "cd mobile/android && ./gradlew :app:assembleRelease"

# 開発ビルド（実機デバッグ・Metro ホットリロード）
cd mobile
TAKUSU_BUILD_VARIANT=dev nix develop --command bash -c \
  "npx expo run:android --device --variant debug"
```

詳細は [`mobile/AGENTS.md`](mobile/AGENTS.md) を参照。

## 使い方

```sh
TAKUSU_ROOT_TOKEN=tsk_... cargo run -p takusu-local
```

## 設計ドキュメント

[`main.typ`](main.typ) — プロジェクト全体の設計思想 (Typst・日本語)
[`AGENTS.md`](AGENTS.md) — 開発者向けガイド・コマンドリファレンス

## License

MIT
