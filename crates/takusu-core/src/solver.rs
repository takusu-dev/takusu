//! # ソルバー: 並列再起動 SA
//!
//! k 本の独立 SA チェーンを rayon で並列実行し、評価関数最大の解を選択する。
//! タスクはすべてスケジュールされる（諦めない）。
//! abandonability が高いタスクは deadline 超過ペナルティが軽減される。
//!
//! ## 部分問題分割の検討
//!
//! ### DAG 連結成分分解
//! 依存グラフを連結成分に分割し、成分ごとに独立 SA。n=100 を 5×20 に分割すれば
//! 評価関数 25倍高速。品質低下は中程度。時間窓競合のマージが課題。
//!
//! ### 結論
//! 現時点では全体 SA + 並列再起動が最も堅実。

use std::cmp::Ordering;

use rand::SeedableRng;
use rand::rngs::StdRng;
use rayon::prelude::*;

use super::*;
use anneal::{sa_lns, sa_lns_partial};
use evaluate::evaluate;

const MAX_CHAINS: usize = 4;

pub fn solve(planner: &Planner) -> Plan {
    let num_chains = rayon::current_num_threads().clamp(1, MAX_CHAINS);

    (0..num_chains)
        .into_par_iter()
        .map(|seed| sa_lns(planner, &mut StdRng::seed_from_u64(seed as u64)))
        .max_by(|a, b| {
            evaluate(planner, a, 0.0, 1.0)
                .partial_cmp(&evaluate(planner, b, 0.0, 1.0))
                .unwrap_or(Ordering::Equal)
        })
        .unwrap_or_else(|| Plan { schedules: vec![] })
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
                })
                .unwrap();
        }
        let plan = solve(&planner);
        for (_s, e, _) in &plan.schedules {
            assert!(e.0 <= 10000);
        }
    }
}

pub fn solve_partial(planner: &Planner, pinned: &[(Point, Point, usize)]) -> Plan {
    if pinned.is_empty() {
        return solve(planner);
    }

    let num_chains = rayon::current_num_threads().clamp(1, MAX_CHAINS);

    (0..num_chains)
        .into_par_iter()
        .map(|seed| sa_lns_partial(planner, pinned, &mut StdRng::seed_from_u64(seed as u64)))
        .max_by(|a, b| {
            evaluate(planner, a, 0.0, 1.0)
                .partial_cmp(&evaluate(planner, b, 0.0, 1.0))
                .unwrap_or(Ordering::Equal)
        })
        .unwrap_or_else(|| Plan { schedules: vec![] })
}
