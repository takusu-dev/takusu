# CLI

`takusu-cli` は takusu のコマンドラインクライアントです。`takusu-local-lib` を直接使用するため、ネットワーク通信なしで動作します。

## 設定

CLI の設定は `$XDG_CONFIG_HOME/takusu/config.toml` に保存されます。

```toml
[server]
url = "http://127.0.0.1:3000"
token = "tsk_..."

[display]
mode = "rich"  # "rich" または "simple"
```

## 表示モード

- `rich`: `comfy-table` を使ったカラフルなテーブル表示
- `simple`: プレーンテキスト表示

## サブコマンド一覧

### task

```sh
takusu task list
takusu task show <id>
takusu task create --title "買い物" --end-at 2026-07-28T18:00 --avg-time 30m
takusu task edit <id>
takusu task update <id> --status completed
takusu task delete <id>
```

### schedule

```sh
takusu schedule get
takusu schedule generate
takusu schedule reschedule --mode range --from <datetime> --until <datetime>
takusu schedule move <id> --start-at <datetime> [--force]
takusu schedule clear
```

### habit

```sh
takusu habit list
takusu habit create --title "朝ラン" \
  --recurrence '{"freq":"daily","interval":1,"by_day":[],"by_month":[],"by_month_day":[],"count":null,"exdates":[]}' \
  --start-time 06:00 --end-time 06:30 --avg-time 30m
takusu habit edit <id>
takusu habit update <id> --recurrence '{"freq":"weekly","interval":1,"by_day":[{"weekday":"mon"},{"weekday":"wed"},{"weekday":"fri"}],"by_month":[],"by_month_day":[],"count":null,"exdates":[]}'
takusu habit delete <id>
```

### token

```sh
takusu token create
takusu token list
takusu token revoke <id>
```

### sync

```sh
takusu sync settings
takusu sync setup --refresh-token <TOKEN>
takusu sync login --client-id <ID> --client-secret <SECRET>
takusu sync trigger
```

## エディタ編集

`task edit` は一時ファイルにタスクを書き出し、`$EDITOR`（デフォルト `vi`）で編集後、パースして更新します。`#` で始まる行はコメントとして無視されます。

## 所要時間の指定

`--avg-time` / `--sigma-time` は、`-t` や `--time` ではなく `30m`, `1h30m`, `2h` などの長さ表現を受け付けます。`--sigma-time` を省略するか `0` を指定すると、自動的に `avg/5` が使われます。
