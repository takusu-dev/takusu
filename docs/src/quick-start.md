# クイックスタート

このページでは、Nix 開発環境を使って `takusu-local` を起動し、CLI で最初のスケジュールを生成するまでの手順を説明します。

## 前提条件

- [Nix](https://nixos.org/download/) がインストールされていること
- `direnv` を使う場合は `.envrc` を許可済みであること

## 1. 開発環境に入る

```sh
nix develop
# または
 direnv allow
```

## 2. ローカルサーバーを起動する

```sh
TAKUSU_ROOT_TOKEN=tsk_$(python3 -c "import uuid; print(uuid.uuid7())") cargo run -p takusu-local
```

`TAKUSU_ROOT_TOKEN` は `tsk_` プレフィックス + UUID v7 の形式です。`uuidgen` コマンドは通常 UUID v4 を生成するため、明示的に v7 を生成するようにしています。サーバーはデフォルトで `http://127.0.0.1:3000` で待ち受けます。

## 3. CLI でタスクを追加する

別のターミナルで開発シェルに入り、CLI を使います。

```sh
# タスクを作成
cargo run -p takusu-cli -- task create \
  --title "レポートを書く" \
  --end-at "2026-07-28T18:00:00" \
  --avg-time 2h \
  --sigma-time 30m

# 習慣を作成（recurrence は JSON 形式の RecurrenceRule）
cargo run -p takusu-cli -- habit create \
  --title "朝のランニング" \
  --recurrence '{"freq":"daily","interval":1,"by_day":[],"by_month":[],"by_month_day":[],"count":null,"exdates":[]}' \
  --start-time "06:00" \
  --end-time "06:30" \
  --avg-time 30m
```

## 4. スケジュールを生成する

```sh
cargo run -p takusu-cli -- schedule generate
```

このコマンドは `takusu-local` に対してリクエストを送信し、登録済みのタスクと習慣からスケジュールを生成します。結果は CLI のテーブル表示か、`takusu-local` の SQLite データベースに保存されます。

## 5. スケジュールを確認する

```sh
cargo run -p takusu-cli -- schedule get
```

## 次のステップ

- [タスクの概念](concepts/task.md)
- [セットアップ詳細](setup/index.md)
- [CLI ガイド](guide/cli.md)
