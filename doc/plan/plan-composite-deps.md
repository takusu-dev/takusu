# 合成射(冗長依存辺)の検出・確認・削除 計画 (#355)

## Summary

- **#355**: タスクの依存に 1→2, 2→3, 1→3 がある場合、1→3 は合成射(推移的に冗長な辺)。
  これを検出してユーザーに提示し、**どの辺を消すか選ばせて**削除できるようにする
  (冗長辺 1→3 を消すのが基本だが、issue の要望どおり経路上の辺 2→3 を消す選択も可能にする)。
- 対象は **task の `depends`** と **habit step の `depends_on`** の両方。
- 検出ロジックは **takusu-local-lib に petgraph で実装**する。
  **takusu-worker には入れない**(worker はストレージなので分析ロジックを持たない)。
- 削除は既存の mutation(`PATCH /api/tasks/{id}`, `PUT /api/habits/{id}/steps`)を使う。
  新規エンドポイントは **read-only の分析 2 本のみ**。
- PR は 2 本に分ける: PR 1 = server + CLI、PR 2 = mobile UI(こちらに `Closes #355`)。

## 前提(現状の仕組み)

- task の依存は `tasks.depends`(JSON 配列のタスク ID、`TaskRow.depends: String`)。
  habit step の依存は `habit_steps.depends_on`(JSON 配列の step ID、habit 内に閉じる)。
- 循環検出は現在 2 箇所に重複実装されている:
  - `crates/takusu-local-lib/src/app.rs` の `detect_cycle`(L386 付近)と
    `topo_sort_steps`(L123 付近、Kahn 法)
  - `crates/takusu-worker/src/validate.rs` の `detect_cycle`(L221 付近)
  - **worker 側は触らない**。local-lib 側のみ petgraph ベースに統合する。
- mobile アプリは takusu-android 経由で takusu-local の axum サーバーを
  インプロセスで起動し、HTTP(`mobile/src/api/client.ts` の `TakusuClient`)で叩く。
  → local-lib/local にエンドポイントを足せば **UniFFI/Kotlin の変更なしで mobile から使える**。
- 依存の削除手段は既存 API で足りる:
  - task: `PATCH /api/tasks/{id}` で `depends` を差し替え
  - step: `PUT /api/habits/{id}/steps` で `depends_on` を差し替え

## Part 1: takusu-local-lib にグラフモジュール(petgraph 導入 + 統合)

### 依存追加

- workspace の `Cargo.toml` に `petgraph = { version = "0.8", default-features = false }`
  を追加し(バージョンは最新安定を確認、公開から 7 日未満の版は避ける)、
  `crates/takusu-local-lib/Cargo.toml` から `petgraph.workspace = true` で利用。
  必要な feature は `algo` 系のみ(`stable_graph` / `graphmap` / `matrix_graph` は不要)。

### 新規モジュール `crates/takusu-local-lib/src/graph.rs`

公開する関数は 3 つ(いずれも `adj: &[Vec<usize>]`(隣接リスト、`adj[u]` = u が依存する先
もしくは u → v の辺)を入力とする。既存コードの辺の向きに合わせること):

```rust
pub(crate) struct RedundantEdge {
    pub from: usize,
    pub to: usize,
    /// from → … → to の witness 経路(from, to を含む、長さ >= 3)
    pub via: Vec<usize>,
}

/// 循環があれば Err(サイクルに含まれるノードの一つ)を返す。
pub(crate) fn detect_cycle(adj: &[Vec<usize>]) -> Result<(), usize>;

/// Kahn 法相当のトポロジカル順序を返す。循環があれば Err。
pub(crate) fn topo_sort(adj: &[Vec<usize>]) -> Result<Vec<usize>, usize>;

/// DAG の推移簡約を計算し、簡約に残らない辺 = 冗長辺を返す。
/// 各冗長辺について直接辺を除いた BFS で witness 経路を 1 本添える。
/// 入力に循環がある場合は Err。
pub(crate) fn find_redundant_edges(adj: &[Vec<usize>]) -> Result<Vec<RedundantEdge>, usize>;
```

