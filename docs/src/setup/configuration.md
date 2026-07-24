# 設定ファイルと環境変数

## 環境変数

| 変数 | 必須 | デフォルト | 説明 |
|------|------|------------|------|
| `TAKUSU_JWT_SECRET` | SQLite 時 | - | JWT 署名・検証用シークレット |
| `TAKUSU_STORAGE` | - | `sqlite` | `sqlite` / `workers` / `cloudflare` / `d1` |
| `TAKUSU_DB` | - | `sqlite:./takusu.db` | SQLite ファイルパス |
| `TAKUSU_BIND` | - | `127.0.0.1:3000` | 待ち受けアドレス |
| `TAKUSU_WORKERS_URL` | Worker 時 | - | Worker エンドポイント URL |
| `TAKUSU_WORKERS_TOKEN` | Worker 時 | `TAKUSU_ROOT_TOKEN` | Worker 認証トークン |
| `TAKUSU_ROOT_TOKEN` | - | - | ルートトークン |
| `CARTESIA_API_KEY` | TTS 時 | - | Cartesia Sonic API キー |
| `SENTRY_DSN` | - | - | Sentry DSN |

## `takusu-local` 設定

### Nix 起動

`takusu-local` はコマンドライン引数を解析しません。待ち受けアドレスは `TAKUSU_BIND` 環境変数で指定します。

```sh
TAKUSU_BIND=127.0.0.1:3000 nix run .#takusu-local
```

### CLI 設定ファイル

`$XDG_CONFIG_HOME/takusu/config.toml`:

```toml
[server]
url = "http://127.0.0.1:3000"
token = "tsk_..."

[display]
mode = "rich"
```

## Solver 設定

Solver は `Planner` の設定または環境変数で切り替えできます。

| 値 | 説明 |
|----|------|
| `sa` | Simulated Annealing + LNS + Tabu Search |
| `priority` | Priority decoder + ALNS |
| `auto` | priority を試し、失敗時に sa へフォールバック |

## 睡眠時間設定

`settings` API または CLI / モバイル設定画面で、就寝時刻と起床時刻を設定できます。Solver は睡眠時間を避けてスケジュールを作成します。
