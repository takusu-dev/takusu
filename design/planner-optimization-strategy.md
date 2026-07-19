# takusu-core / takusu-habit 最適化戦略

## Summary

`takusu-core` の計画生成は、30日分の実世界habit fixtureで数百msを要しており、同期的なUXではボトルネックになり得る。本書では、スケジュール品質を維持しながら、必要であれば探索表現やアルゴリズムにも破壊的変更を加える前提で、検討すべき最適化方針を整理する。

推奨する順序は次の通り。

1. 品質を総合score以外も含めて測るquality benchmarkを先に整備する
2. `Plan` の全体cloneと全体再評価を、mutableな探索状態と差分評価へ置き換える
3. 任意配置をペナルティで修復する探索から、実行可能解を生成するdecoder + ALNSへ移行する
4. habit occurrenceをすべて独立taskとして扱わず、habit group単位の階層探索を導入する
5. time budget、warm start、局所再計画によりanytimeなUXを構築する

最初の実装対象としては、品質benchmarkを整備した上で、`shift` と `duration` moveだけをin-place更新 + 差分評価にするのが最も低リスクである。その結果を基準に、priority decoder + ALNSとhabit階層化を段階的に試す。

## 現状分析

### 探索量

現在のfull solveは、各SA chainでおおむね次の探索を行う。

- 初期温度 `T0` から `T0 * 1e-4` まで、`alpha = 0.93` で冷却
- 各温度で `task_count * 30` iteration
- 最大4本の独立chainを並列実行

温度段階は約127なので、4 chain合計では概算 `15,000 * task_count` 回の近傍評価になる。

### 近傍評価のコスト

通常の `shift`、`swap`、`duration` moveでも、現在は `schedules.to_vec()` でPlan全体を複製する。その後の評価では毎回次を行う。

- task IDから配置へのindexを再構築
- schedulesを開始時刻で再ソート
- task、依存、buffer、sleep、日次負荷、並列違反、stability、habit consistencyを全再計算

特に `buffer_score` は各taskについて他の全taskを走査する。並列違反も最悪時にはtask pairを走査する。このため、評価1回はO(n²)、iteration数もtask数に比例するので、solve全体は実質的にO(n³)に近いスケーリングを持つ。

既存baselineでは、7日fixtureが17 tasksで約22ms、30日fixtureが72 tasksで約438msである。単純な線形増加より大幅に悪化しており、長いplanning horizonでUX問題になる可能性が高い。

profileでも、Plan複製に由来するcopy、日次・睡眠windowのunion計算、habit consistency、並列違反、index構築が主要なself timeを占めている。

### takusu-habitの位置付け

`takusu-habit` のrecurrence生成は日付を順に走査するが、7〜30日程度ではsolverより小さいコストと考えられる。短期的にはgeneratorの定数倍最適化よりも、生成された全occurrenceを独立した探索変数にすることで `takusu-core` の問題サイズが増える点を優先して扱う。

## 品質維持の定義

アルゴリズム変更前後でT=0の総合scoreだけを比較するのは不十分である。重み付き和では、重大な制約違反の悪化が別のsoft score改善で相殺され得る。また評価関数自体を変更する実験では、旧scoreと新scoreを直接比較できない。

品質検証をhard gateとsoft metricsに分ける。

### Hard gate

以下は個別に測定し、baselineより悪化した候補を原則不採用とする。

- task IDの欠落と重複
- fixed / pinned taskの移動
- 不正な並列重複スロット数
- 依存違反スロット数
- taskの開始可能時刻違反
- 最低睡眠時間を下回る日数と不足スロット数
- 日次maximum超過スロット数

実行不可能な入力では違反ゼロを要求できないため、baselineと同じ入力に対する違反量のpaired comparisonを行う。将来、実行不可能理由をPlanに表現できるようになった場合は、明示されたabandonmentやwarningも別指標として扱う。

### Soft metrics

