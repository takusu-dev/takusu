# Priority decoder production化ロードマップ

## Summary

Phase 2のpriority decoder prototypeは、taskの絶対時刻を直接操作する現行SAと異なり、task priorityを探索し、dependency上readyなtaskを順番に合法な位置へ配置する。10 seedの初期比較では、通常fixtureのqualityを維持または改善しながら7〜18倍高速化し、stress fixtureでも現行SAよりhard violationを大幅に削減した。

この結果はpriority decoder方式が有望であることを示す。一方、現在の実装は`quality-benchmark` feature限定のfull solve prototypeであり、実行不可能入力、partial / range solve、挿入位置の最適化、duration choice、ALNS、time budget、十分なcorrectness testには未対応である。本書は、prototypeをproduction solver候補へ育てるための不足事項、実装順序、採用条件を定義する。

関連する全体戦略は`design/planner-optimization-strategy.md`、実行不可能性の扱いは`design/core-problems.md`を参照する。

## Prototypeの現状

### 実装済み

- task ID列をpriority表現として保持
- fixed taskを先にtimelineへ配置
- dependencyがすべて配置済みのready taskをpriority順に選択
- `try_place`を利用したsleep、通常重複、parallel条件を考慮する最早配置
- priorityの2点swapによるseeded SA探索
- full `evaluate`による候補比較
- `TAKUSU_QUALITY_SOLVER=priority`によるquality benchmark切り替え
- dependency順序のunit test

### Prototypeに限定している理由

現在の実装は、探索表現を絶対時刻からpriorityへ変える価値があるかを低コストで検証するためのものに限定している。以下を意図的に未実装としているため、本番の`Planner::plan()`には接続していない。

- full solve以外の経路
- 挿入不能時の構造化された結果
- 複数挿入候補の目的関数比較
- duration choice
- destroy / repair operator
- adaptive operator selection
- time budgetとearly stopping
- warm start
- production向け設定とAPI
- 網羅的なcorrectness test

## 初期benchmark結果

すべてrelease build、single thread、10 seedsで測定した。SAの値は`baseline-sa-1core.tsv`、CP-SATの値は`baseline-cp/`を参照する。

### 通常fixture

| fixture | 現行SA score median | prototype score median | 現行SA latency p50 | prototype latency p50 | speedup |
|---|---:|---:|---:|---:|---:|
| small | 251.000 | 285.000 | 3.225 ms | 0.176 ms | 18.3x |
| 7d | -919.205 | -919.205 | 18.356 ms | 1.507 ms | 12.2x |
| 14d | -1844.372 | -1844.372 | 68.132 ms | 7.378 ms | 9.2x |
| 30d | -3873.816 | -3873.816 | 305.507 ms | 43.859 ms | 7.0x |

通常fixtureではmissing、duplicate、overlap、dependency、start、sleep、daily maximumのhard violationはすべて0だった。

### Stress fixture

| fixture | 現行SA score median | prototype score median | 現行SA latency p50 | prototype latency p50 |
|---|---:|---:|---:|---:|
| stress_30d | 108588.752 | 14058.684 | 19.54 s | 2.48 s |
| stress_30d_dense | 93796.125 | 24983.440 | 16.84 s | 1.27 s |
| stress_30d_mixed | 106218.515 | 27440.662 | 17.79 s | 1.62 s |

prototypeの総合scoreは低いが、現行SAが残すdependency、start、overlap違反は0になり、daily maximum超過も大幅に減った。例えば`stress_30d`では、現行SAのdependency違反7.3 slots、start違反1204.5 slots、daily maximum超過19544.7 slotsに対し、prototypeはそれぞれ0、0、253.8 slotsだった。

総合scoreだけではhard violation改善を正しく表せないため、stress fixtureの採用判断ではhard gateを先に評価し、soft componentを個別に比較する必要がある。

### CP-SATとの位置付け

| fixture | CP-SAT result | CP-SAT wall time | prototype latency p50 |
|---|---|---:|---:|
| small | OPTIMAL | 5.35 ms | 0.176 ms |
| stress_30d | FEASIBLE | 60.0 s | 2.48 s |
| stress_30d_dense | FEASIBLE | 60.0 s | 1.27 s |
| stress_30d_mixed | UNKNOWN、scheduleなし | 60.0 s | 1.62 s |

CP-SAT objectiveとtakusuの評価関数は同一ではないため、objective値は直接比較しない。小規模instanceのhard constraint oracle、モデル検証、best-known解生成に利用する。

