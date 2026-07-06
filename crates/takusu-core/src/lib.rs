//! # takusu-core — schedule planner
//!
//! ユーザーのタスク集合から自動スケジュールを構築するコアライブラリ。
//! 焼きなまし法 (SA) + 大規模近傍探索 (LNS) + Tabu Search で最適化する。
//!
//! ## 概要
//!
//! ```no_run
//! use takusu_core::{Planner, NormalDist, Point, SleepConfig, Task};
//! use jiff::Timestamp;
//!
//! let mut planner = Planner::new(Point::now(5), SleepConfig::disabled());
//!
//! // 軽量なタスク追加
//! let task_id = planner.add(Task {
//!     id: 0,
//!     start: Some(Point::from_raw(0)),
//!     end: Point::from_raw(100),
//!     cost_estimate: NormalDist::new(10, 2),
//!     depends: vec![],
//!     parallelizable: false,
//!     allows_parallel: false,
//!     abandonability: 0.5,
//!     fixed: false,
//!     habit_group: None,
//! }).unwrap();
//!
//! let plan = planner.plan();
//! if let Some(start) = plan.task_start(task_id) {
//!     println!("task {task_id} starts at slot {}", start.0);
//! }
//! ```
//!
//! ## 時間の単位
//!
//! すべての時間は `Point` (i64) で表現する。
//! 1 単位 = 5 分。`Point::from_timestamp(ts, 5)` で jiff の Timestamp から変換。
//! `Point::from_raw(n)` でスロット値から直接生成。
//!
//! ## 睡眠
//!
//! `SleepConfig::recommended()` で 22:00-06:00 (8時間) の標準設定が得られる。
//! `SleepConfig::disabled()` で睡眠制約なし。

mod anneal;
pub mod evaluate;
mod solver;

use std::collections::HashMap;

use jiff::Timestamp;
use rustc_hash::FxHashSet;
use thiserror::Error;

// ── Point ────────────────────────────────────────────────────────────

/// 離散時間点。1単位 = 5分。
///
/// `Point(i64)` で、`i64` はエポックからの 5 分スロット数。
/// `Point(0)` が Timestamp(0) = UNIX エポック。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Point(pub i64);

impl Point {
    /// jiff の `Timestamp` から `per` 分単位の Point に変換。
    /// 通常 `per` は 5。
    pub fn from_timestamp(ts: Timestamp, per: u16) -> Point {
        Point(ts.as_second() / per as i64 / 60)
    }

    /// 現在時刻の Point。
    pub fn now(per: u16) -> Self {
        Self::from_timestamp(Timestamp::now(), per)
    }

    /// スロット値から Point を生成。`Point::from_raw(12)` = 60 分後。
    pub fn from_raw(n: i64) -> Self {
        Point(n)
    }

    /// 絶対値の差 (符号なし)。
    pub fn diff(lhs: Point, rhs: Point) -> i64 {
        (lhs.0 - rhs.0).abs()
    }

    /// 符号付きの差。`lhs - rhs`。前後関係の判定に使う。
    pub fn delta(lhs: Point, rhs: Point) -> i64 {
        lhs.0 - rhs.0
    }
}

impl std::ops::Add<i64> for Point {
    type Output = Point;
    fn add(self, rhs: i64) -> Point {
        Point(self.0 + rhs)
    }
}

impl std::ops::Sub<i64> for Point {
    type Output = Point;
    fn sub(self, rhs: i64) -> Point {
        Point(self.0 - rhs)
    }
}

// ── NormalDist ────────────────────────────────────────────────────────

/// 正規分布（平均と標準偏差）。タスクの所要時間見積りに使う。
///
/// - `sigma = 0`: 確定タスク（予定など）
/// - `sigma` 大: 不安定なタスク。後ろにバッファが取られる
///
/// `avg`/`sigma` の単位は 5 分スロット数。
#[derive(Debug, Clone, Copy)]
pub struct NormalDist {
    pub avg: u64,
    pub sigma: u64,
}

impl NormalDist {
    /// `avg` スロット、`sigma` スロットの正規分布。
    pub fn new(avg: u64, sigma: u64) -> Self {
        Self { avg, sigma }
    }
}

// ── SleepConfig ───────────────────────────────────────────────────────

