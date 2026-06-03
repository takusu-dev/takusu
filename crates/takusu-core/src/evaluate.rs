//! # 評価関数 (Evaluation Function)
//!
//! スケジュール `Plan` をスカラー値に写像する。最大化すべき値。
//!
//! ```text
//! E(plan, T) = Σ deadline_score(i)      // 締切充足
//!             + Σ start_score(i)         // 開始可能時間制約
//!             + Σ depend_score(i, T)    // 依存関係 (constraint annealing)
//!             + Σ buffer_score(i)        // 不確実性バッファ報酬
//!             + Σ duration_score(i)     // 所要時間マッチ
//!             + Σ sleep_score(d)         // 日ごと睡眠評価
//!             + Σ parallel_violation     // 並列違反
//!             + inclusion_bonus          // スケジュール存在ボーナス
//! ```
//!
//! ## 各項の詳細
//!
//! ### deadline_score
//! - slack >= 0: `min(slack * W_EARLY, 早期報酬上限)` — 早く終わるほどボーナス(上限あり)
//! - slack < 0:  `slack * W_LATE` — 締切超過ペナルティ (|W_LATE| ≫ W_EARLY)
//!
//! ### start_score
//! - 開始可能時刻なし または 開始可能時刻以後 → 0
//! - それ以外 → `(scheduled_start - start) * W_START` (負)
//!
//! ### depend_score (constraint annealing, 違反スロット数比例)
//! - 依存先タスクが終了していない場合:
//!   `-(違反スロット数) * W_DEPEND_BASE * (1.0 - T/T₀)`
//! - 温度 T が高いうちは違反ペナルティが小さい → 探索範囲が広がる
//! - T → 0 で最大ペナルティに収束 → 実行可能領域へ誘導
//! - 違反の大きさに比例するため、大きな依存違反ほど強く罰せられる
//!
//! ### buffer_score
//! - `task.sigma * 締切までの空きslot数 * W_BUFFER`
//! - sigma=0 の確定タスクはバッファ報酬なし
//! - sigmaが大きいタスクの後ろに空きがあるほど高スコア
//!
//! ### duration_score
//! - `deficit = avg - scheduled_duration`
//! - deficit > 0: `-deficit² * W_SHORT` — 見積り不足 (二次で急峻)
//! - deficit < 0: `deficit * W_OVER` — 取りすぎ (線形で軽微)
//!
//! ### sleep_score (per day, 3h threshold)
//! - ベース: `-sleep_used * W_SLEEP_NORMAL`
//! - 睡眠残りが MIN_SLEEP (3時間) を下回った場合:
//!   `-(MIN_SLEEP - sleep_got)² * W_SLEEP_SEVERE` (追加二次ペナルティ)
//!
//! ### parallel_violation (重複スロット数比例)
//! - 時間的重複があり、かつ並列条件を満たさないペア:
//!   `-(重複スロット数) * W_PARALLEL_VIOL`
//!
//! ### inclusion_bonus
//! - スケジュールされているタスクごとに `+W_INCLUSION`
//!
//! ## 重み設計
//! |W_DEPEND_BASE| ≫ |W_LATE| ≫ |W_START| > W_BUFFER > W_INCLUSION

use super::*;

const W_EARLY: f64 = 1.0;
const W_LATE: f64 = 20.0;
const W_START: f64 = 5.0;
const W_DEPEND_BASE: f64 = 100.0;
const W_BUFFER: f64 = 2.0;
const W_SHORT: f64 = 3.0;
const W_OVER: f64 = 0.5;
const W_SLEEP_NORMAL: f64 = 4.0;
const W_SLEEP_SEVERE: f64 = 15.0;
const W_PARALLEL_VIOL: f64 = 50.0;
const W_INCLUSION: f64 = 10.0;
const MIN_SLEEP: i64 = 36;

pub fn evaluate(planner: &Planner, plan: &Plan, temperature: f64, t0: f64) -> f64 {
    let mut score = 0.0;
    let schedules = &plan.schedules;

    score += deadline_score(planner, schedules);
    score += start_score(planner, schedules);
    score += depend_score(planner, schedules, temperature, t0);
    score += buffer_score(planner, schedules);
    score += duration_score(planner, schedules);
    score += sleep_score(planner, schedules);
    score += parallel_violation_score(planner, schedules);
    score += inclusion_bonus(planner, schedules);

    score
}

