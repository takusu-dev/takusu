# takusu mobile (Android) 実装プラン

## Summary

takusu-local-lib (Planner + axum) を Android 上で組み込みサーバーとして動かし、タスク依存を DAG として可視化する Expo (React Native) アプリを作る。ストレージは Workers (HTTP) のみ。グラフ可視化は WebView + Cytoscape.js (dagre)。UIは `design/mobile-ui.md` に基づく。

## 確認済みの仕様

- **pending** = `status="pending"` (status は5種: pending/scheduled/in_progress/completed/skipped)
- **受け皿** = `allows_parallel=true` のタスクを左に 1:3 横幅で表示
- **graph** = 推移的依存すべて表示、完了ノードは灰色
- **graph編集** = 編集モードで切替 (編集中は pan/zoom 無効)
- **同期ボタン** = config で同時/2段階を切り替え
- **task add view** = 依存先タスクを追加できる画面 (deps graph 表示は不要)
- **habit add** = title, 周期, cost, abandonability
- **過ぎた日** = 今日が上端、過去は上に隠れて pull-down-to-reveal
- **undo/redo** = タスクCRUD + スケジュール操作 + habit CRUD (同期は含まない)、50step
- **abandonability** = 0.0/0.25/0.5/0.75/1.0 の5段階スライダー
- **カレンダーoverlay** = マーク付き月表示、日付選択でジャンプ
- **reschedule** = 「選択をreschedule」= 選択だけ再スケ (他はpin)、plan_partial(pinned=選択以外)
- **ブランドカラー** = #7261A3 をアクセントに

## Architecture

```
Android App (Expo / React Native)
├── JS/TS UI Layer
│   ├── Home (task view): 上段(メニュー/search/sync) + 中段(タスクカード時系列) + 下段(add/start&done)
│   ├── Graph view: WebView + Cytoscape.js (dagre, 編集モード切替)
│   ├── Habit view: habitカード一覧 + add
│   ├── Task detail / Task add / Habit detail / Habit add / Settings
│   └── Navigation: 右側面 floating (day/page scroll + calendar), 左側面下 view changer
├── Expo Native Module (Kotlin)
│   ├── System.loadLibrary("takusu_android")
│   └── UniFFI生成Kotlin → start/stop server lifecycle
└── Embedded Rust Server (takusu-android cdylib)
    ├── axum on localhost:PORT
    ├── takusu-local-lib (Planner SAをネイティブ実行)
    └── WorkersStorage (HTTP → Cloudflare Worker)
```

データフロー: JS → fetch(localhost) → axum → TakusuApp → WorkersStorage → Cloudflare Worker

## Phase 0: UIデザイン受け取り (完了)

`design/mobile-ui.md` に記述済み。曖昧点は上記「確認済みの仕様」で解消。

## Phase 1: Rust 側の準備

### 1. reqwest features 整理 (`Cargo.toml`)

- reqwest 0.13 は default-tls=rustls なので、features から `default-tls` を削除
- `features = ["json", "form"]` に変更 (default-features = false は維持)

### 2. takusu-local-lib の sqlite を feature gate 化

- `lib.rs`: `pub mod storage_sqlite;` → `#[cfg(feature = "sqlite")] pub mod storage_sqlite;`
- `Cargo.toml`: `[features] default = ["sqlite"]` 追加、sqlx を optional に
- takusu-local は `features = ["sqlite"]` を明示すれば影響なし

### 3. 新規 crate `takusu-android` (cdylib) を作成

- `Cargo.toml`: `crate-type = ["cdylib", "lib"]`, UniFFI 依存
- `src/lib.rs`: UniFFI interface で `TakusuServer` クラス
  - `start(port, workers_url, token) -> Result<i32>` — tokio runtime + axum serve
  - `stop()` — graceful shutdown
  - `status() -> ServerStatus`
- `src/takusu.udl`: UniFFI IDL
- `build.rs`: uniffi-build
- WorkersStorage のみ使用 (sqlx なし)、takusu-local の router/handlers を再利用
- config は env var ではなく関数パラメータで受け取る