- abandonability帯別のdeadline超過スロット数
- duration不足・超過スロット数
- uncertainty buffer
- previous scheduleからの移動量
- habit groupごとの時刻分散
- 日次負荷の分散とcomfortable超過量
- 現行評価関数によるT=0 score

### 測定方法

性能benchmarkとは別にquality benchmarkを用意する。

- 代表fixtureごとに32〜100 seedを実行する
- 同じseedをbaselineと候補で比較する
- medianだけでなくp10またはworst decileも確認する
- full、partial、range solveをすべて対象にする
- task数、依存密度、fixed比率、並列task比率、habit horizonを変えたfixtureを持つ
- 小規模問題では長時間探索またはexact solverによるbest-known解とのgapを測る

初期の採用条件例は次の通り。

- hard violation metricsがbaseline以下
- median soft scoreがbaseline以上
- p10 soft scoreがbaseline比1%以内の低下
- p95 latencyが定めたbudget以下

1%は暫定値であり、各score componentのスケールを確認した後に固定する。単一の総合scoreだけで合否を決めない。

## 方針1: mutable SearchStateと差分評価

### 目的

評価関数の意味を維持したまま、通常moveごとのPlan clone、sort、index再構築、全score再計算を除去する。

### 状態表現

概念上、次のような内部状態を持つ。

```rust
struct SearchState {
    placement_by_task: Vec<Placement>,
    order_by_start: Vec<TaskId>,
    position_by_task: Vec<usize>,

    task_score: Vec<f64>,
    dependency_score: Vec<f64>,
    day_load: Vec<i64>,
    sleep_occupied: Vec<i64>,
    habit_stats: Vec<HabitStats>,

    total_score: f64,
}
```

探索operatorは新しいPlanを返すのではなく、moveを生成する。

```rust
enum Move {
    Shift { task: TaskId, new_start: Point },
    Swap { a: TaskId, b: TaskId },
    Resize { task: TaskId, new_duration: i64 },
    Rebuild { removed: Vec<TaskId>, inserted: Vec<Placement> },
}
```

`apply(move)` は差分とundo情報を返し、不採用moveでは `undo` する。best更新時だけ外部公開用のPlanへmaterializeする。

### 影響範囲

1 taskのshiftで再計算する候補は次の通り。

- 当該taskのdeadline、start、duration、stability
- 当該taskの依存元と依存先
- old / new区間と交差するtaskの並列違反
- old / new区間が属する日の日次負荷
- old / new区間と交差する睡眠window
- 同じhabit groupのstatistics
- buffer終端が変化し得るtask

bufferは影響関係が比較的広い。最初から複雑なinterval treeを導入せず、当該taskと影響候補だけをO(n)で再走査する実装から始める。それでも毎moveの全task × 全task評価より大幅に小さくできる。

### 正しさの検証

差分評価は、全体評価との二重実装になる。score driftを検出するため、次を必須とする。

- unit testでは各move後にfull `evaluate`と一致確認
- property testまたは乱数seed固定testで長いapply / undo列を検証
- debug buildでは設定可能な間隔でfull評価
- release buildでも一定iterationごとにfull評価する検証モードを用意
- score component単位で差分を比較できる構造にする

### 期待する効果

理論上は、通常moveの評価をO(n²)からO(n)または影響範囲比例へ近づけ、solve全体をO(n³)からO(n²)へ近づけられる。実速度はoperator構成とbuffer更新に依存するため、具体的な倍率はbenchmarkで判断する。30日fixtureのp95をまず100ms前後まで下げられるかを実験目標とし、保証値とはしない。

## 方針2: 実行可能解を生成するpriority decoder

### 現状の問題

現在のSAはtask startとdurationを直接変更し、一時的な依存違反や並列違反をpenaltyで許容する。探索空間の大きな部分が最終的に利用できない配置であり、hard constraintの修復にも多くのiterationを使う。

### 提案

探索対象を絶対時刻の集合ではなく、task priority、duration choice、日・時刻のpreferenceとして表現する。それをdeterministicまたは少数候補付きdecoderでPlanに変換する。

