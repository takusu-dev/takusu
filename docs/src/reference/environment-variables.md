# 環境変数一覧

## `takusu-local`

| 変数 | 必須 | デフォルト | 説明 |
|------|------|------------|------|
| `TAKUSU_JWT_SECRET` | SQLite 時 | - | JWT 署名・検証用シークレット |
| `TAKUSU_STORAGE` | - | `sqlite` | ストレージバックエンド（`sqlite`、`workers`、`cloudflare`、`d1`） |
| `TAKUSU_DB` | - | `sqlite:./takusu.db` | SQLite ファイルパス |
| `TAKUSU_BIND` | - | `127.0.0.1:3000` | 待ち受けアドレス |
| `TAKUSU_WORKERS_URL` | Worker 時 | - | Worker エンドポイント。`TAKUSU_WORKER_URL` の上位互換 |
| `TAKUSU_WORKER_URL` | Worker 時 | - | 旧称（`TAKUSU_WORKERS_URL` が優先） |
| `TAKUSU_WORKERS_TOKEN` | Worker 時 | `TAKUSU_ROOT_TOKEN` | Worker 認証トークン |
| `TAKUSU_ROOT_TOKEN` | - | - | ルートトークン。未設定でも `TAKUSU_WORKERS_TOKEN` があればフォールバック |
| `SENTRY_DSN` | - | - | Sentry DSN |

## `takusu-cli`

CLI は設定ファイル `$XDG_CONFIG_HOME/takusu/config.toml`（`XDG_CONFIG_HOME` 未設定時は `$HOME/.config`）を読みます。設定ファイルの各項目は同名の環境変数で上書きできます。

| 変数 | ファイル内キー | 説明 |
|------|----------------|------|
| `XDG_CONFIG_HOME` | - | 設定ファイル保存先のベースディレクトリ |
| `EDITOR` | - | `task edit`、`habit edit` 等で使うエディタ（未設定時は `vi`） |
| `TAKUSU_TIMEZONE` | `tz` | 表示・パースで使用するタイムゾーン（例: `Asia/Tokyo`） |
| `TAKUSU_STORAGE` | `storage` | `sqlite` または `workers` |
| `TAKUSU_DB` | `db` | SQLite ファイルパス。`sqlite:./takusu.db` 等 |
| `TAKUSU_WORKERS_URL` | `worker_url` | Worker エンドポイント。`TAKUSU_WORKER_URL` の上位互換 |
| `TAKUSU_WORKER_URL` | `url` | 旧称（`TAKUSU_WORKERS_URL` が優先） |
| `TAKUSU_WORKERS_TOKEN` | `workers_token` / `token` | Worker 認証トークン |
| `TAKUSU_ROOT_TOKEN` | `root_token` | ルートトークン |
| `TAKUSU_JWT_SECRET` | `jwt_secret` | `gen-root-token` 時に使用 |

`gen-root-token` では `TAKUSU_JWT_SECRET`（または config の `jwt_secret`）が必須です。

## `takusu-worker`（Cloudflare Worker）

| 変数 | 種別 | 説明 |
|------|------|------|
| `TAKUSU_JWT_SECRET` | Secret | JWT 署名・検証 |
| `TAKUSU_ALLOWED_ORIGIN` | Var | 許可 origin（空白区切り） |
| `TAKUSU_LOG` | Var | ログレベル（例: `debug`） |

## `takusu-audio` / モバイル音声

| 変数 | 説明 |
|------|------|
| `CARTESIA_API_KEY` | Cartesia Sonic TTS API キー |

Sherpa-ONNX モデルパスは、モバイルでは設定画面または初回ダウンロード、`takusu-audio-cli` ではコマンドライン引数で指定します。環境変数は使用しません。

## モバイル（Expo / React Native）

| 変数 | 説明 |
|------|------|
| `EXPO_PUBLIC_SENTRY_DSN` | モバイル Sentry DSN |
| `TAKUSU_BUILD_VARIANT` | `dev`（開発ビルドと stable 共存）または未設定（リリースビルド） |
| `TAKUSU_ANDROID_ABIS` | ビルド対象 ABI（エミュレータ等で `x86_64` を指定） |
