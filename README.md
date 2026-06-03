# takusu

ユーザーのスケジュールを自動構築するプランナーと、LLM 音声アシスタント。

## 特徴

- 締め切り・見積り・依存関係・並列性・諦めやすさを考慮した自動スケジューリング
- 焼きなまし法 (SA) + LNS + Tabu Search で最適化
- REST API サーバー (axum + SQLite)
- 音声アシスタント (Whisper + LLM、開発中)

## セットアップ

```sh
nix develop  # または direnv allow
```

## コマンド

```sh
cargo check                        # 型チェック
cargo nextest run --workspace      # テスト
cargo bench -p takusu-core        # ベンチマーク
cargo run --example daily          # サンプル実行
```

## 使い方

```rust
use takusu_core::{Planner, Task, NormalDist, Point, SleepConfig};

let mut planner = Planner::new(Point::now(5), SleepConfig::recommended());

planner.add(Task {
    id: 0,
    start: Some(Point::from_raw(0)),
    end: Point::from_raw(100),
    cost_estimate: NormalDist::new(10, 2),
    depends: vec![],
    parallelizable: false,
    allows_parallel: false,
    abandonability: 0.5,
}).unwrap();

let plan = planner.plan();
```

### API サーバー

```sh
TAKUSU_ROOT_TOKEN=tsk_... cargo run -p takusu-serve
```

API 仕様: [`crates/takusu-serve/SPEC.md`](crates/takusu-serve/SPEC.md)

## 設計ドキュメント

[`main.typ`](main.typ) — プロジェクト全体の設計思想 (Typst・日本語)

## License

MIT