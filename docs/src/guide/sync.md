# カレンダーと同期する

## Google Calendar 同期

### 1. Google Cloud Console で OAuth2 クライアントを作成

1. [Google Cloud Console](https://console.cloud.google.com/) でプロジェクトを作成
2. 「API とサービス」→「認証情報」→「OAuth クライアント ID」を作成
3. アプリケーションタイプは「デスクトップアプリ」を選択
4. クライアント ID とクライアントシークレットを取得

### 2. CLI で認証

```sh
cargo run -p takusu-cli -- sync login --client-id <ID> --client-secret <SECRET>
```

ブラウザが開き、認証後に refresh_token が取得されます。これは `tokens` テーブルに保存されます。

### 3. 同期を実行

```sh
cargo run -p takusu-cli -- sync trigger
```

または `schedule generate` / `schedule move` 実行時に自動的に同期されます。

## iCalendar インポート

iCalendar ファイルをインポートして、固定予定としてタスクを登録できます。

```sh
# ファイルパスを指定
cargo run -p takusu-cli -- task import-ical calendar.ics

# 標準入力から読み込む
cargo run -p takusu-cli -- task import-ical - < calendar.ics
```

## 注意点

- 同期は同期的に実行されます。失敗した場合は次回同期で回収されます。
- Google Calendar 側でイベントを削除しても、takusu 側には反映されません。
- `sync delete-all` を使うと、Google Calendar 側の takusu 同期イベントをすべて削除できます。

詳細は [Google Calendar 設定](../setup/google-calendar.md) を参照してください。