## Production化に必要な設計

### Decoderの独立module化

prototypeは既存の`try_place`を再利用するため`anneal.rs`に置かれている。production化ではdecoderをSAの実装詳細から分離する。

```text
solver.rs
├── sa.rs
├── decoder.rs
├── alns.rs
└── evaluate.rs
```

decoderは探索方法を知らず、priority、duration choice、preference、固定配置を入力としてPlan候補と診断情報を返す。

```rust
struct DecodeInput<'a> {
    priority: &'a [TaskId],
    duration_choices: &'a [i64],
    pinned: &'a [Placement],
}

struct DecodeResult {
    plan: Plan,
    diagnostics: DecodeDiagnostics,
}
```

外部公開APIはproduction採用判断まで変更せず、まずcrate内部型として導入する。

### 実行不可能性の明示

現在は`try_place`失敗時に末尾へfallback配置する。このfallbackはsleep、parallel、deadline、daily maximumなどを破る可能性がある。production版では、黙って制約を破るのではなく、失敗理由と緩和内容を記録する。

```rust
enum PlacementFailure {
    DependencyCycle,
    NoLegalSlot,
    DailyCapacityExceeded,
    SleepConflict,
    ParallelConflict,
    DeadlineExceeded,
}
```

入力自体が実行不可能な場合に全task配置を維持するなら、緩和順序を明示する必要がある。暫定的には、fixed / pinned、task inclusion、dependency、parallel、startをhardとして先に守り、deadline、daily maximum、sleep、durationをどの順で緩和するかquality benchmarkで決める。最終方針は`design/core-problems.md`のinfeasibility reportingと整合させる。

### Fixed / pinned / partial / range

production版decoderはfull、partial、rangeを別実装にせず、固定配置集合の違いとして扱う。

1. fixed taskをtimelineへ投入
2. partialのpinned taskを投入
3. range外taskとextra pinnedを投入
4. dependency graph上のready taskを選択
5. 固定配置と競合しない合法候補へ挿入

保証するinvariantは次の通り。

- fixed / pinnedの位置を変更しない
- task IDを欠落・重複させない
- dependency graphがacyclicなら依存順を破らない
- range外taskを変更しない
- pinned taskに依存するtaskと、pinned taskから依存されるtaskの両方向を扱う
- previous scheduleをstability costへ反映する

### 合法挿入候補の列挙

現在は最初に見つかった合法位置を採用する。production版では候補を複数列挙し、目的関数deltaで選択する。

候補点は少なくとも次から生成する。

- dependencyの最遅終了直後
- taskのstart
- fixed / pinned /既配置taskの終了直後
- sleep windowの終了直後
- deadlineからdurationを引いた位置
- habit group anchor付近
- previous scheduleの開始時刻付近
- 日境界とcomfortable capacity内の位置

全slotを走査せず、制約境界から候補点を作ることで候補数を抑える。

### Repair operator

初期実装では以下を比較する。

- earliest feasible insertion
- lowest objective delta insertion
- deadline-priority insertion
- habit-anchor insertion
- regret-2 insertion

regret-2は、各未配置taskの2番目に良い挿入costと最良costの差を計算し、その差が大きいtaskから確定する。挿入場所の選択肢を失いやすいtaskを優先できる。

### Duration choice

prototypeは常に`cost_estimate.avg`を使う。production版ではtaskごとに少数のduration候補を持つ。

- expected: `avg`
- conservative: `avg + sigma`
- short: 実行不可能性を緩和する最小候補
- fixed duration: fixed / calendar由来task

候補数を小さく保ち、deadline、buffer、daily load、duration penaltyを含む挿入costで選択する。短縮を採用した場合はdiagnosticsへ記録する。

## ALNS

### Destroy operator

最初からadaptive weightingを導入せず、固定確率でoperatorごとの効果を測定する。

- random priority segment
- random time window
- deadline超過taskと周辺
- overloaded day
- dependency chain
- habit group
- previous scheduleから大きく移動したtask
- 高sigma taskと後続task

### Repair operatorとの組み合わせ

各destroy結果を複数repair方式で再構築し、次を記録する。

- 実行回数
- accepted回数
- best更新回数
- hard violationの改善量
- soft scoreの改善量
- wall time
- 1msあたりの改善量

十分なsampleを得てから、改善効率に基づくadaptive weightingを導入する。

### 探索状態