/// 睡眠設定。
///
/// 一日の基点 (`day_start`) からの相対スロット数で睡眠時間帯を指定する。
/// 例: `day_start=0` (0:00基点), `start=264` (22:00), `end=360` (翌6:00) → 8時間睡眠。
#[derive(Debug, Clone, Copy)]
pub struct SleepConfig {
    /// 一日の基点 (エポックからのスロット)。通常 0。
    pub day_start: i64,
    /// 睡眠開始 (基点からの相対スロット)。
    pub start: i64,
    /// 睡眠終了 (基点からの相対スロット)。end > start。
    pub end: i64,
    /// 睡眠制約が有効かどうか。
    pub enabled: bool,
}

impl SleepConfig {
    /// 推奨設定: 22:00–06:00 (8時間), 一日は 0:00 基点。
    pub fn recommended() -> Self {
        Self {
            day_start: 0,
            start: 264, // 22 * 12
            end: 360,   // 30 * 12 = 6:00 next day
            enabled: true,
        }
    }

    /// 睡眠制約なし。
    pub fn disabled() -> Self {
        Self {
            day_start: 0,
            start: 0,
            end: 0,
            enabled: false,
        }
    }

    /// タイムゾーンとローカル時計時刻から SleepConfig を構築。
    ///
    /// `per` は 1 スロットの分数 (通常 5)。`tz` は jiff タイムゾーン。
    /// `start_h`/`start_m` と `end_h`/`end_m` はローカル時刻による睡眠窓。
    /// 日跨ぎ (例: 22:00–06:00) は自動で処理される。
    pub fn from_local(
        per: u16,
        tz: &jiff::tz::TimeZone,
        start_h: u8,
        start_m: u8,
        end_h: u8,
        end_m: u8,
    ) -> Self {
        let slots_per_hour: i64 = 60 / per as i64;
        let slots_per_day: i64 = 24 * slots_per_hour;

        let offset_secs: i64 = tz.to_offset(jiff::Timestamp::now()).seconds().into();
        let offset_slots = offset_secs / (per as i64 * 60);

        let day_start = (slots_per_day - offset_slots).rem_euclid(slots_per_day);

        let start = start_h as i64 * slots_per_hour + start_m as i64 / per as i64;
        let mut end = end_h as i64 * slots_per_hour + end_m as i64 / per as i64;

        if end <= start {
            end += slots_per_day;
        }

        Self {
            day_start,
            start,
            end,
            enabled: true,
        }
    }
}

impl Default for SleepConfig {
    fn default() -> Self {
        Self::disabled()
    }
}

// ── Task ──────────────────────────────────────────────────────────────

/// プランナーに渡すタスク。
///
/// タスクは 5 分スロットに離散化された時間軸上に配置される。
/// `start <= task < end`。
#[derive(Debug, Clone)]
pub struct Task {
    /// タスク ID。add_task 時に自動設定されるが、外部で管理したい場合は任意の値。
    pub id: usize,

    /// 開始可能時間。None の場合は即時開始可能。
    pub start: Option<Point>,

    /// 締切。この時刻までに終了している必要がある。
    pub end: Point,

    /// 所要時間の見積り (正規分布)。
    pub cost_estimate: NormalDist,

    /// 依存タスクの ID リスト。これらのタスクがすべて終了してから開始可能。
    pub depends: Vec<usize>,

    /// 他のタスク実行中でも実行可能か (例: スマホでできるタスク)。
    pub parallelizable: bool,

    /// このタスク実行中に他のタスクの並行実行を許すか (例: 電車移動)。
    pub allows_parallel: bool,

    /// 諦めやすさ [0.0, 1.0]。大きいほど諦められやすい。
    /// 全タスクが収まらない場合、この値が大きいタスクからドロップされる。
    pub abandonability: f64,

    /// 開始時刻を固定するか。true の場合、Planner は now 以前の
    /// 配置も許可し、SA の近傍操作でも移動しない。
    /// 学校など開始時刻が厳密なタスクに使う。
    pub fixed: bool,

    /// #306: Habit 由来のタスクの場合、habit グループのインデックス。
    /// 同じ habit_id のタスクは日ごとに近い時刻に配置されるとボーナス。
    /// 非 habit タスクは None。
    pub habit_group: Option<usize>,
}

// ── Plan ──────────────────────────────────────────────────────────────

/// プランナーの出力。タスクの割り当て結果。
///
/// タスクは常に全数スケジュールされる。
/// `abandonability` が高いタスクは deadline 超過が許容されるが、諦められない。
#[derive(Debug, Clone)]
pub struct Plan {
    /// スケジュールされたタスク。各要素は `(開始slot, 終了slot, task_id)`。
    pub schedules: Vec<(Point, Point, usize)>,
}

impl Plan {
    /// タスクの開始時刻。
    pub fn task_start(&self, task_id: usize) -> Option<Point> {
        self.schedules
            .iter()
            .find(|(_, _, id)| *id == task_id)
            .map(|(s, _, _)| *s)
    }