fn find_schedule(
    schedules: &[(Point, Point, usize)],
    task_id: usize,
) -> Option<&(Point, Point, usize)> {
    schedules.iter().find(|(_, _, id)| *id == task_id)
}

fn deadline_score(planner: &Planner, schedules: &[(Point, Point, usize)]) -> f64 {
    let mut score = 0.0;
    for task in &planner.tasks {
        let Some((_start, sched_end, _id)) = find_schedule(schedules, task.id) else {
            continue;
        };
        let slack = Point::delta(task.end, *sched_end);
        if slack >= 0 {
            score += (slack as f64 * W_EARLY).min(50.0);
        } else {
            let weight = 1.0 - task.abandonability;
            score += slack as f64 * W_LATE * weight;
        }
    }
    score
}

fn start_score(planner: &Planner, schedules: &[(Point, Point, usize)]) -> f64 {
    let mut score = 0.0;
    for task in &planner.tasks {
        let Some((sched_start, _sched_end, _id)) = find_schedule(schedules, task.id) else {
            continue;
        };
        if let Some(task_start) = task.start
            && *sched_start < task_start
        {
            score += Point::delta(*sched_start, task_start) as f64 * W_START;
        }
    }
    score
}

fn depend_score(
    planner: &Planner,
    schedules: &[(Point, Point, usize)],
    temperature: f64,
    t0: f64,
) -> f64 {
    let weight = W_DEPEND_BASE * (1.0 - temperature / t0);
    let mut penalty_slots = 0i64;
    for task in &planner.tasks {
        let Some((sched_start, _, _)) = find_schedule(schedules, task.id) else {
            continue;
        };
        for dep_id in &task.depends {
            if let Some((_, dep_end, _)) = find_schedule(schedules, *dep_id)
                && *dep_end > *sched_start
            {
                penalty_slots += dep_end.0 - sched_start.0;
            }
        }
    }
    -(penalty_slots as f64) * weight
}

fn buffer_score(planner: &Planner, schedules: &[(Point, Point, usize)]) -> f64 {
    let mut score = 0.0;
    for task in &planner.tasks {
        let Some((_start, sched_end, _id)) = find_schedule(schedules, task.id) else {
            continue;
        };
        let remaining = Point::delta(task.end, *sched_end);
        if remaining > 0 {
            score += task.cost_estimate.sigma as f64 * remaining as f64 * W_BUFFER;
        }
    }
    score
}

fn duration_score(planner: &Planner, schedules: &[(Point, Point, usize)]) -> f64 {
    let mut score = 0.0;
    for task in &planner.tasks {
        let Some((sched_start, sched_end, _id)) = find_schedule(schedules, task.id) else {
            continue;
        };
        let actual = Point::delta(*sched_end, *sched_start);
        let deficit = task.cost_estimate.avg as i64 - actual;
        if deficit > 0 {
            score += -(deficit * deficit) as f64 * W_SHORT;
        } else if deficit < 0 {
            score += deficit as f64 * W_OVER;
        }
    }
    score
}

