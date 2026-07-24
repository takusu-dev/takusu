# ローカルサーバーを動かす

## 起動

```sh
TAKUSU_JWT_SECRET=<random-secret> \
TAKUSU_ROOT_TOKEN=tsk_$(python3 -c "import uuid; print(uuid.uuid7())") \
TAKUSU_DB=sqlite:./takusu.db \
TAKUSU_BIND=127.0.0.1:3000 \
cargo run -p takusu-local
```

## 必要な環境変数

| 変数 | 役割 | 備考 |
|------|------|------|
| `TAKUSU_JWT_SECRET` | JWT 署名・検証 | 必須 |
| `TAKUSU_ROOT_TOKEN` | ルート認証トークン | `tsk_` + UUID v7。`uuidgen` は通常 v4 を生成するため、v7 を明示的に生成してください |
| `TAKUSU_DB` | SQLite ファイルパス | デフォルト `sqlite:./takusu.db` |
| `TAKUSU_BIND` | 待ち受けアドレス | デフォルト `127.0.0.1:3000` |

## ストレージ

`TAKUSU_STORAGE` でストレージバックエンドを選択します。

- `sqlite`（デフォルト）
- `workers` / `cloudflare` / `d1`（Cloudflare Worker 経由）

## バックグラウンド運用

systemd サービスや `tmux` / `screen` で常時起動しておくのが一般的です。

## 更新

```sh
git pull
cargo build -p takusu-local --release
# サービスを再起動
```

マイグレーションはアプリケーション起動時に組み込み SQL マイグレーションとして自動的に適用されます。