```text
priority order
+ duration choices
+ day/time preferences
        ↓
schedule decoder
        ↓
fixed / pinned / dependency / parallel条件を満たすPlan
```

decoderはResource-Constrained Project Scheduling Problemで使われるSerial Schedule Generation Schemeに近いものとする。

1. fixed / pinned scheduleをtimelineへ投入する
2. 依存関係上readyなtaskからpriority順に選ぶ
3. dependency終了時刻とtask start以後の候補を得る
4. sleep、並列条件、既配置taskを考慮して合法な挿入位置を列挙する
5. deadline、日次負荷、bufferを含む挿入costが最小の位置へ配置する

現在の `try_place` と `greedy_rebuild` は、このdecoderの原型として再利用できる。

### 注意点

feasible-only探索では、現行のconstraint annealingが可能にしている「一時的に依存違反領域を通って別の良解へ移る」経路がなくなる。1 taskずつpriorityを変更するだけでは局所解に閉じやすいため、複数taskをまとめて除去・再挿入するLNS / ALNSと組み合わせる。

入力自体が実行不可能な場合もある。その場合はdecoderが黙って制約を破るのではなく、未配置、短縮、deadline超過、生活制約違反などを構造化して返す必要がある。この部分は `design/core-problems.md` のinfeasibility reporting方針と整合させる。

## 方針3: ALNSとregret insertion

### Destroy operators

問題の構造に応じて、複数のdestroy operatorを用意する。

- random time window
- deadline超過taskとその周辺
- overloaded day
- dependency chainまたは依存違反周辺
- 同じhabit group
- 高sigma taskと後続task
- previous scheduleから大きく移動したtask
- 競合の多い区間

### Repair operators

- earliest feasible insertion
- lowest objective delta insertion
- deadline優先 insertion
- habit anchor優先 insertion
- regret-k insertion

regret-2では、未配置taskごとに「2番目に良い挿入cost - 最良挿入cost」を計算し、その差が大きいtaskから配置する。後回しにすると良い挿入場所を失いやすいtaskを先に確定できる。

### Adaptive selection

各destroy / repair operatorについて、直近の採用率、best改善回数、計算時間を記録する。改善効率の高いoperatorの選択確率を上げる。単に改善回数だけを見ると高コストoperatorを過大評価するため、wall timeまたは評価回数で正規化した指標も比較する。

当初は固定重みでoperatorごとの品質と速度を測り、十分なサンプルが得られてからadaptive weightingを導入する。

## 方針4: habitの階層探索

### 問題

30日のdaily habitは、現在は30個の独立taskへ展開される。solverから見ると各occurrenceが独立変数なので、horizonに比例して探索状態と評価コストが増える。一方、目的関数は同じhabit groupを近い時刻へ配置することを評価しており、独立moveは目的構造と合っていない。

### 提案

habit groupごとに次を上位変数として探索する。

- preferred time-of-day
- 許容time window
- weekdayごとのoffset
- 基本duration
- exception occurrence

```text
habit template群のanchorを決定
        ↓
各occurrenceをmaterialize
        ↓
通常task / fixed予定と競合するoccurrenceだけ局所修正
```

毎日のhabitをoccurrence数個分の独立startとして扱う代わりに、1つのanchorと少数のexceptionで表現する。habit consistencyを保ちやすく、長いhorizonでも探索次元の増加を抑えられる。

### API候補

長期的には `takusu-habit` が即座に `Vec<Task>` だけを返すのではなく、recurrence構造を保持した入力をcoreへ渡す。

```rust
enum PlanningItem {
    Task(Task),
    HabitSeries(HabitSeries),
}
```

solver内部では必要なhorizonまたはrolling windowだけをmaterializeする。ただしこれはcrate境界とpublic APIに影響するため、差分評価とdecoderの効果を確認した後の段階とする。

## 方針5: anytime、warm start、rolling horizon

### Anytime solve

固定iteration数ではなくtime budgetを受け取り、いつ停止しても現在のbest feasible planを返せるようにする。

