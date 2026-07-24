# Sentry と Secret の扱い

## 注意

提供されているビルド成果物（AppImage や APK など）には、ビルドした人の Sentry DSN や API キーが埋め込まれている可能性があります。自分の環境で運用する場合は **自分でソースからビルド** してください。

## Sentry

Sentry DSN は `SENTRY_DSN` 環境変数で設定します。

```sh
SENTRY_DSN=https://xxx@yyy.ingest.sentry.io/zzz cargo build -p takusu-local --release
```

モバイルでは `EXPO_PUBLIC_SENTRY_DSN` を使用します。

## API キー

API キーは `.env` や `config.toml`、環境変数で設定してください。リポジトリにコミットしないよう注意してください。

```sh
# .envrc.example
eval "$(direnv-export-dotenv)"
```

## トークン

`takusu-local` のトークンは SHA-256 ハッシュで保存されます。発行時にしか平文は表示されないので、必ず控えを保管してください。

## 推奨

- `.env` ファイルを `.gitignore` に追加
- `TAKUSU_JWT_SECRET` は 32 文字以上のランダム文字列を使用
- `TAKUSU_ROOT_TOKEN` は `tsk_` + UUID v7 の形式を使用
