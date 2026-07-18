//! # ソルバー: 単一 SA チェーン
//!
//! 1 本の SA チェーンを実行して最適解を返す。
//! タスクはすべてスケジュールされる（諦めない）。
//! abandonability が高いタスクは deadline 超過ペナルティが軽減される。

use rand::SeedableRng;
use rand::rngs::StdRng;

use super::*;
use anneal::{sa_lns, sa_lns_partial};

pub fn solve(planner: &Planner) -> Plan {
    let mut rng = StdRng::seed_from_u64(0);
    sa_lns(planner, &mut rng)
}

/// 範囲外のタスク ID は無視し、有効な pinned が空の場合は solve (sa_lns) に委譲する。
/// sa_lns と sa_lns_partial は pinned_ids のフィルタリング以外は同一アルゴリズムのため、
/// 空 pinned の場合はオーバーヘッドを避けてフル SA にフォールバック。
pub fn solve_partial(planner: &Planner, pinned: &[(Point, Point, usize)]) -> Plan {
    if pinned.is_empty() {
        return solve(planner);
    }

    let valid_pinned: Vec<_> = pinned
        .iter()
        .filter(|(_, _, id)| *id < planner.tasks.len())
        .copied()
        .collect();

    if valid_pinned.is_empty() {
        return solve(planner);
    }

    let mut rng = StdRng::seed_from_u64(0);
    sa_lns_partial(planner, &valid_pinned, &mut rng)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{NormalDist, Planner, SleepConfig, Task};

    fn make_planner(task_count: usize) -> Planner {
        let mut planner = Planner::new(Point(0), SleepConfig::disabled());
        for i in 0..task_count {
            planner
                .add(Task {
                    id: 0,
                    start: None,
                    end: Point(288 * 7),
                    cost_estimate: NormalDist::new(12, 2),
                    depends: if i > 0 { vec![i - 1] } else { vec![] },
                    parallelizable: false,
                    allows_parallel: false,
                    abandonability: 0.5,
                    fixed: false,
                    habit_group: None,
                })
                .unwrap();
        }
        planner
    }

    #[test]
    fn solve_produces_valid_plan() {
        let planner = make_planner(5);
        let plan = solve(&planner);
        assert!(!plan.schedules.is_empty());
        for (s, e, _) in &plan.schedules {
            assert!(e.0 >= s.0);
        }
    }

    #[test]
    fn solve_partial_preserves_pinned_order() {
        let planner = make_planner(5);
        let plan = solve(&planner);
        let pinned: Vec<_> = plan.schedules.get(0..2).unwrap_or(&[]).to_vec();
        let partial = solve_partial(&planner, &pinned);
        assert!(!partial.schedules.is_empty());
        if partial.schedules.len() >= 2 {
            assert_eq!(partial.schedules[0], pinned[0]);
            assert_eq!(partial.schedules[1], pinned[1]);
        }
    }

    #[test]
    fn solve_partial_empty_pinned_equals_solve() {
        let planner = make_planner(3);
        let plan_full = solve(&planner);
        let plan_partial = solve_partial(&planner, &[]);
        assert_eq!(plan_full.schedules.len(), plan_partial.schedules.len());
    }

    #[test]
    fn solve_partial_ignores_out_of_range_pinned_ids() {
        let planner = make_planner(3);
        let plan = solve(&planner);
        let mut pinned: Vec<_> = plan.schedules.get(0..1).unwrap_or(&[]).to_vec();
        pinned.push((Point(0), Point(1), 99));
        let partial = solve_partial(&planner, &pinned);
        assert!(!partial.schedules.iter().any(|(_, _, id)| *id == 99));
    }

    #[test]
    fn solve_empty_planner() {
        let planner = Planner::new(Point(0), SleepConfig::disabled());
        let plan = solve(&planner);
        assert!(plan.schedules.is_empty());
    }

    #[test]
    fn solve_no_deadline_violation_for_easy_tasks() {
        let mut planner = Planner::new(Point(0), SleepConfig::disabled());
        for _i in 0..5 {
            planner
                .add(Task {
                    id: 0,
                    start: None,
                    end: Point(10000),
                    cost_estimate: NormalDist::new(6, 0),
                    depends: vec![],
                    parallelizable: false,
                    allows_parallel: false,
                    abandonability: 0.0,
                    fixed: false,
                    habit_group: None,
                })
                .unwrap();
        }
        let plan = solve(&planner);
        for (_s, e, _) in &plan.schedules {
            assert!(e.0 <= 10000);
        }
    }
}
