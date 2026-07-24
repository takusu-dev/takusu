# ソースからビルドする

## 前提条件

- Nix（パッケージマネージャ）
- Git

## Nix 開発環境

```sh
git clone https://github.com/takusu-dev/takusu
cd takusu
nix develop
```

`direnv` を使う場合:

```sh
direnv allow
```

## 型チェック

```sh
cargo check
```

## テスト

```sh
cargo nextest run --workspace
```

## ビルド

### ローカルサーバー

```sh
cargo build -p takusu-local --release
```

成果物は `target/release/takusu-local` です。

### CLI

```sh
cargo build -p takusu-cli --release
```

成果物は `target/release/takusu` です。

### モバイル

```sh
# 開発ビルド
TAKUSU_BUILD_VARIANT=dev nix develop --command bash -c \
  "cd mobile && npx expo run:android --device --variant debug"

# リリース APK
nix run .#build-android-apk
```

## Nix ビルド

```sh
nix build .#takusu-local
nix build .#takusu-cli
```

## 注意

- リリースバイナリには自分の Sentry DSN や API キーが含まれないよう注意してください。
- `.env` ファイルや `config.toml` は Git に含めないでください。