    /// タスクの終了時刻。
    pub fn task_end(&self, task_id: usize) -> Option<Point> {
        self.schedules
            .iter()
            .find(|(_, _, id)| *id == task_id)
            .map(|(_, e, _)| *e)
    }

    /// タスクがスケジュールされているか（常に true のはず）。
    pub fn is_scheduled(&self, task_id: usize) -> bool {
        self.schedules.iter().any(|(_, _, id)| *id == task_id)
    }
}

// ── RescheduleRange ───────────────────────────────────────────────────

/// 部分再スケジュールの期間指定。
#[derive(Debug, Clone, Copy)]
pub struct RescheduleRange {
    /// 期間の開始 (このスロット以降に開始されるタスクが再スケジュール対象)。
    pub from: Point,
    /// 期間の終了 (このスロット以前に終了されるタスクが再スケジュール対象)。
    pub until: Point,
}

// ── Error ─────────────────────────────────────────────────────────────

/// プランナーのエラー。
#[derive(Debug, Error)]
pub enum Error {
    /// 開始可能時刻が締切より後。
    #[error("The start is {0:?} but the end is {1:?} which is earlier than the start")]
    LateStart(Point, Point),
}

type ResultE<T> = Result<T, Error>;

// ── Planner ───────────────────────────────────────────────────────────

/// スケジュールプランナー。タスクを登録して `plan()` でスケジュールを得る。
///
/// ## 使用例
///
/// ```
/// use takusu_core::{Planner, Task, NormalDist, Point, SleepConfig};
///
/// let mut p = Planner::new(Point::from_raw(0), SleepConfig::disabled());
///
/// p.add(Task {
///     id: 0,
///     start: Some(Point::from_raw(0)),
///     end: Point::from_raw(20),
///     cost_estimate: NormalDist::new(5, 0),
///     depends: vec![],
///     parallelizable: false,
///     allows_parallel: false,
///     abandonability: 0.5,
///     fixed: false,
///     habit_group: None,
/// }).unwrap();
///
/// let plan = p.plan();
/// assert!(plan.is_scheduled(0));
/// ```
#[derive(Debug)]
pub struct Planner {
    tasks: Vec<Task>,
    now: Point,
    per: u16,
    sleep: SleepConfig,
    /// #211: 前回スケジュールの参照（安定性ペナルティ用）。
    /// 各タスクの (start, end) で、SAが移動を嫌うようにする。
    /// 直近のタスクほど強いペナルティ。
    previous_schedule: Vec<Option<(Point, Point)>>,
}

impl Planner {
    /// 新しいプランナーを作成。
    ///
    /// - `now`: 現在時刻 (これより前にタスクを配置しない)
    /// - `sleep`: 睡眠設定。`SleepConfig::recommended()` または `SleepConfig::disabled()`
    pub fn new(now: Point, sleep: SleepConfig) -> Self {
        Self {
            tasks: vec![],
            now,
            per: 5,
            sleep,
            previous_schedule: vec![],
        }
    }

    /// タスクを登録。戻り値は登録されたタスク ID (= `self.tasks.len() - 1`)。
    ///
    /// `task.id` は内部的に上書きされる。外部で ID を管理したい場合は
    /// `add()` の戻り値を保持すること。
    pub fn add(&mut self, task: Task) -> ResultE<usize> {
        let id = self.tasks.len();

        if let Some(start) = task.start
            && start > task.end
        {
            return Err(Error::LateStart(start, task.end));
        }

        self.tasks.push(Task { id, ..task });

        Ok(id)
    }

    /// スケジュールを計算して返す。
    ///
    /// 内部では 4 本の独立 SA チェーンを並列実行し最良解を選択する。
    /// 全タスクがスケジュールされる。`abandonability` が高いタスクは
    /// deadline 超過ペナルティが軽減されるが、ドロップはされない。
    ///
    /// `previous_schedule` が設定されている場合、直近のタスクを
    /// 前回位置から動かすことにペナルティを課す (#211)。
    pub fn plan(&self) -> Plan {
        solver::solve(self)
    }

    /// #211: 前回スケジュールを設定し、安定性ペナルティを有効化する。
    /// `schedule` は (start, end, task_id) のリスト。
    /// 設定後、plan() は前回位置からの移動を嫌うようになる。
    /// 直近（now に近い）ほどペナルティが大きい。
    pub fn set_previous_schedule(&mut self, schedule: &[(Point, Point, usize)]) {
        self.previous_schedule = vec![None; self.tasks.len()];
        for (s, e, id) in schedule {
            if *id < self.previous_schedule.len() {
                self.previous_schedule[*id] = Some((*s, *e));
            }
        }
    }

