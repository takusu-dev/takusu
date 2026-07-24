# セットアップ

## 前提条件

- Nix 開発環境
- Android SDK（`nix develop` で提供）
- 実機またはエミュレータ

## 開発ビルド

```sh
cd mobile
TAKUSU_BUILD_VARIANT=dev nix develop --command bash -c \
  "npx expo run:android --device --variant debug"
```

## リリース APK ビルド

```sh
nix run .#build-android-apk
```

開発ビルドとエミュレータ用ビルドは、それぞれ `.#build-android-apk-dev` / `.#build-android-apk-emulator` を使います。

## サーバー接続

モバイルアプリは `takusu-local` または Cloudflare Worker に接続します。設定画面で endpoint と token を入力してください。