fn sleep_score(planner: &Planner, schedules: &[(Point, Point, usize)]) -> f64 {
    let slots_per_day: i64 = (24 * 60) / planner.per as i64;
    let (day_start_epoch, sleep_start_rel, sleep_end_rel) = (
        planner.sleep.day_start,
        planner.sleep.start,
        planner.sleep.end,
    );
    let sleep_len = sleep_end_rel - sleep_start_rel;

    let (plan_start, plan_end) = plan_range(schedules);
    if plan_start >= plan_end {
        return 0.0;
    }

    let first_day = day_start_epoch
        + (plan_start.0 - day_start_epoch).div_euclid(slots_per_day) * slots_per_day;
    let mut day_start_point = Point(first_day - slots_per_day);

    let mut score = 0.0;

    while day_start_point.0 + sleep_start_rel <= plan_end.0 {
        let sleep_window_start = Point(day_start_point.0 + sleep_start_rel);
        let sleep_window_end = Point(day_start_point.0 + sleep_end_rel);

        let mut occupied = 0i64;
        for (s_start, s_end, _) in schedules {
            let overlap_start = s_start.0.max(sleep_window_start.0);
            let overlap_end = s_end.0.min(sleep_window_end.0);
            if overlap_start < overlap_end {
                occupied += overlap_end - overlap_start;
            }
        }

        if occupied > 0 {
            let sleep_got = sleep_len - occupied;
            score += -(occupied as f64) * W_SLEEP_NORMAL;
            if sleep_got < MIN_SLEEP {
                let deficit = MIN_SLEEP - sleep_got;
                score += -(deficit * deficit) as f64 * W_SLEEP_SEVERE;
            }
        }

        day_start_point = Point(day_start_point.0 + slots_per_day);
    }

    score
}

fn parallel_violation_score(planner: &Planner, schedules: &[(Point, Point, usize)]) -> f64 {
    let mut penalty_slots = 0i64;
    let n = schedules.len();
    for i in 0..n {
        let (a_start, a_end, a_id) = schedules[i];
        for (b_start, b_end, b_id) in schedules.iter().skip(i + 1).copied() {
            if a_end <= b_start || b_end <= a_start {
                continue;
            }
            let task_a = &planner.tasks[a_id];
            let task_b = &planner.tasks[b_id];
            if !((task_a.allows_parallel && task_b.parallelizable)
                || (task_b.allows_parallel && task_a.parallelizable))
            {
                let overlap = a_end.0.min(b_end.0) - a_start.0.max(b_start.0);
                penalty_slots += overlap;
            }
        }
    }
    -(penalty_slots as f64) * W_PARALLEL_VIOL
}

fn inclusion_bonus(_planner: &Planner, schedules: &[(Point, Point, usize)]) -> f64 {
    schedules.len() as f64 * W_INCLUSION
}