    /// 前回スケジュールの参照（評価関数から使用）。
    pub fn previous_schedule(&self) -> &[Option<(Point, Point)>] {
        &self.previous_schedule
    }

    /// 固定タスクを保持したまま未固定タスクをスケジュール。
    ///
    /// `pinned` に含まれるタスクは指定位置に固定され、近傍操作の対象外。
    /// 未固定タスクのみがSAで配置される。評価関数は固定・未固定両方を考慮する。
    pub fn plan_partial(&self, pinned: &[(Point, Point, usize)]) -> Plan {
        solver::solve_partial(self, pinned)
    }

    /// 指定期間内のタスクのみ再スケジュール。
    ///
    /// `current_schedule` に含まれるタスクのうち、期間外のものを固定とみなす。
    /// `pinned` に追加で固定したいタスクも指定できる。
    /// 期間内 (`range.from <= start` かつ `end <= range.until`) のタスクのみがSAで再配置される。
    ///
    /// # Known limitation
    /// 依存関係が固定→再配置タスクの方向 (pinned が sub-planner 内のタスクに依存) の場合、
    /// その依存はチェックされない。sub-planner は pinned タスクを一切持たないため。
    /// 現実的には pinned タスクは期間外で既に完了していることが多く、この問題は顕在化しにくい。
    /// 逆方向 (再配置→固定) は remap 時に depend から filtered out されるため安全。
    pub fn plan_in_range(
        &self,
        range: &RescheduleRange,
        current_schedule: &[(Point, Point, usize)],
        extra_pinned: &[usize],
    ) -> Plan {
        let mut pinned: Vec<(Point, Point, usize)> = Vec::new();
        let mut pinned_ids: FxHashSet<usize> = extra_pinned.iter().copied().collect();

        for (s, e, id) in current_schedule {
            let out_of_range = e.0 <= range.from.0 || s.0 >= range.until.0;
            if out_of_range || extra_pinned.contains(id) {
                pinned.push((*s, *e, *id));
                pinned_ids.insert(*id);
            }
        }

        let mut sub_planner = Planner::new(self.now, self.sleep);
        let mut sub_to_orig: Vec<usize> = Vec::new();
        for task in &self.tasks {
            if !pinned_ids.contains(&task.id) {
                sub_to_orig.push(task.id);
                sub_planner.add(task.clone()).ok();
            }
        }

        let mut orig_to_sub: HashMap<usize, usize> = HashMap::new();
        for (sub_idx, &orig_id) in sub_to_orig.iter().enumerate() {
            orig_to_sub.insert(orig_id, sub_idx);
        }

        for task in sub_planner.tasks_mut() {
            task.depends = task
                .depends
                .iter()
                .filter_map(|orig_dep| orig_to_sub.get(orig_dep).copied())
                .collect();
        }

        let sub_plan = solver::solve_partial(&sub_planner, &[]);

        let mut schedules = pinned;
        for (s, e, sub_id) in &sub_plan.schedules {
            let orig_id = sub_to_orig[*sub_id];
            schedules.push((*s, *e, orig_id));
        }
        Plan { schedules }
    }

    /// 登録された全タスクを返す。
    pub fn tasks(&self) -> &[Task] {
        &self.tasks
    }

    pub fn tasks_mut(&mut self) -> &mut [Task] {
        &mut self.tasks
    }

    /// 1スロットの分数 (通常5)。
    pub fn per(&self) -> u16 {
        self.per
    }

    /// タスクの「余裕度」を返す [0.0, 1.0]。
    /// 値が大きい = 余裕がある = 優先度が低い。
    /// 値が小さい = 切迫している = 優先度が高い。
    ///
    /// # Counterintuitive naming
    /// 名前は「free」だが、値が大きいほど deprioritized される。
    /// 低 freeness → 締切までの slack が小さい → build_initial で先に配置。
    /// `freeness()` の結果でソートし、値が小さい順に greedy 配置される。
    /// 「freeness」＝「(slack - avg) / slack」のイメージ。
    pub(crate) fn freeness(&self, id: usize) -> f64 {
        let slack = Point::diff(
            self.tasks[id].start.unwrap_or(Point(0)).max(self.now),
            self.tasks[id].end,
        );
        if slack == 0 {
            return 0.;
        }
        1. - (self.tasks[id].cost_estimate.avg as f64 / slack as f64)
    }
}