- greedy decoderで初期解を早期に生成
- 20ms、50ms、100msなどのbudgetまで改善
- 改善停止chainを早期終了
- task数とbudgetに応じてchain数を調整
- best更新時に中間結果を通知できる内部APIを検討

同期APIではbudget到達時にbestを返す。将来のresident plannerでは、早いplanを提示した後にバックグラウンドで改善を継続する構成も可能になる。

### Warm start

task追加・編集・完了による再計画では、前回Planを初期解にする。変更taskから次のclosureだけをdestroyする。

- dependency ancestors / descendants
- 時間的に競合するtask
- 同じ日の日次負荷に影響するtask
- 同じhabit group

それ以外はpinし、局所的にrepairする。full solveよりも日常的な再計画UXに直接効く可能性が高い。

### Rolling horizon

長期horizonでは、近い予定ほど詳細に最適化し、遠い予定は粗い表現またはhabit anchorだけで保持する。

- near term: 5分slotで確定的に探索
- middle term: 日または広いtime window単位
- far term: habit templateとcapacity reservation

horizon境界付近の不自然な配置を避けるため、windowを重ねて順次確定する。近未来をpinしすぎると後続problemが実行不可能になるため、境界にbufferを持たせ、定期的にwindow全体を再評価する。

## 優先しない方針

### SA parameter tuningだけを行う

`alpha`、iteration数、operator確率の調整は有効だが、Plan cloneと全評価による超線形構造を残す。quality benchmark整備後の補助的調整とし、主戦略にはしない。

### chain数を増やす

CPU使用量と発熱が増え、mobileでは不利である。各chain内の無駄を減らす前に並列度を上げない。

### recurrence generatorの定数倍最適化

年単位の大量生成では価値があるが、現在の7〜30日UXではsolver側の問題サイズ増加を先に解決する。

### DAG連結成分をそのまま独立solveする

依存関係がなくても、taskは時間資源、睡眠、日次負荷を共有するので真に独立ではない。成分ごとの解を後からmergeすると品質や実行可能性が悪化し得る。rolling horizonやALNSのdestroy単位として利用する方が安全である。

### CP-SATへ直ちに全面移行する

CP-SATは小規模instanceのoracle、hard constraint modelの検証、best-known解生成には有用である。一方、takusu固有のbuffer、habit consistency、stability、非線形日次負荷、Rust / Android配布条件を含めた本番全面移行は大きな設計変更になる。まず比較・検証用途で評価し、採用判断は別途行う。

## 実験ロードマップ

### Phase 0: quality benchmark

1. seed指定可能なsingle-chain solveを用意する
2. score componentとhard violationを個別に集計する
3. full / partial / rangeの代表fixtureを追加する
4. 32〜100 seedのpaired comparisonを自動化する
5. 小規模instanceにbest-knownまたはexact oracleを用意する
6. latencyとqualityのPareto frontを記録する

### Phase 1: セマンティクス不変の差分探索

1. `SearchState` とMove / Undoを導入する
2. `shift` と `duration` をin-place化する
3. task、依存、stability scoreを差分化する
4. day、sleep、habit scoreを差分化する
5. overlapとbufferの局所更新を追加する
6. `swap`、LNS、partial solveへ展開する

各段階でfull評価との一致をtestし、quality gateとrealworld benchmarkを通す。

### Phase 2: priority decoder + ALNS

1. 現在の `try_place` / `greedy_rebuild` を独立decoderへ整理する
2. fixed、pinned、dependency、parallel条件を構築時に保証する
3. 複数のdestroy operatorを追加する
4. lowest-deltaとregret insertionを比較する
5. operatorごとの時間・採用・改善統計を取る
6. 現行SA、差分SA、ALNSを同じquality benchmarkで比較する

### Phase 3: habit階層化

1. habit group anchorを内部探索変数として試作する
2. occurrence展開後の局所repairを実装する
3. exception occurrenceを表現する
4. 30日・90日fixtureで品質とスケーリングを測る
5. 効果が確認できた場合だけcrate間APIを変更する

### Phase 4: UX統合

