# habit 機能改善計画 (#95 / #303 / window_mode)

## Summary

- **#303**: 事前にスケジュールされた habit の期間指定無効化(休暇など)。`habit_pauses` テーブルで複数の休止期間を管理する。
- **#95**: 一つの habit から依存のある複数タスク(通勤→仕事→帰宅)を生成。`habit_steps` テーブルで各ステップの設定を個別に持ち、ステップ間は任意の DAG で依存を張れるようにする。
- **window_mode**: 週1などの habit で「期間内のどこにでもスケジュールしていい」タスクを生成する `window_mode = 'period'` を追加する(window = occurrence〜次の occurrence 直前)。

実装は #303(小) → #95(大) → window_mode の順で、それぞれ独立した PR にする。

## 前提(現状の仕組み)

- habit は `habits` テーブル1行 = 1日1タスク生成。`sync_habit_tasks`
  (`crates/takusu-local-lib/src/app.rs`) が14日先まで `(habit_id, 日付)` キーで
  生成/更新/削除を同期する。
  - pending かつ `user_edited = false` のタスクだけが habit 側の変更で上書き/削除される。
- タスク依存は `tasks.depends` (JSON配列のタスクID) で既にスケジューラ対応済み
  (`build_planner` で解決+循環検出)。
- SQLite マイグレーションは `storage_sqlite.rs` に `include_str!` + 冪等ガードで適用、
  D1 は `crates/takusu-worker/migrations/`(wrangler)。現在 008 まで。
