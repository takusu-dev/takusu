# Google Calendar 連携の設定

## 1. Google Cloud Console で OAuth2 クライアントを作成

1. [Google Cloud Console](https://console.cloud.google.com/) にアクセス
2. プロジェクトを作成または選択
3. 「API とサービス」→「ライブラリ」→ Google Calendar API を有効化
4. 「認証情報」→「OAuth クライアント ID」を作成
5. アプリケーションタイプは「デスクトップアプリ」を選択
6. クライアント ID とクライアントシークレットをメモ

## 2. CLI で認証

```sh
cargo run -p takusu-cli -- sync login \
  --client-id <CLIENT_ID> \
  --client-secret <CLIENT_SECRET>
```

ブラウザが開き、Google 認証後にローカルコールバックサーバーが認可コードを受け取ります。CLI は refresh_token を取得し、DB に保存します。

## 3. 既存の refresh_token を設定

```sh
cargo run -p takusu-cli -- sync setup --refresh-token <REFRESH_TOKEN>
```

## 4. 同期を実行

```sh
cargo run -p takusu-cli -- sync trigger
```

`schedule generate` / `schedule move` / `schedule clear` 実行時にも自動的に同期されます。

## 注意

- モバイルアプリでは OAuth2 フローは実行しません。CLI で取得した refresh_token を共有ストレージから読み取ります。
- Google Calendar 側で削除したイベントは takusu 側には反映されません。
- `takusu sync delete-all` で Google Calendar 側の takusu イベントをすべて削除できます。
