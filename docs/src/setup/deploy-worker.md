# Cloudflare Worker にデプロイする

`takusu-worker` は Rust/WASM で書かれた Cloudflare Worker です。D1（SQLite 互換）をストレージに使います。

## 前提条件

- `wrangler` CLI
- Cloudflare アカウント
- D1 データベース

## D1 データベース作成

```sh
wrangler d1 create takusu
```

`crates/takusu-worker/wrangler.toml` の `database_id` を作成したデータベース UUID に更新します。

## 必須シークレット

```sh
cd crates/takusu-worker
wrangler secret put TAKUSU_JWT_SECRET
```

## 任意設定

`wrangler.toml` の `[vars]` または `--var` フラグで設定:

```toml
TAKUSU_ALLOWED_ORIGIN = "https://app.example.com"
TAKUSU_LOG = "debug"
```

`TAKUSU_ALLOWED_ORIGIN` は空白区切りで許可 origin を制限します。

## ビルド・マイグレーション・デプロイ

```sh
cd crates/takusu-worker

# 初回または未インストールの場合
# cargo install worker-build

worker-build --release
wrangler d1 migrations apply takusu --remote
wrangler deploy
```

## ローカルサーバーから Worker を使う

```sh
TAKUSU_STORAGE=workers \
TAKUSU_WORKERS_URL=https://takusu-worker.xxx.workers.dev \
TAKUSU_WORKERS_TOKEN=<worker-token> \
cargo run -p takusu-local
```

## GitHub Actions による自動デプロイ

`.github/workflows/release.yaml` の `deploy-worker` ジョブを参照してください。