実装は petgraph を使う:

- `detect_cycle` / `topo_sort`: `petgraph::algo::toposort`(`Err(Cycle)` からノードを取る)
- `find_redundant_edges`:
  1. `toposort` でトポ順を得る
  2. `petgraph::algo::tred::dag_to_toposorted_adjacency_list` で変換
  3. `dag_transitive_reduction_closure` で推移簡約を得る
  4. 元グラフの辺のうち簡約に含まれない辺が冗長辺
  5. 冗長辺 (u, v) ごとに、辺 (u, v) を除いたグラフで u→v の BFS を行い
     witness 経路 `via` を復元する(冗長辺は少数なので計算量は問題ない)

### 既存コードの置き換え(挙動・エラーメッセージは維持)

- `app.rs` の `detect_cycle`(L386)を削除し、呼び出し側で `graph::detect_cycle` を使い、
  Err 時は従来と同じ `AppError::BadRequest` メッセージに変換する。
- `topo_sort_steps`(L123)は id→index 変換と JSON パースを残しつつ、
  中身のソートを `graph::topo_sort` に委譲する。
- `crates/takusu-worker/src/validate.rs` は**変更しない**。
- 既存テスト(循環検出まわり)が全部通ることを確認する。

## Part 2: TakusuApp + HTTP API

### TakusuApp メソッド(`crates/takusu-local-lib/src/app.rs`)

```rust
pub struct RedundantDependency {
    pub from: String,        // 依存する側のタスク/step ID
    pub from_title: String,  // 表示用(step の場合は step title)
    pub to: String,          // 依存される側
    pub to_title: String,
    pub via: Vec<DependencyNode>, // witness 経路(from, to を含む)、id + title
}

pub async fn analyze_task_dependencies(&self) -> Result<Vec<RedundantDependency>, AppError>;
pub async fn analyze_habit_step_dependencies(&self, habit_id: &str)
    -> Result<Vec<RedundantDependency>, AppError>;
```

- task 版: **status が done でないタスク**のみを対象にする
  (完了済みタスクへの依存整理は意味がないため)。`depends` が存在しない ID を
  指していても分析はエラーにせず、その辺を無視する(既存データの整合性に寛容に)。
- step 版: 対象 habit の全 step の `depends_on` を対象にする。
- どちらも既存の `build_dep_graph` 相当の id↔index 変換を再利用/踏襲する。
- グラフに循環がある場合(通常は作成時に弾かれているが防御的に)は
  `AppError::BadRequest` を返す。

### ルート(`crates/takusu-local/src/router.rs` + handlers)

- `GET /api/tasks/dependency-analysis` → `analyze_task_dependencies`
- `GET /api/habits/{id}/steps/dependency-analysis` → `analyze_habit_step_dependencies`

レスポンス例:

```json
{
  "redundant": [
    {
      "from": "task-1", "from_title": "レポート提出",
      "to": "task-3", "to_title": "資料集め",
      "via": [
        {"id": "task-1", "title": "レポート提出"},
        {"id": "task-2", "title": "下書き"},
        {"id": "task-3", "title": "資料集め"}
      ]
    }
  ]
}
```

既存ルートの登録方法・認証ミドルウェア・handler の書き方は近隣の
task/habit ルートに合わせる。**takusu-worker にはルートを追加しない。**

## Part 3: takusu-client + CLI

### takusu-client(`crates/takusu-client/src/lib.rs`)

- レスポンス型 `RedundantDependency` / `DependencyNode` / `DependencyAnalysisResponse` を追加
- `pub async fn analyze_task_dependencies(&self) -> Result<DependencyAnalysisResponse, ...>`
- `pub async fn analyze_habit_step_dependencies(&self, habit_id: &str) -> ...`
- 既存メソッドの命名・エラー処理スタイルに合わせる。roundtrip テスト
  (`crates/takusu-client/tests/roundtrip.rs`)にも型を足す。

### CLI(`crates/takusu-cli/src/main.rs`)

新サブコマンド:

- `takusu task deps-check`
- `takusu habit steps-check <habit>`