### 4. Nix で Android クロスコンパイル環境を整備

- `rust-toolchain.toml`: `targets` に `aarch64-linux-android`, `armv7-linux-androideabi`, `x86_64-linux-android`, `i686-linux-android` を追加
- `flake.nix` devShells に追加:
  - `cargo-ndk`
  - Android NDK: `androidenv.composeAndroidPackages { includeNDK = true; }`
  - `shellHook` に `export ANDROID_NDK_HOME=...` を追加
- ビルドコマンド: `cargo ndk -t aarch64-linux-android build -p takusu-android --release --no-default-features`

## Phase 2: Expo アプリ

### 5. Expo プロジェクト初期化

- `npx create-expo-app@latest mobile --template blank-typescript`
- cytoscape, dagre, react-native-webview, expo-haptics, react-native-gesture-handler を追加

### 6. Expo Native Module `takusu-server` を作成

- `modules/takusu-server/` に Expo Module
- Kotlin: `System.loadLibrary("takusu_android")` + UniFFI 生成コード
- TS: `startServer(port, url, token)`, `stopServer()`, `serverStatus()`
- `.so` は `jniLibs/<arch>/` に配置 (ビルドスクリプトで自動化)

### 7. HTTP クライアント + 状態管理 (TS)

- `src/api.ts`: fetch で localhost の axum にアクセス
- takusu-client のリクエスト/レスポンス型を TS にポーティング
- undo/redo スタック (50step) の実装 (タスクCRUD + スケジュール操作 + habit CRUD)

### 8. 画面実装 (`design/mobile-ui.md` に基づく)

- `src/views/Home.tsx`: 上段(ハンバーガー/search/sync) + 中段(タスクカード時系列、pending上、日付区切りバー、pull-down-to-reveal過去日) + 下段(add中央/start&done右)
- `src/views/Graph.tsx`: WebView + Cytoscape.js (推移的依存、完了ノード灰色、編集モード切替でedge切断/node間追加)
- `src/views/Habit.tsx`: habitカード一覧 + add
- `src/views/TaskDetail.tsx`: title/time/parallel/cost/abandonability(5段階スライダー)/habit/description/parallel config/deps graph(関係あるものだけ)
- `src/views/TaskAdd.tsx`: タスク追加 + 依存先タスク追加画面
- `src/views/HabitDetail.tsx`: 情報表示 + 直近の生成タスクリスト
- `src/views/HabitAdd.tsx`: title/周期/cost/abandonability
- `src/views/Settings.tsx`: general(dark-white/sync) / worker(endpoint,key) / google cal / info(license,version)
- `src/components/TaskCard.tsx`: 左に開始/終了時刻、中央タイトル、abandonabilityで背景色、右下にcost(avg,sigma)。slide-right=done(弱haptics)、slide-delete(強haptics)。doneは取消線&灰色。allows_parallelなら左に受け皿(1:3)
- `src/components/NavigationButtons.tsx`: 右側面 floating (day/page scroll上下 + calendar overlay マーク付き月表示)
- `src/components/ViewChanger.tsx`: 左側面下 (habit/task/graph 縦並び)
- `src/components/ContextMenu.tsx`: 設定/undo-redo(常時) + 選択時(選択以外reschedule/選択をreschedule/削除/依存とする新規タスク/選択解除)
- ブランドカラー #7261A3 をアクセントに

## Phase 3: ビルド・動作確認

### 9. ビルドスクリプト作成

- `scripts/build-android.sh`: cargo-ndk で .so ビルド → jniLibs に配置 → UniFFI Kotlin 生成 → Expo Module に配置

### 10. エミュレータで動作確認

- `npx expo run:android` でビルド・起動
- サーバー起動 → タスク取得 → グラフ表示の確認

## Phase 4: CI/CD (GitHub Actions)

### 11. CI ワークフロー (`.github/workflows/ci.yml`)