Phase 2はPhase 1の差分SA完了を必須としない。最初はPlan生成後のfull `evaluate`で比較する。ALNSの候補評価が支配的になった場合に、decoder向けの配置indexとscore component cacheを導入する。SA専用Move / Undoをそのまま移植するのではなく、複数taskのremove / insertに適した状態へ設計する。

## Time budgetとanytime性

固定iteration数`task_count * 100`はprototype用である。production版ではiteration budgetとtime budgetを分離する。

1. decoder-onlyで初期Planを即時生成
2. 指定budgetまでALNSで改善
3. cancellation時は現在のbest feasible Planを返す
4. 改善が一定期間なければearly stop

評価budget候補は20ms、50ms、100msとし、stress fixtureではより長いbackground budgetも比較する。モバイルではwall timeだけでなくCPU time、memory、発熱を確認する。

## Production化ステージ

### Stage 1: Correct decoder

- `decoder.rs`へ独立
- fixed / pinned timelineを統一
- DAG ready queueを明示化
- dependency cycleを検出
- 合法挿入候補を列挙
- fallbackを`DecodeDiagnostics`へ置換
- full / partial / rangeを統一実装
- correctness testを追加

この段階ではproductionの`plan()`を置き換えない。

### Stage 2: Quality improvement

- score component別benchmark
- lowest-delta insertion
- regret-2 insertion
- deadline / habit / stability repair
- duration choice
- stress fixtureのdaily maximum超過削減

hard gateを維持しながらsoft scoreを改善する。

### Stage 3: ALNS

- destroy operator追加
- repair operatorの組み合わせ比較
- operator統計
- 固定weight tuning
- adaptive weighting

### Stage 4: Production integration

- time budgetとearly stopping
- warm start
- deterministic seed policy
- solver選択設定
- `Planner::plan()`、partial、rangeへの接続
- mobile実機測定
- SA fallbackとrollback手段

solverの切り替え方式は現時点では確定しない。最初は設定で切り替え可能にする方針を候補とするが、decoder内部のcorrectnessとqualityを優先し、公開設定は後で設計する。

## Test plan

### Unit test

- fixed task不変
- pinned task不変
- dependency chain
- dependency cycle
- task start境界
- sleep回避
- parallel許可と禁止
- 合法slotなし
- deadline超過
- daily maximum超過
- duration choice
- zero average duration
- fixed同士の競合
- habit anchor
- previous schedule stability
- 同一seedの決定性

### Property / randomized test

- task IDの欠落・重複なし
- acyclicかつfeasibleな入力でdependency違反なし
- fixed / pinned移動なし
- decoder結果のdiagnosticsとhard metricsが一致
- 同じ入力・priority・seedで同じ結果
- full / partial / rangeでtask IDが保持される

### Benchmark

- 32〜100 seedsのpaired comparison
- small、7d、14d、30d
- stress、dense dependency、mixed fixed / parallel
- full、partial、range
- CP-SAT oracleとの小規模比較
- p50 / p95 latency
- hard metric全項目
- score component別median / p10
- task数とhorizonに対するscaling

## Production採用条件

priority decoder / ALNSをproduction solver候補として`plan()`へ接続するには、少なくとも次を満たす。

- 通常fixtureのhard violationが全seedで0
- stress fixtureの各hard metricが現行SA以下
- fixed / pinned移動が全testで0
- dependency cycleや挿入不能入力でpanicしない
- diagnosticsが実際の緩和・違反と一致
- 通常fixtureのmedian soft scoreが現行SA以上
- p10 soft score低下が現行SA比1%以内
- 30日fullのp95が100ms以下
- partial / rangeが現行SAより高速
- 100 seed paired benchmarkを通過
- small instanceでCP-SAT oracleとhard constraint結果が一致
- mobile実機で許容可能なCPU、memory、発熱
- SAへ戻せる設定またはrollback経路がある

stress fixtureでは総合scoreだけをgateにしない。hard gateを先に通した上で、deadline、duration、buffer、habit、stability、daily loadをcomponent別に評価する。

## 次の実装単位

次の変更ではStage 1に着手する。

1. prototypeのdecoderを`decoder.rs`へ移す
2. `DecodeInput`、`DecodeResult`、`DecodeDiagnostics`を内部型として追加
3. fixed / pinnedを共通timelineへ投入
4. ready queueとcycle検出を実装
5. fallbackを診断付きの明示的緩和へ置換
6. full / partial / rangeのcorrectness testを追加

この単位ではALNS operatorや公開solver設定を追加しない。まずdecoderが全solve modeで制約と失敗理由を正しく扱えることを確立する。