対話フロー(既存の `prompt()` / `is_interactive()` を踏襲):

```
冗長な依存が見つかりました (1/2):
  「レポート提出」→「下書き」→「資料集め」 の経路があるため
  「レポート提出」→「資料集め」 は冗長です。
  [1] 冗長な辺 レポート提出→資料集め を削除
  [2] 経路上の辺を削除 (2a: レポート提出→下書き, 2b: 下書き→資料集め)
  [s] スキップ  [q] 終了
>
```

- `[1]`: from タスクの `depends` から to を除いて `update_task`(step なら
  `replace_habit_steps` で該当 step の `depends_on` を差し替え)
- `[2x]`: 経路上の選んだ辺を同様に削除
- **1 辺削除するごとに分析 API を叩き直す**(経路上の辺を消すと他の冗長判定が
  変わるため)。残りの冗長辺で続行。
- 非対話環境(`!is_interactive()`)では検出結果を表示するのみで変更しない。
  表示は `display_rich` / `display_simple` の既存パターンに合わせる。
- 冗長辺が無ければ「冗長な依存はありません」を出して終了コード 0。

## Part 4: mobile UI

- `mobile/src/api/client.ts` に `analyzeTaskDependencies()` /
  `analyzeHabitStepDependencies(habitId)` と型を追加。
- **task**: `mobile/src/views/TaskDetailView.tsx` の依存セクション
  (依存リスト+ミニグラフがある付近、L1027-1090 あたり)に、開いているタスクが
  関与する冗長辺があれば警告表示を出す。タップで既存パターンの `Alert.alert`
  (`mobile/src/views/HomeView.tsx` L1054-1070 の Promise ラップ形式を踏襲)で
  「冗長な辺を削除 / 経路上の辺を削除 / キャンセル」を選択 → `updateTask` →
  再フェッチ+再分析。
- **habit step**: `mobile/src/components/HabitStepEditor.tsx` に冗長辺の警告バッジを
  表示し、タップで同様の選択ダイアログ → `replaceHabitSteps` → 再分析。
  既存の `hasCycle`(`mobile/src/utils/habitSteps.ts`)による循環チェック UI と
  並ぶ位置に置く。
- 文言は既存 UI に合わせて日本語。

## Part 5: テスト・検証

### 単体テスト(`graph.rs` 内 `#[cfg(test)]`)

- ダイヤモンド型(1→2, 1→3, 2→4, 3→4 — 冗長なし)
- 単純な合成射(1→2, 2→3, 1→3 — 1→3 が冗長、via = [1,2,3])
- 長い経路経由(1→2→3→4 と 1→4)
- 複数の冗長辺
- 循環入力で Err
- witness 経路が実際に辺として存在すること

### 統合テスト(`crates/takusu-local/tests/integration.rs`)

- 既存の `setup()` / `auth_req()` パターンを踏襲
- task: 3 タスク(1→2, 2→3, 1→3)を作成 → `GET /api/tasks/dependency-analysis` が
  1→3 を冗長として返す。冗長辺を PATCH で消した後は空になる
- done タスクが分析対象から除外されること
- step: 3 step の habit で同様に `GET /api/habits/{id}/steps/dependency-analysis`
- 冗長がない場合に空配列が返ること

### 検証コマンド

```sh
cargo nextest run --workspace   # 既存 ~171 テスト + 新規が全部通ること
cargo clippy
treefmt                          # または cargo fmt
# mobile (PR 2):
cd mobile && npx tsc --noEmit && npm run lint && npm run fmt:check
```

## PR 分割・進め方

1. **PR 1**(server + CLI): Part 1〜3 + Part 5 の Rust 側テスト。
   本文で `#355` を参照するが `Closes` は付けない(mobile が残るため)。
2. **PR 2**(mobile): Part 4 + typecheck/lint。本文に `Closes #355`。

各 PR は `.devin/rules/pr-workflow.md` の Jujutsu ワークフローに従う:
`jj describe` → `jj git push --change` → `gh pr create`。
コミットメッセージは現在形・小文字始まり・末尾ピリオドなし。