- Rust品質チェック: `cargo check`, `cargo clippy`, `cargo fmt --check`, `cargo nextest run --workspace`
- Android .so ビルド確認: `cargo ndk -t aarch64-linux-android build -p takusu-android --release --no-default-features` (Nixで)
- Expoアプリチェック: typecheck, lint, build
- Nix flake check
- トリガー: push to main, PR

### 12. CD ワークフロー (`.github/workflows/release.yml`)

- タグ打ち (`v*`) でトリガー
- Nix で Android .so ビルド (全arch: aarch64, armv7, x86_64, i686)
- Expo で APK ビルド (`npx expo run:android` or `gradle assembleRelease`)
- GitHub Releases に APK をアップロード

## Files to Modify

- `Cargo.toml` — reqwest features から `default-tls` を削除
- `crates/takusu-local-lib/Cargo.toml` — sqlx を optional に、`[features]` 追加
- `crates/takusu-local-lib/src/lib.rs` — `storage_sqlite` を `#[cfg(feature = "sqlite")]` に
- `crates/takusu-local/Cargo.toml` — `takusu-local-lib` に `features = ["sqlite"]` を明示
- `rust-toolchain.toml` — Android ターゲット追加
- `flake.nix` — devShells に cargo-ndk + Android NDK 追加、shellHook に `ANDROID_NDK_HOME` 設定

## Files to Create

- `crates/takusu-android/Cargo.toml` — cdylib, UniFFI
- `crates/takusu-android/src/lib.rs` — UniFFI interface + server lifecycle
- `crates/takusu-android/src/takusu.udl` — UniFFI IDL
- `crates/takusu-android/build.rs` — uniffi-build
- `mobile/` — Expo プロジェクト全体
- `mobile/modules/takusu-server/` — Expo Native Module (Kotlin + TS)
- `mobile/src/api.ts` — HTTP クライアント
- `mobile/src/views/*.tsx` — 各画面
- `mobile/src/components/*.tsx` — 共通コンポーネント
- `scripts/build-android.sh` — ビルド自動化
- `.github/workflows/ci.yml` — CI ワークフロー
- `.github/workflows/release.yml` — CD ワークフロー

## Verification

- [ ] `cargo check -p takusu-local-lib --no-default-features` が通る (sqlx なし)
- [ ] `cargo check -p takusu-local` が通る (既存 sqlite 機能に影響なし)
- [ ] `cargo nextest run --workspace` が既存テスト全て通る
- [ ] `nix develop` で `cargo ndk -t aarch64-linux-android build -p takusu-android --release --no-default-features` が通る
- [ ] Expo アプリがエミュレータで起動し、localhost サーバーに fetch できる
- [ ] タスク依存グラフが dagre layout で表示される (推移的依存、完了ノード灰色)
- [ ] graph編集モードで edge切断・node間追加ができる
- [ ] タスクカードの slide-done / slide-delete / haptics が動く
- [ ] undo/redo (50step) が タスクCRUD + スケジュール操作 + habit CRUD で動く
- [ ] 同期ボタンが config に従って同時/2段階で動く
- [ ] UIが `design/mobile-ui.md` の記述に沿っている

## Risks/Considerations

- **Android での tokio runtime**: JNI スレッドで tokio runtime を起動する際のスレッド affinity に注意。UniFFI の async サポートを使う
- **サーバーライフサイクル**: Android のアプリライフサイクル (background/foreground) に合わせてサーバーを start/stop
- **ポート衝突**: 固定ポート (例: 3838) を使うか、動的ポートで JS 側に通知
- **バイナリサイズ**: .so は ~5-10MB 程度。APK サイズへの影響は許容範囲
- **Expo Module の複雑さ**: UniFFI 生成コードと Expo Modules API の統合には試行錯誤が必要な可能性
- **WebView + Cytoscape.js の編集モード**: ジェスチャー (edge切断/node間追加) の実装は Cytoscape.js の handler で実装可能だが、タッチ操作の精度に注意
- **undo/redo のスコープ**: 同期操作は含まないため、push 後の取り消しは不可。これは仕様
- **Nix での Android NDK**: `androidenv.composeAndroidPackages` で NDK を取得するが、バージョン固定と再現性に注意