fn plan_range(schedules: &[(Point, Point, usize)]) -> (Point, Point) {
    if schedules.is_empty() {
        return (Point(0), Point(0));
    }
    let mut min_p = schedules[0].0;
    let mut max_p = schedules[0].1;
    for (s, e, _) in schedules {
        if s.0 < min_p.0 {
            min_p = *s;
        }
        if e.0 > max_p.0 {
            max_p = *e;
        }
    }
    (min_p, max_p)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_planner() -> Planner {
        Planner::new(Point(0), SleepConfig::disabled())
    }

    fn add_simple_task(p: &mut Planner, avg: u64, sigma: u64, end: i64) -> usize {
        p.add(Task {
            id: 0,
            start: Some(Point(0)),
            end: Point(end),
            cost_estimate: NormalDist::new(avg, sigma),
            depends: vec![],
            parallelizable: false,
            allows_parallel: false,
            abandonability: 0.5,
        })
        .unwrap()
    }

    fn plan_with(schedules: Vec<(Point, Point, usize)>) -> Plan {
        Plan { schedules }
    }

    #[test]
    fn evaluate_empty_schedule() {
        let p = make_planner();
        let plan = plan_with(vec![]);
        let score = evaluate(&p, &plan, 1.0, 1.0);
        assert_eq!(score, 0.0);
    }

    #[test]
    fn evaluate_deadline_violation() {
        let mut p = make_planner();
        let id = add_simple_task(&mut p, 3, 0, 5);
        let ok = plan_with(vec![(Point(0), Point(3), id)]);
        let late = plan_with(vec![(Point(0), Point(6), id)]);

        let score_ok = evaluate(&p, &ok, 0.0, 1.0);
        let score_late = evaluate(&p, &late, 0.0, 1.0);
        assert!(score_ok > score_late, "ok={score_ok} late={score_late}");
    }

    #[test]
    fn evaluate_start_violation() {
        let mut p = make_planner();
        let id = p
            .add(Task {
                id: 0,
                start: Some(Point(10)),
                end: Point(20),
                cost_estimate: NormalDist::new(3, 0),
                depends: vec![],
                parallelizable: false,
                allows_parallel: false,
                abandonability: 0.5,
            })
            .unwrap();

        let ok = plan_with(vec![(Point(10), Point(13), id)]);
        let early = plan_with(vec![(Point(5), Point(8), id)]);

        let score_ok = evaluate(&p, &ok, 0.0, 1.0);
        let score_early = evaluate(&p, &early, 0.0, 1.0);
        assert!(score_ok > score_early);
    }

    #[test]
    fn evaluate_depend_violation() {
        let mut p = make_planner();
        let a = add_simple_task(&mut p, 2, 0, 10);
        let b_id = p
            .add(Task {
                id: 0,
                start: Some(Point(0)),
                end: Point(10),
                cost_estimate: NormalDist::new(2, 0),
                depends: vec![a],
                parallelizable: false,
                allows_parallel: false,
                abandonability: 0.5,
            })
            .unwrap();

        let ok = plan_with(vec![(Point(0), Point(2), a), (Point(2), Point(4), b_id)]);
        let violated = plan_with(vec![(Point(0), Point(2), b_id), (Point(2), Point(4), a)]);

        let score_ok = evaluate(&p, &ok, 0.0, 1.0);
        let score_bad = evaluate(&p, &violated, 0.0, 1.0);
        assert!(score_ok > score_bad, "ok={score_ok} bad={score_bad}");
    }

    #[test]
    fn buffer_prefers_high_sigma_earlier() {
        let mut p = make_planner();
        let a = add_simple_task(&mut p, 1, 0, 5);
        let b = add_simple_task(&mut p, 1, 2, 5);

        let ab = plan_with(vec![(Point(0), Point(1), a), (Point(1), Point(2), b)]);
        let ba = plan_with(vec![(Point(0), Point(1), b), (Point(1), Point(2), a)]);

        let score_ab = evaluate(&p, &ab, 0.0, 1.0);
        let score_ba = evaluate(&p, &ba, 0.0, 1.0);
        assert!(
            score_ba > score_ab,
            "B→A should be better (B gets more buffer): ab={score_ab} ba={score_ba}"
        );
    }

    #[test]
    fn duration_too_short_penalized() {
        let mut p = make_planner();
        let id = add_simple_task(&mut p, 5, 0, 10);

        let full = plan_with(vec![(Point(0), Point(5), id)]);
        let short = plan_with(vec![(Point(0), Point(2), id)]);

        let score_full = evaluate(&p, &full, 0.0, 1.0);
        let score_short = evaluate(&p, &short, 0.0, 1.0);
        assert!(
            score_full > score_short,
            "full={score_full} short={score_short}"
        );
    }

    #[test]
    fn sleep_three_hour_threshold() {
        let mut p = make_planner();

        p.sleep = SleepConfig {
            day_start: 0,
            start: 0,
            end: 96,
        };

        let task_id = add_simple_task(&mut p, 24, 0, 200);
        let plan_4h_lost = plan_with(vec![(Point(0), Point(48), task_id)]);
        let plan_6h_lost = plan_with(vec![(Point(0), Point(72), task_id)]);

        let score_4h = evaluate(&p, &plan_4h_lost, 0.0, 1.0);
        let score_6h = evaluate(&p, &plan_6h_lost, 0.0, 1.0);

        assert!(
            score_4h > score_6h,
            "4h sleep lost should be less penalized than 6h: 4h={score_4h} 6h={score_6h}"
        );
    }

    #[test]
    fn parallel_task_can_overlap() {
        let mut p = make_planner();
        let host = p
            .add(Task {
                id: 0,
                start: Some(Point(0)),
                end: Point(10),
                cost_estimate: NormalDist::new(5, 0),
                depends: vec![],
                parallelizable: false,
                allows_parallel: true,
                abandonability: 0.5,
            })
            .unwrap();
        let guest = p
            .add(Task {
                id: 0,
                start: Some(Point(0)),
                end: Point(10),
                cost_estimate: NormalDist::new(2, 0),
                depends: vec![],
                parallelizable: true,
                allows_parallel: false,
                abandonability: 0.5,
            })
            .unwrap();

        let overlapping = plan_with(vec![
            (Point(0), Point(5), host),
            (Point(0), Point(2), guest),
        ]);
        let score = evaluate(&p, &overlapping, 0.0, 1.0);
        assert!(score.is_finite());
    }

    #[test]
    fn parallel_violation_penalty_applied() {
        let mut p = make_planner();
        let a = add_simple_task(&mut p, 3, 0, 100);
        let b = add_simple_task(&mut p, 3, 0, 100);

        let overlapping = plan_with(vec![(Point(0), Point(3), a), (Point(0), Point(3), b)]);
        let separate = plan_with(vec![(Point(0), Point(3), a), (Point(3), Point(6), b)]);

        let score_overlap = evaluate(&p, &overlapping, 0.0, 1.0);
        let score_separate = evaluate(&p, &separate, 0.0, 1.0);
        assert!(
            score_separate > score_overlap,
            "separate should score higher due to no parallel penalty: sep={score_separate} overlap={score_overlap}"
        );
    }

    #[test]
    fn parallel_tasks_no_penalty() {
        let mut p = make_planner();
        let host = p
            .add(Task {
                id: 0,
                start: Some(Point(0)),
                end: Point(100),
                cost_estimate: NormalDist::new(3, 0),
                depends: vec![],
                parallelizable: false,
                allows_parallel: true,
                abandonability: 0.5,
            })
            .unwrap();
        let guest = p
            .add(Task {
                id: 0,
                start: Some(Point(0)),
                end: Point(100),
                cost_estimate: NormalDist::new(3, 0),
                depends: vec![],
                parallelizable: true,
                allows_parallel: false,
                abandonability: 0.5,
            })
            .unwrap();

        let overlapping = plan_with(vec![
            (Point(0), Point(3), host),
            (Point(0), Point(3), guest),
        ]);
        let no_overlap = plan_with(vec![
            (Point(0), Point(3), host),
            (Point(3), Point(6), guest),
        ]);

        let score_overlap = evaluate(&p, &overlapping, 0.0, 1.0);
        let score_no = evaluate(&p, &no_overlap, 0.0, 1.0);
        assert!(
            (score_overlap - score_no).abs() < 1e-6,
            "parallel tasks should have no violation penalty. overlap={score_overlap} no={score_no}"
        );
    }

    #[test]
    fn sleep_recommended_nighttime_penalized() {
        let mut p = Planner::new(Point(0), SleepConfig::recommended());

        let id = p
            .add(Task {
                id: 0,
                start: Some(Point(0)),
                end: Point(500),
                cost_estimate: NormalDist::new(12, 0),
                depends: vec![],
                parallelizable: false,
                allows_parallel: false,
                abandonability: 0.5,
            })
            .unwrap();

        let day_plan = plan_with(vec![(Point(96), Point(108), id)]);
        let night_plan = plan_with(vec![(Point(276), Point(288), id)]);

        let day_score = evaluate(&p, &day_plan, 0.0, 1.0);
        let night_score = evaluate(&p, &night_plan, 0.0, 1.0);

        assert!(
            day_score > night_score,
            "Daytime should score higher than nighttime: day={day_score} night={night_score}"
        );
    }

    #[test]
    fn sleep_recommended_second_day() {
        let mut p = Planner::new(Point(0), SleepConfig::recommended());

        let id = p
            .add(Task {
                id: 0,
                start: Some(Point(0)),
                end: Point(1000),
                cost_estimate: NormalDist::new(20, 0),
                depends: vec![],
                parallelizable: false,
                allows_parallel: false,
                abandonability: 0.5,
            })
            .unwrap();

        let day2_plan = plan_with(vec![(Point(400), Point(420), id)]);
        let night2_plan = plan_with(vec![(Point(552), Point(572), id)]);

        let day2_score = evaluate(&p, &day2_plan, 0.0, 1.0);
        let night2_score = evaluate(&p, &night2_plan, 0.0, 1.0);

        assert!(
            day2_score > night2_score,
            "Second day afternoon should score higher than second night: day2={day2_score} night2={night2_score}"
        );
    }
}
