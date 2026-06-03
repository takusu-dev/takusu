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