- `takusu-core::Task::habit_group` (#306) が habit 単位の時刻一貫性ボーナスを与えている。

## Part 1: #303 habit の期間指定無効化

### マイグレーション `009_habit_pauses.sql`(local-lib / worker 両方)

```sql
CREATE TABLE habit_pauses (
    id         TEXT PRIMARY KEY,
    habit_id   TEXT NOT NULL REFERENCES habits(id) ON DELETE CASCADE,
    start_date TEXT NOT NULL,  -- YYYY-MM-DD (両端含む, ユーザーtzローカル日付)
    end_date   TEXT NOT NULL,
    reason     TEXT,
    created_at TEXT NOT NULL
);
CREATE INDEX idx_habit_pauses_habit ON habit_pauses(habit_id);
```

### モデル / Storage

- `HabitPauseRow` を `takusu-storage/src/model.rs` に追加。
- Storage trait に追加: `list_habit_pauses(habit_id)` / `list_all_habit_pauses()` /
  `create_habit_pause` / `delete_habit_pause`。
- SqliteStorage / WorkersStorage / worker 側 D1 ハンドラの3箇所に実装。
- バリデーション: `start_date <= end_date`、`YYYY-MM-DD` 形式。

### sync との統合

- `sync_habit_tasks` で expected 生成後、その habit の pause 期間に日付が入る
  occurrence を除外する。
- 既存の cleanup ループが「期待されなくなった pending・未編集タスク」を自動削除するので、
  **休暇設定 → 生成済みタスクも自動で消える**。編集済み/進行中タスクは既存ルール通り保護。

### API

- `GET /api/habits/:id/pauses` — pause 一覧
- `POST /api/habits/:id/pauses` — 作成 (`start_date`, `end_date`, `reason?`)
- `DELETE /api/habits/:id/pauses/:pause_id` — 削除

local / worker / client の3箇所に追加。

### CLI

- `habit pause <id> --from 2026-08-01 --to 2026-08-07 [--reason 休暇]`
- `habit pause list <id>`
- `habit pause rm <pause_id>`
- `habit show` に pause 一覧を表示。

### テスト

- pause 期間内の occurrence が生成されない
- pause 追加で既存 pending 未編集タスクが削除される
- 編集済み / 非 pending タスクは保護される
- 期間バリデーション(逆転、形式不正)

## Part 2: #95 一つの habit から依存のある複数タスク生成

### マイグレーション `010_habit_steps.sql`(local-lib / worker 両方)

```sql
CREATE TABLE habit_steps (
    id              TEXT PRIMARY KEY,
    habit_id        TEXT NOT NULL REFERENCES habits(id) ON DELETE CASCADE,
    position        INTEGER NOT NULL,      -- 表示順
    title           TEXT NOT NULL,
    description     TEXT,
    start_time      TEXT NOT NULL,         -- HH:MM (ステップ個別のwindow)
    end_time        TEXT NOT NULL,
    avg_minutes     INTEGER NOT NULL,
    sigma_minutes   INTEGER NOT NULL DEFAULT 0,
    parallelizable  BOOLEAN NOT NULL DEFAULT 0,
    allows_parallel BOOLEAN NOT NULL DEFAULT 0,
    abandonability  REAL NOT NULL DEFAULT 0.0,
    fixed           BOOLEAN NOT NULL DEFAULT 0,
    depends_on      TEXT NOT NULL DEFAULT '[]'  -- JSON配列: 同habit内のstep id (DAG)
);
ALTER TABLE tasks ADD COLUMN habit_step_id TEXT REFERENCES habit_steps(id);
```

### セマンティクス

- steps が 0 個 = 従来のシンプルモード(**完全後方互換**)。
- steps がある場合、habit 本体の window / avg / sigma / parallelizable 等は無視し、
  recurrence・active・pause だけ habit 側を使う。各ステップが自分の
  start_time/end_time/avg/sigma/フラグ類を持つ(issue の「task の設定は個別でいじれる」要件)。

### DAG 検証

- step 作成/更新時に循環検出(`build_planner` と同様の DFS)。
- `depends_on` は同一 habit 内の step id のみ許可。

### sync の変更

- 同期キーを `(habit_id, step_id: Option<String>, 日付)` に拡張し、
  タスク側は `habit_step_id` で step と紐付ける。
- occurrence 日付ごとに step 数分のタスクを生成:
  1. step をトポロジカル順に create/update
  2. 同日の step 間依存を task の `depends` に書き込む
     (step id → 生成した task id の対応表で解決)
- 更新・削除の保護ルール(pending かつ未編集のみ)は従来通り。
- step が削除された場合、対応する既存タスクは cleanup ループで削除される
  (保護ルール適用)。

### habit_group(時刻一貫性ボーナス #306)

- グループキーを habit 単位から `(habit_id, step_id)` 単位に変更し、
  ステップごとに毎日同じ時刻帯に寄るようにする。

### API

- habit レスポンス(GET)に `steps: Vec<HabitStep>` を含める。
- `PUT /api/habits/:id/steps` — 配列一括置換:
  - 既存 step は `id` 指定で維持(生成済みタスクとの紐付けを保つ)
  - `id` なしの要素は新規作成
  - 配列に含まれない既存 step は削除
- local / worker / client の3箇所に追加。

### CLI / editor

`habit edit` の editor フォーマットに step ブロックを追加:

```
[step 1]
title: 通勤
start_time: 08:00
end_time: 09:30
avg_minutes: 60
depends:              # 空 = 依存なし
[step 2]
title: 仕事
start_time: 09:00
end_time: 18:30
avg_minutes: 480
depends: 1            # step番号で参照 (DAGなので複数可: 1,3)
```

- parse 時に step 番号 → step id に解決(既存 step は id を保持)。
- `habit show` で step 一覧+依存を表示。

### テスト

- 複数 step 生成+depends 連結(同日タスク間の依存が正しく張られる)
- DAG 循環の拒否
- step 編集で pending 未編集タスクのみ更新される
- step 削除で対応タスクが cleanup される
- steps なし habit の後方互換(既存テストが通ること)
- pause(#303)× steps の組み合わせ(pause 期間は全 step 生成されない)

## Part 3: window_mode — 期間内のどこでもスケジュール可能な habit タスク

週1などの habit で「その週のどこにでもスケジュールしていい」タスクを生成する。
スケジューラ(takusu-core)は task の `start`〜`end`(deadline) の範囲内なら自由に
配置できるので、**生成時に window を1日ではなく期間全体に広げるだけで実現できる**
(コア側の変更は不要)。

### マイグレーション `011_window_mode.sql`(local-lib / worker 両方)

```sql
ALTER TABLE habits ADD COLUMN window_mode TEXT NOT NULL DEFAULT 'day';
-- 'day'    = 従来通り occurrence 当日の start_time〜end_time
-- 'period' = occurrence から次の occurrence 直前までが window
```

### セマンティクス(次 occurrence 方式)

- **window 開始** = occurrence 日の `start_time`。ただし進行中の期間(今日を含む期間)は
  今日の 0 時にクランプする(#204/#205 の「過去開始で planner が別日に再配置する」
  問題の再発防止)。
- **deadline** = 次の occurrence の直前。`RecurrenceGenerator` で次の occurrence を
  1つ先読みして計算する(生成 window の `until` を超えて先読みする)。
  `count` 制限などで次の occurrence が存在しない場合は
  occurrence + interval 分(freq 相当の期間)をフォールバックとする。
- この定義により「週1」だけでなく「隔週」「月1」でもそのまま動く
  (期間 = occurrence 間の間隔)。ISO週/カレンダー月方式は週開始定義の設定が
  必要になり隔週で曖昧になるため採用しない。
- **dedup キー**: タスク開始日ではなく **occurrence 日付** で固定
  (クランプしても重複生成しない)。sync キーの「日付」成分を occurrence 日付と
  明確に定義する。
- **habit_group(時刻一貫性ボーナス)**: `period` モードでは `None`
  (毎日同じ時刻帯に寄せる意味がないため)。
- **#95 steps との関係**: window_mode は **habit 単位のみ**。steps あり habit では
  全 step が同じ期間 window を共有し、step 間は依存(`depends`)だけで順序付けされる。
  step 個別の start_time/end_time は `day` モードでのみ意味を持つ
  (`period` モードでは期間全体に展開)。
- **#303 pause との関係**: occurrence 日付が pause 期間内なら期間タスクごと生成しない。

### API / CLI

- `CreateHabit` / `UpdateHabit` に `window_mode: Option<String>`(`"day"` / `"period"`、
  バリデーションあり)を追加。habit レスポンスに含める。
- CLI: `habit create --window period` / `habit update --window day`。
- editor フォーマットに `window_mode:` 行を追加。
- `habit show` / `habit list` に表示。

### テスト

- 週1 habit で window が occurrence〜次 occurrence 直前になる
- 隔週・月1 での期間計算
- 進行中期間のクランプ(今日 0 時開始、dedup キーは occurrence 日付のまま)
- `period` モードで habit_group が付かない
- pause 期間内の occurrence は期間タスクごと生成されない
- `day` モードの後方互換(既存テストが通ること)

## Mobile UI(mobile/ Expo アプリ)

既存部品を再利用する: `HabitDetailView` の編集モード + kebab メニュー、
`DateTimePickerModal`、`RruleBuilderModal`、abandonability スライダー、
`CancelConfirmButton`(2段階確認)、undo/redo スタック。
アクセントは `BRAND_COLOR` (#7261A3)。

### #303 休止期間

`HabitDetailView` に「休止期間」セクションを追加(説明の下):

```
休止期間
┌─────────────────────────────┐
│ ⏸ 8/1 〜 8/7   夏休み      ✕ │   ← 現在実行中の休止は色を変えてハイライト
│ ⏸ 12/29 〜 1/3              ✕ │
└─────────────────────────────┘
[+ 休止期間を追加]
```

- 各行: 期間 + reason + 削除ボタン(✕)。削除は **2段階確認**
  (既存 `CancelConfirmButton` と同じ tap → confirm の2tap方式)。
  削除/追加は undo/redo スタックに積む(`toggleActive` と同パターン)。
- **現在実行中の休止期間**(今日が from〜to 内)は行の色を変えてハイライト。
- 「+ 追加」→ モーダル: `DateTimePickerModal`(date モード)× 2(from/to)+
  reason テキスト入力 + 確定。`from <= to` をクライアント側でもバリデーション。
- kebab メニューに「休止期間を追加...」ショートカット
  (無効化 = 無期限、休止 = 期間指定という整理)。
- `HabitView` カード: 現在休止中の habit は inactive 同様に淡色表示 +
  「⏸ 〜8/7」バッジ。将来の休止予定はカードには出さない(detail で見る)。

### #95 ステップ

`HabitDetailView` に「ステップ」セクションを追加。

閲覧モード — position 順のステップカードリスト:

```
ステップ
① 通勤   08:00-09:30  60m
② 仕事   09:00-18:30  480m±30  依存: ①
③ 帰宅   17:00-20:00  60m      依存: ②
```

編集モード:

- 各ステップカードはアコーディオン展開で habit と同じフィールド一式
  (title / start・end time picker / avg・sigma / parallel チェック /
  abandonability スライダー / fixed)を編集。
- 並べ替えは上下ボタン(position 変更)。削除はカード内ゴミ箱アイコン +
  確認ダイアログ(生成済みタスクが消える旨を表示)。
- **依存の編集**: 展開内の「依存」行をタップ → 他ステップの
  **チェックボックスリスト**(DAG なので複数選択可)。クライアント側で
  循環検出して循環になる選択肢は disable、サーバー側バリデーションは
  フォールバック。
- 最下部に「+ ステップを追加」。
- ステップが1つ以上あると habit 本体の時間帯/コスト/parallel/fixed
  セクションはグレーアウトし「ステップ設定が優先されます」の注記
  (recurrence・active・休止は habit 側のまま有効)。
- 保存は既存編集モードの ✓ ボタンに統合し `PUT /api/habits/:id/steps` で
  一括置換(既存 step は id 維持)。
- **`HabitAddView` にもステップ追加 UI を載せる**(detail と同じ
  アコーディオン編集部品を共有コンポーネント化)。作成フローは
  habit 本体 POST → steps PUT の2段階だが UI 上は1画面で完結させる。
- `HabitView` カードにステップ数バッジ「3 steps」。
- `TaskDetailView`: 既存の habit 行にステップ名を追加表示
  (`habit: 仕事 › 通勤`)。

### window_mode

`HabitDetailView` / `HabitAddView` に SegmentedButtons (paper):

```
スケジュール枠   [ 当日 | 期間内どこでも ]
```

- 「期間内どこでも」選択時:
  - end_time picker を disable し、ヘルパーテキスト
    「次の周期の直前が締め切りになります」を表示
    (start_time は window 開始として有効なまま)。
  - steps あり habit では全 step が期間 window を共有する旨の注記。
- `HabitView` カード: period モードには小チップ **「自由枠」**
  (icon: calendar-range)。「期間内」だけでは単体で意味が通らないため。
- `HomeView` の TaskCard: period タスクは window 開始日の位置に表示されるが、
  コスト表示の近くに締め切りヒント「〜8/10」を小さく表示
  (複数日 window であることが分かるように)。

### 共通(実装面)

- `mobile/src/api/types.ts` / `client.ts` に `HabitPauseRow`・`HabitStep`・
  `window_mode` と対応エンドポイントを追加。
- pause / step の CRUD は既存パターン通り undo/redo スタックに登録。

## 実装順序と検証

1. **PR 1 (#303)**: migration 009 + storage + sync 除外ロジック + API + CLI + テスト
2. **PR 2 (#95)**: migration 010 + storage + DAG検証 + sync 拡張 + API + editor/CLI + テスト
   (大きければ storage/sync と API/CLI で分割)
3. **PR 3 (window_mode)**: migration 011 + sync の window 計算変更 + API + CLI + テスト
4. **PR 4〜 (mobile)**: サーバー側が揃った機能から順に mobile UI を実装
   (#303 → #95 → window_mode。types/client 追加 + 各 view 変更 +
   `npm run lint` / `fmt:check` / typecheck で検証)

各 PR:

- `cargo nextest run --workspace` / `cargo clippy` / `cargo fmt` で検証
- `jj describe` → `jj git push --change` → `gh pr create`
- PR 本文に `Closes #303` / `Closes #95` を記載