impl Default for Planner {
    fn default() -> Self {
        Self::new(Point(0), SleepConfig::disabled())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn planner_simple_two_tasks() {
        let mut p = Planner::default();
        let _a = p
            .add(Task {
                id: 0,
                start: Some(Point(0)),
                end: Point(5),
                cost_estimate: NormalDist::new(1, 0),
                depends: vec![],
                parallelizable: false,
                allows_parallel: false,
                abandonability: 0.5,
                fixed: false,
                habit_group: None,
            })
            .unwrap();
        let b = p
            .add(Task {
                id: 1,
                start: Some(Point(0)),
                end: Point(5),
                cost_estimate: NormalDist::new(1, 2),
                depends: vec![],
                parallelizable: false,
                allows_parallel: false,
                abandonability: 0.5,
                fixed: false,
                habit_group: None,
            })
            .unwrap();

        let plan = p.plan();
        assert_eq!(plan.schedules.len(), 2);
        assert!(plan.task_end(b).unwrap().0 <= 5);
    }

    #[test]
    fn planner_sleep_avoided() {
        let mut p = Planner::new(
            Point(0),
            SleepConfig {
                day_start: 0,
                start: 0,
                end: 96,
                enabled: true,
            },
        );
        p.add(Task {
            id: 0,
            start: Some(Point(0)),
            end: Point(200),
            cost_estimate: NormalDist::new(10, 0),
            depends: vec![],
            parallelizable: false,
            allows_parallel: false,
            abandonability: 0.5,
            fixed: false,
            habit_group: None,
        })
        .unwrap();

        let plan = p.plan();
        let sleep_occupied: i64 = plan
            .schedules
            .iter()
            .filter(|(s, e, _)| s.0 < 96 && e.0 > 0)
            .map(|(s, e, _)| {
                let o_start = s.0.max(0);
                let o_end = e.0.min(96);
                (o_end - o_start).max(0)
            })
            .sum();

        assert!(sleep_occupied < 96);
    }

    #[test]
    fn planner_deadline_miss_still_scheduled() {
        let mut p = Planner::default();
        p.add(Task {
            id: 0,
            start: Some(Point(0)),
            end: Point(0),
            cost_estimate: NormalDist::new(5, 0),
            depends: vec![],
            parallelizable: false,
            allows_parallel: false,
            abandonability: 0.9,
            fixed: false,
            habit_group: None,
        })
        .unwrap();

        let plan = p.plan();
        assert!(
            plan.is_scheduled(0),
            "task should be scheduled even if deadline is impossible. schedules={:?}",
            plan.schedules
        );
    }

    #[test]
    fn plan_convenience_methods() {
        let plan = Plan {
            schedules: vec![(Point(1), Point(3), 42)],
        };
        assert_eq!(plan.task_start(42), Some(Point(1)));
        assert_eq!(plan.task_end(42), Some(Point(3)));
        assert!(plan.is_scheduled(42));
        assert!(!plan.is_scheduled(99));
    }

    #[test]
    fn plan_partial_keeps_pinned() {
        let mut p = Planner::default();
        let a = p
            .add(Task {
                id: 0,
                start: Some(Point(0)),
                end: Point(20),
                cost_estimate: NormalDist::new(3, 0),
                depends: vec![],
                parallelizable: false,
                allows_parallel: false,
                abandonability: 0.5,
                fixed: false,
                habit_group: None,
            })
            .unwrap();
        let _b = p
            .add(Task {
                id: 1,
                start: Some(Point(0)),
                end: Point(20),
                cost_estimate: NormalDist::new(3, 0),
                depends: vec![],
                parallelizable: false,
                allows_parallel: false,
                abandonability: 0.5,
                fixed: false,
                habit_group: None,
            })
            .unwrap();

        let pinned = vec![(Point(0), Point(3), a)];
        let plan = p.plan_partial(&pinned);

        let pinned_start = plan.task_start(a).unwrap();
        let pinned_end = plan.task_end(a).unwrap();
        assert_eq!(
            pinned_start,
            Point(0),
            "pinned task start should be unchanged"
        );
        assert_eq!(pinned_end, Point(3), "pinned task end should be unchanged");
        assert_eq!(plan.schedules.len(), 2, "all tasks should be scheduled");
    }

    #[test]
    fn plan_partial_no_pinned_equals_plan() {
        let mut p = Planner::default();
        p.add(Task {
            id: 0,
            start: Some(Point(0)),
            end: Point(10),
            cost_estimate: NormalDist::new(2, 0),
            depends: vec![],
            parallelizable: false,
            allows_parallel: false,
            abandonability: 0.5,
            fixed: false,
            habit_group: None,
        })
        .unwrap();

        let plan_full = p.plan();
        let plan_partial = p.plan_partial(&[]);
        assert_eq!(
            plan_partial.schedules.len(),
            plan_full.schedules.len(),
            "plan_partial with no pinned should schedule all tasks"
        );
    }

    #[test]
    fn plan_in_range_reschedules_within_range() {
        let mut p = Planner::default();
        let _a = p
            .add(Task {
                id: 0,
                start: Some(Point(0)),
                end: Point(50),
                cost_estimate: NormalDist::new(5, 0),
                depends: vec![],
                parallelizable: false,
                allows_parallel: false,
                abandonability: 0.5,
                fixed: false,
                habit_group: None,
            })
            .unwrap();
        let _b = p
            .add(Task {
                id: 1,
                start: Some(Point(50)),
                end: Point(100),
                cost_estimate: NormalDist::new(5, 0),
                depends: vec![],
                parallelizable: false,
                allows_parallel: false,
                abandonability: 0.5,
                fixed: false,
                habit_group: None,
            })
            .unwrap();

        let current = p.plan();
        let range = RescheduleRange {
            from: Point(0),
            until: Point(50),
        };
        let replanned = p.plan_in_range(&range, &current.schedules, &[]);
        assert_eq!(
            replanned.schedules.len(),
            2,
            "all tasks should be scheduled"
        );
    }

    #[test]
    fn plan_in_range_preserves_task_ids_with_pinned_middle() {
        let mut p = Planner::default();
        let _a = p
            .add(Task {
                id: 0,
                start: Some(Point(0)),
                end: Point(100),
                cost_estimate: NormalDist::new(5, 0),
                depends: vec![],
                parallelizable: false,
                allows_parallel: false,
                abandonability: 0.5,
                fixed: false,
                habit_group: None,
            })
            .unwrap();
        let _b = p
            .add(Task {
                id: 1,
                start: Some(Point(0)),
                end: Point(100),
                cost_estimate: NormalDist::new(5, 0),
                depends: vec![],
                parallelizable: false,
                allows_parallel: false,
                abandonability: 0.5,
                fixed: false,
                habit_group: None,
            })
            .unwrap();
        let _c = p
            .add(Task {
                id: 2,
                start: Some(Point(0)),
                end: Point(100),
                cost_estimate: NormalDist::new(5, 0),
                depends: vec![],
                parallelizable: false,
                allows_parallel: false,
                abandonability: 0.5,
                fixed: false,
                habit_group: None,
            })
            .unwrap();

        let current_schedule = vec![
            (Point(0), Point(5), 0),
            (Point(10), Point(15), 1),
            (Point(50), Point(55), 2),
        ];
        let range = RescheduleRange {
            from: Point(5),
            until: Point(50),
        };
        let replanned = p.plan_in_range(&range, &current_schedule, &[]);
        assert_eq!(replanned.schedules.len(), 3);
        let ids: Vec<usize> = replanned.schedules.iter().map(|(_, _, id)| *id).collect();
        assert!(ids.contains(&0), "task 0 should be preserved");
        assert!(ids.contains(&1), "task 1 should be preserved");
        assert!(ids.contains(&2), "task 2 should be preserved");
        assert_eq!(
            replanned.task_start(0).unwrap(),
            Point(0),
            "pinned task 0 start should be unchanged"
        );
        assert_eq!(
            replanned.task_end(0).unwrap(),
            Point(5),
            "pinned task 0 end should be unchanged"
        );
        assert_eq!(
            replanned.task_start(2).unwrap(),
            Point(50),
            "pinned task 2 start should be unchanged"
        );
        assert_eq!(
            replanned.task_end(2).unwrap(),
            Point(55),
            "pinned task 2 end should be unchanged"
        );
    }

    #[test]
    fn plan_in_range_remaps_depends_correctly() {
        let mut p = Planner::default();
        let _a = p
            .add(Task {
                id: 0,
                start: Some(Point(20)),
                end: Point(100),
                cost_estimate: NormalDist::new(1, 0),
                depends: vec![],
                parallelizable: false,
                allows_parallel: false,
                abandonability: 0.5,
                fixed: false,
                habit_group: None,
            })
            .unwrap();
        let _b = p
            .add(Task {
                id: 1,
                start: Some(Point(0)),
                end: Point(100),
                cost_estimate: NormalDist::new(1, 0),
                depends: vec![0],
                parallelizable: false,
                allows_parallel: false,
                abandonability: 0.5,
                fixed: false,
                habit_group: None,
            })
            .unwrap();
        let _c = p
            .add(Task {
                id: 2,
                start: Some(Point(0)),
                end: Point(100),
                cost_estimate: NormalDist::new(1, 0),
                depends: vec![1],
                parallelizable: false,
                allows_parallel: false,
                abandonability: 0.5,
                fixed: false,
                habit_group: None,
            })
            .unwrap();

        let current_schedule = vec![
            (Point(20), Point(30), 0),
            (Point(10), Point(20), 1),
            (Point(30), Point(40), 2),
        ];
        let range = RescheduleRange {
            from: Point(0),
            until: Point(15),
        };
        // Task 0 is out of range (starts at 20, range ends at 15) → pinned.
        // Tasks 1 and 2 are in range → rescheduled in sub-planner.
        // Before remap: task 2.depends = [1] (original id), but in sub-planner idx 1 is task 1.
        // After remap: task 2.depends should be [1] (sub-planner idx).
        // Task 1.depends = [0] (original), but 0 is pinned → filtered out, depends becomes [].
        let replanned = p.plan_in_range(&range, &current_schedule, &[]);
        assert_eq!(replanned.schedules.len(), 3);
        let pinned_0 = replanned
            .schedules
            .iter()
            .find(|(_, _, id)| *id == 0)
            .unwrap();
        assert_eq!(pinned_0.0, Point(20), "task 0 pinned start unchanged");
        assert_eq!(pinned_0.1, Point(30), "task 0 pinned end unchanged");
    }

    #[test]
    fn plan_in_range_dep_chain_remap_self_dep_prevented() {
        let mut p = Planner::default();
        let _a = p
            .add(Task {
                id: 0,
                start: Some(Point(0)),
                end: Point(100),
                cost_estimate: NormalDist::new(1, 0),
                depends: vec![],
                parallelizable: false,
                allows_parallel: false,
                abandonability: 0.5,
                fixed: false,
                habit_group: None,
            })
            .unwrap();
        let _b = p
            .add(Task {
                id: 1,
                start: Some(Point(0)),
                end: Point(100),
                cost_estimate: NormalDist::new(1, 0),
                depends: vec![0],
                parallelizable: false,
                allows_parallel: false,
                abandonability: 0.5,
                fixed: false,
                habit_group: None,
            })
            .unwrap();
        let _c = p
            .add(Task {
                id: 2,
                start: Some(Point(0)),
                end: Point(100),
                cost_estimate: NormalDist::new(1, 0),
                depends: vec![1],
                parallelizable: false,
                allows_parallel: false,
                abandonability: 0.5,
                fixed: false,
                habit_group: None,
            })
            .unwrap();

        let current_schedule = vec![
            (Point(0), Point(10), 0),
            (Point(10), Point(20), 1),
            (Point(50), Point(60), 2),
        ];
        let range = RescheduleRange {
            from: Point(0),
            until: Point(30),
        };
        // Tasks 0 and 1 are in range → rescheduled.
        // Task 2 is out of range (starts at 50) → pinned.
        // Sub-planner: [task 0, task 1]. Task 1.depends = [0] → remapped to [0]. Correct.
        let replanned = p.plan_in_range(&range, &current_schedule, &[]);
        assert_eq!(replanned.schedules.len(), 3);
        let pinned_2 = replanned
            .schedules
            .iter()
            .find(|(_, _, id)| *id == 2)
            .unwrap();
        assert_eq!(pinned_2.0, Point(50), "task 2 pinned start unchanged");
    }

    #[test]
    fn point_arithmetic() {
        let p = Point(10);
        assert_eq!((p + 5).0, 15);
        assert_eq!((p - 3).0, 7);
        assert_eq!(Point::diff(Point(10), Point(20)), 10);
        assert_eq!(Point::delta(Point(20), Point(10)), 10);
    }

    #[test]
    fn point_from_raw() {
        let p = Point::from_raw(42);
        assert_eq!(p.0, 42);
    }

    #[test]
    fn normal_dist_new() {
        let nd = NormalDist::new(10, 3);
        assert_eq!(nd.avg, 10);
        assert_eq!(nd.sigma, 3);
    }

    #[test]
    fn normal_dist_sigma_can_exceed_avg() {
        let nd = NormalDist::new(5, 8);
        assert_eq!(nd.avg, 5);
        assert_eq!(nd.sigma, 8);
    }

    #[test]
    fn normal_dist_zero_avg() {
        let nd = NormalDist::new(0, 0);
        assert_eq!(nd.avg, 0);
        assert_eq!(nd.sigma, 0);
    }

    #[test]
    fn sleep_config_disabled() {
        let sc = SleepConfig::disabled();
        assert!(!sc.enabled);
    }

    #[test]
    fn sleep_config_recommended() {
        let sc = SleepConfig::recommended();
        assert!(sc.enabled);
    }

    #[test]
    fn task_add_assigns_id() {
        let mut planner = Planner::new(Point(0), SleepConfig::disabled());
        let id1 = planner
            .add(Task {
                id: 0,
                start: None,
                end: Point(100),
                cost_estimate: NormalDist::new(10, 2),
                depends: vec![],
                parallelizable: false,
                allows_parallel: false,
                abandonability: 0.5,
                fixed: false,
                habit_group: None,
            })
            .unwrap();
        let id2 = planner
            .add(Task {
                id: 0,
                start: None,
                end: Point(200),
                cost_estimate: NormalDist::new(5, 1),
                depends: vec![],
                parallelizable: false,
                allows_parallel: false,
                abandonability: 0.5,
                fixed: false,
                habit_group: None,
            })
            .unwrap();
        assert_eq!(id1, 0);
        assert_eq!(id2, 1);
    }

    #[test]
    fn task_add_updates_depend_indices() {
        let mut planner = Planner::new(Point(0), SleepConfig::disabled());
        planner
            .add(Task {
                id: 0,
                start: None,
                end: Point(100),
                cost_estimate: NormalDist::new(5, 0),
                depends: vec![],
                parallelizable: false,
                allows_parallel: false,
                abandonability: 0.5,
                fixed: false,
                habit_group: None,
            })
            .unwrap();
        planner
            .add(Task {
                id: 0,
                start: None,
                end: Point(200),
                cost_estimate: NormalDist::new(5, 0),
                depends: vec![0],
                parallelizable: false,
                allows_parallel: false,
                abandonability: 0.5,
                fixed: false,
                habit_group: None,
            })
            .unwrap();
        assert_eq!(planner.tasks()[1].depends, vec![0]);
    }

    #[test]
    fn freeness_returns_valid_range() {
        let mut planner = Planner::new(Point(0), SleepConfig::disabled());
        planner
            .add(Task {
                id: 0,
                start: None,
                end: Point(48),
                cost_estimate: NormalDist::new(6, 2),
                depends: vec![],
                parallelizable: false,
                allows_parallel: false,
                abandonability: 0.0,
                fixed: false,
                habit_group: None,
            })
            .unwrap();
        let f = planner.freeness(0);
        assert!((0.0..=1.0).contains(&f));
    }

    #[test]
    fn plan_is_scheduled() {
        let planner = simple_two_task_planner();
        let plan = planner.plan();
        assert!(plan.is_scheduled(0));
        assert!(plan.is_scheduled(1));
    }

    #[test]
    fn plan_task_start_end_not_scheduled() {
        let plan = Plan { schedules: vec![] };
        assert!(plan.task_start(0).is_none());
        assert!(plan.task_end(0).is_none());
        assert!(!plan.is_scheduled(0));
    }

    #[test]
    fn point_from_timestamp_and_now() {
        let ts = jiff::Timestamp::from_second(0).unwrap();
        let p = Point::from_timestamp(ts, 5);
        assert_eq!(p.0, 0);
    }

    #[test]
    fn evaluate_empty_schedule_is_inclusion_loss() {
        let planner = simple_two_task_planner();
        let plan = Plan { schedules: vec![] };
        let score = evaluate::evaluate(&planner, &plan, 0.0, 1.0);
        let full_plan = planner.plan();
        let full_score = evaluate::evaluate(&planner, &full_plan, 0.0, 1.0);
        assert!(full_score > score);
    }

    fn simple_two_task_planner() -> Planner {
        let mut planner = Planner::new(Point(0), SleepConfig::disabled());
        planner
            .add(Task {
                id: 0,
                start: None,
                end: Point(100),
                cost_estimate: NormalDist::new(10, 2),
                depends: vec![],
                parallelizable: false,
                allows_parallel: false,
                abandonability: 0.5,
                fixed: false,
                habit_group: None,
            })
            .unwrap();
        planner
            .add(Task {
                id: 0,
                start: None,
                end: Point(200),
                cost_estimate: NormalDist::new(10, 2),
                depends: vec![],
                parallelizable: false,
                allows_parallel: false,
                abandonability: 0.5,
                fixed: false,
                habit_group: None,
            })
            .unwrap();
        planner
    }
}