1. solve time budgetを導入する
2. previous scheduleからのwarm startを導入する
3. affected neighborhoodだけを再計画する
4. rolling horizonを長期planへ適用する
5. p50 / p95 latencyとスケジュール変更量を実利用に近い操作列で測る

## 意思決定基準

最終的には、単一benchmarkの最速実装ではなく、次のPareto frontで方式を選ぶ。

- hard constraint品質
- soft score分布
- p50 / p95 latency
- task数・horizonに対するスケーリング
- memory使用量
- mobileでのCPU時間と発熱
- partial reschedule時のschedule stability
- 実装複雑度と検証可能性

差分SAが目標latencyを満たすなら、decoder全面移行を急ぐ必要はない。逆にhabit horizon増加時に差分SAでも超線形悪化が残るなら、habit階層化とpriority decoderを優先する。各phaseを独立した実験として扱い、品質gateを満たさない変更は採用しない。

## 参考資料

### LNS / ALNS

- Paul Shaw, [Using Constraint Programming and Local Search Methods to Solve Vehicle Routing Problems](https://doi.org/10.1007/3-540-49481-2_30), CP 1998, LNCS 1520, pp. 417–431. Related itemsを除去して再挿入するLarge Neighborhood Searchの初期文献。
- Stefan Ropke and David Pisinger, [An Adaptive Large Neighborhood Search Heuristic for the Pickup and Delivery Problem with Time Windows](https://doi.org/10.1287/trsc.1050.0135), Transportation Science 40(4), 2006, pp. 455–472. 複数のdestroy / repair heuristicを過去の性能に応じて選択するALNSを扱う。
- David Pisinger and Stefan Ropke, [Large Neighborhood Search](https://doi.org/10.1007/978-3-319-91086-4_4), Handbook of Metaheuristics, 2018, pp. 99–127. LNSの後年の概説。

### Schedule generationと挿入heuristic

- Rainer Kolisch, [Serial and Parallel Resource-Constrained Project Scheduling Methods Revisited: Theory and Computation](https://doi.org/10.1016/0377-2217(95)00357-6), European Journal of Operational Research 90(2), 1996, pp. 320–333. RCPSPのserial / parallel schedule generation schemeを扱う。
- Rainer Kolisch and Sönke Hartmann, [Heuristic Algorithms for the Resource-Constrained Project Scheduling Problem: Classification and Computational Analysis](https://doi.org/10.1007/978-1-4615-5533-9_7), Project Scheduling: Recent Models, Algorithms and Applications, 1999, pp. 147–178. Schedule generationとmetaheuristicの分類・比較。
- Jean-Yves Potvin and Jean-Marc Rousseau, [A Parallel Route Building Algorithm for the Vehicle Routing and Scheduling Problem with Time Windows](https://doi.org/10.1016/0377-2217(93)90221-8), European Journal of Operational Research 66(3), 1993, pp. 331–340. Generalized regret measureを用いた挿入順序を扱う。

### Anytimeと再計画

- Shlomo Zilberstein, [Using Anytime Algorithms in Intelligent Systems](https://hdl.handle.net/20.500.14394/9746), AI Magazine 17(3), 1996, pp. 73–83. 計算時間と解品質を交換可能にするanytime algorithmの概説。
- Guilherme E. Vieira, Jeffrey W. Herrmann, and Edward Lin, [Rescheduling Manufacturing Systems: A Framework of Strategies, Policies, and Methods](https://doi.org/10.1023/A:1022235519958), Journal of Scheduling 6(1), 2003, pp. 39–62. Event-driven、periodic、rolling horizonを含む再計画戦略の整理。

### Exact solverを比較用oracleとして使う場合

- Google OR-Tools, [Scheduling Overview](https://developers.google.com/optimization/scheduling). CP-SATを使ったemployee schedulingとjob-shop schedulingへの公式入口。
- Google OR-Tools, [Constraint Optimization with CP-SAT](https://developers.google.com/optimization/cp). Integer constraint modelとsolver利用法の公式資料。
