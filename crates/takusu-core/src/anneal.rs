//! # SA + LNS + Tabu Search
//!
//! ```text
//! 1. greedy_initial(active_tasks) → 初期解
//! 2. T = T₀ → ... → T_min:
//!    各温度でN反復:
//!      neighbor = generate (7種, 確率重み付き)
//!      tabuチェック (aspirationあり)
//!      ΔEで受理判定 (Metropolis)
//!      tabu更新
//! 3. best を返却
//! ```
//!
//! | prob | neighbor      |
//! |------|---------------|
//! | 25%  | shift         |
//! | 25%  | swap          |
//! | 20%  | duration ±1   |
//! | 15%  | reorder       |
//! | 15%  | lns (destroy+rebuild) |
//!
//! ## Design rationale
//!
//! ### Tabu list key = (task_id, start, duration)
//! 同一タスクの同一配置への再訪を防ぐ。完全なハッシュ (全taskの配置) だと容量爆発するため
//! 最後に動かした一つのタスクのみを記録。容量 = task_count*2。
//! aspiration: tabu でも best より良ければ受理 (改善解は tabu を無視)。
//!
//! ### LNS window size
//! pivot タスクの duration*2、最低4スロット。総タスク時間の1/3以上にはしない。
//! これにより小さな window の局所改善と大きな再配置のバランスを取る。
//!
//! ### greedy_rebuild の freeness 順
//! destroy で除去したタスクを freeness 昇順に再配置。freeness の低い(切迫した)タスクから
//! 空きスロットに詰めることで、高 freeness タスクが柔軟に後回しにされる。
//!
//! ### partial モードで pinned_ids を毎回渡す理由
//! generate_neighbor_partial は pinned タスクの位置を一切変更しないため、
//! unpinned の task_id 一覧を毎回抽出する。これは計算量O(n)だが、n<100で支配的でない。
//! 代わりに pinned 固定のインデックス集合をキャッシュすることもできるが、簡潔さを優先。

use std::collections::VecDeque;

use rand::{Rng, RngExt};
use rustc_hash::FxHashSet;

use super::*;
use evaluate::evaluate;

struct TabuList {
    entries: VecDeque<(usize, i64, i64)>,
    capacity: usize,
}

impl TabuList {
    fn new(capacity: usize) -> Self {
        Self {
            entries: VecDeque::new(),
            capacity,
        }
    }

    fn push(&mut self, task_id: usize, start: Point, duration: i64) {
        if self.entries.len() >= self.capacity {
            self.entries.pop_front();
        }
        self.entries.push_back((task_id, start.0, duration));
    }

    fn contains(&self, task_id: usize, start: Point, duration: i64) -> bool {
        self.entries
            .iter()
            .any(|(id, s, d)| *id == task_id && *s == start.0 && *d == duration)
    }
}

fn rand_range(rng: &mut impl Rng, low: i64, high: i64) -> i64 {
    if high <= low {
        return low;
    }
    rng.random_range(low..high)
}

/// トポロジカル順序を計算。依存関係のないタスクは自由順序。
/// 注意: この順序は freeness ソートの入力に使われるだけで、配置順自体ではない。
/// build_initial 内でさらに freeness 昇順に並び替えられる。
fn topological_order(planner: &Planner, active: &FxHashSet<usize>) -> Vec<usize> {
    let n = planner.tasks.len();
    let mut in_degree = vec![0usize; n];
    let mut adj = vec![Vec::new(); n];

    for task in &planner.tasks {
        if !active.contains(&task.id) {
            continue;
        }
        for dep in &task.depends {
            if active.contains(dep) {
                adj[*dep].push(task.id);
                in_degree[task.id] += 1;
            }
        }
    }

    let mut queue: Vec<usize> = (0..n)
        .filter(|i| active.contains(i) && in_degree[*i] == 0)
        .collect();
    let mut result = Vec::with_capacity(n);

    while let Some(u) = queue.pop() {
        result.push(u);
        for &v in &adj[u] {
            in_degree[v] -= 1;
            if in_degree[v] == 0 {
                queue.push(v);
            }
        }
    }

    result
}

fn compute_earliest(planner: &Planner, schedules: &[(Point, Point, usize)], task: &Task) -> Point {
    let mut earliest = planner.now;
    if let Some(start) = task.start {
        earliest = earliest.max(start);
    }
    for dep_id in &task.depends {
        if let Some((_, dep_end, _)) = schedules.iter().find(|(_, _, id)| id == dep_id) {
            earliest = earliest.max(*dep_end);
        }
    }
    earliest
}

/// `[start, end)` と重なる睡眠窓があれば、その窓の終端スロットを返す。
fn sleep_window_conflict(planner: &Planner, start: i64, end: i64) -> Option<i64> {
    let sleep = &planner.sleep;
    if !sleep.enabled {
        return None;
    }
    let slots_per_day: i64 = (24 * 60) / planner.per as i64;
    let mut day = sleep.day_start
        + (start - sleep.day_start).div_euclid(slots_per_day) * slots_per_day
        - slots_per_day;
    while day + sleep.start < end {
        let w_start = day + sleep.start;
        let w_end = day + sleep.end;
        if w_start < end && w_end > start {
            return Some(w_end);
        }
        day += slots_per_day;
    }
    None
}

fn try_place(
    planner: &Planner,
    schedules: &[(Point, Point, usize)],
    task: &Task,
    earliest: Point,
    dur: i64,
) -> Option<(Point, Point)> {
    if dur <= 0 {
        return None;
    }
    let awake_len = if planner.sleep.enabled {
        (24 * 60) / planner.per as i64 - (planner.sleep.end - planner.sleep.start)
    } else {
        i64::MAX
    };
    let avoid_sleep = dur <= awake_len;
    let mut cursor = earliest;
    let mut guard = 0u32;

    loop {
        guard += 1;
        if guard > 10_000 {
            break;
        }
        let candidate_end = Point(cursor.0 + dur);

        if avoid_sleep
            && let Some(w_end) = sleep_window_conflict(planner, cursor.0, candidate_end.0)
        {
            cursor = Point(w_end);
            continue;
        }

        let overlapping: Vec<_> = schedules
            .iter()
            .filter(|(s, e, _)| s.0 < candidate_end.0 && e.0 > cursor.0)
            .collect();

        if overlapping.is_empty() {
            return Some((cursor, candidate_end));
        }

        let can_parallel = task.parallelizable;
        let can_host = task.allows_parallel;

        if can_parallel {
            let all_hosting = overlapping
                .iter()
                .all(|(_, _, oid)| planner.tasks[*oid].allows_parallel);
            if all_hosting {
                return Some((cursor, candidate_end));
            }
        }

        if can_host {
            let all_guesting = overlapping
                .iter()
                .all(|(_, _, oid)| planner.tasks[*oid].parallelizable);
            if all_guesting {
                return Some((cursor, candidate_end));
            }
        }

        let next_start = overlapping
            .iter()
            .map(|(_, e, _)| e.0)
            .max()
            .unwrap_or(cursor.0);
        if next_start <= cursor.0 {
            break;
        }
        cursor = Point(next_start);
    }

    None
}

/// 貪欲法で初期解を構築。
///
/// 方針: 切迫したタスク (freeness 低い) から順に、依存を満たす最も早い位置に配置。
/// 挿入できない場合は末尾に fallback 配置。
///
/// これは単なるヒューリスティック: 必ずしも実行可能解とは限らない
/// (依存サイクルや容量超過で配置できないケースがある)。
/// SA がその後、評価関数の勾配に従って改善する。
fn build_initial(planner: &Planner) -> Plan {
    let all: FxHashSet<usize> = planner.tasks.iter().map(|t| t.id).collect();
    let order = topological_order(planner, &all);

    let mut by_freeness: Vec<usize> = order.into_iter().collect();
    by_freeness.sort_by(|a, b| {
        planner
            .freeness(*a)
            .partial_cmp(&planner.freeness(*b))
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let mut schedules: Vec<(Point, Point, usize)> = Vec::new();
    let mut last_end = planner.now;

    for task_id in by_freeness {
        let task = &planner.tasks[task_id];
        let dur = (task.cost_estimate.avg as i64).max(1);

        let earliest = compute_earliest(planner, &schedules, task);
        if let Some((start, end)) = try_place(planner, &schedules, task, earliest, dur) {
            schedules.push((start, end, task_id));
            last_end = last_end.max(end);
        } else {
            let fallback_start = last_end.max(planner.now);
            let fallback_end = Point(fallback_start.0 + dur);
            schedules.push((fallback_start, fallback_end, task_id));
            last_end = fallback_end;
        }
    }

    Plan { schedules }
}

/// 改善なしの温度レベルがこの回数続いたら current を best に戻す (intensification)。
const STAGNATION_LIMIT: u32 = 3;

/// 長距離 shift を選ぶ確率の分母 (1/5 = 20%)。
const LONG_SHIFT_ONE_IN: u32 = 5;

/// プラン全体の時間スパン。長距離 shift の移動幅に使う。
fn plan_span(plan: &Plan) -> i64 {
    let min_s = plan.schedules.iter().map(|(s, _, _)| s.0).min();
    let max_e = plan.schedules.iter().map(|(_, e, _)| e.0).max();
    match (min_s, max_e) {
        (Some(a), Some(b)) => (b - a).max(1),
        _ => 1,
    }
}

fn shift_range(current: &Plan, dur: i64, rng: &mut impl Rng) -> i64 {
    if rng.random_range(0..LONG_SHIFT_ONE_IN) == 0 {
        plan_span(current)
    } else {
        (dur / 2).max(1)
    }
}

pub fn sa_lns(planner: &Planner, rng: &mut impl Rng) -> Plan {
    let task_count = planner.tasks.len().max(1);

    let mut current = build_initial(planner);
    let mut best = current.clone();

    let total_avg: i64 = planner
        .tasks
        .iter()
        .map(|t| t.cost_estimate.avg as i64)
        .sum();
    let t0 = (total_avg as f64 * 0.1).max(1.0);
    let alpha = 0.93;
    let t_min = t0 * 1e-4;
    let iter_per_temp = task_count * 30;

    let mut tabu = TabuList::new(task_count * 2);
    let mut temperature = t0;

    let mut eval_current = evaluate(planner, &current, temperature, t0);
    let mut eval_best = eval_current;

    let mut stagnant_levels = 0u32;

    while temperature > t_min {
        let mut improved = false;
        for _ in 0..iter_per_temp {
            let neighbor = generate_neighbor(planner, &current, rng);
            let eval_neighbor = evaluate(planner, &neighbor, temperature, t0);

            if is_tabu(&tabu, &neighbor) && eval_neighbor <= eval_best {
                continue;
            }

            let delta = eval_neighbor - eval_current;

            if delta > 0.0 || rng.random::<f64>() < (delta / temperature).exp() {
                mark_tabu(&mut tabu, &neighbor);
                current = neighbor;
                eval_current = eval_neighbor;

                if eval_current > eval_best {
                    best = current.clone();
                    eval_best = eval_current;
                    improved = true;
                }
            }
        }

        if improved {
            stagnant_levels = 0;
        } else {
            stagnant_levels += 1;
            if stagnant_levels >= STAGNATION_LIMIT {
                current = best.clone();
                stagnant_levels = 0;
            }
        }

        temperature *= alpha;
        eval_current = evaluate(planner, &current, temperature, t0);
        eval_best = evaluate(planner, &best, temperature, t0);
    }

    repair_polish(planner, best, None)
}

pub fn sa_lns_partial(
    planner: &Planner,
    pinned: &[(Point, Point, usize)],
    rng: &mut impl Rng,
) -> Plan {
    if pinned.is_empty() {
        return sa_lns(planner, rng);
    }

    let pinned_ids: FxHashSet<usize> = pinned.iter().map(|(_, _, id)| *id).collect();

    let unpinned_count = planner
        .tasks
        .iter()
        .filter(|t| !pinned_ids.contains(&t.id))
        .count();
    let task_count = planner.tasks.len().max(1);

    let mut current = build_initial_partial(planner, pinned);
    let mut best = current.clone();

    let total_avg: i64 = planner
        .tasks
        .iter()
        .filter(|t| !pinned_ids.contains(&t.id))
        .map(|t| t.cost_estimate.avg as i64)
        .sum();
    let t0 = (total_avg as f64 * 0.1).max(1.0);
    let alpha = 0.93;
    let t_min = t0 * 1e-4;
    let iter_per_temp = unpinned_count.max(1) * 30;

    let mut tabu = TabuList::new(task_count * 2);
    let mut temperature = t0;

    let mut eval_current = evaluate(planner, &current, temperature, t0);
    let mut eval_best = eval_current;

    let mut stagnant_levels = 0u32;

    while temperature > t_min {
        let mut improved = false;
        for _ in 0..iter_per_temp {
            let neighbor = generate_neighbor_partial(planner, &current, rng, &pinned_ids);
            let eval_neighbor = evaluate(planner, &neighbor, temperature, t0);

            if is_tabu(&tabu, &neighbor) && eval_neighbor <= eval_best {
                continue;
            }

            let delta = eval_neighbor - eval_current;

            if delta > 0.0 || rng.random::<f64>() < (delta / temperature).exp() {
                mark_tabu(&mut tabu, &neighbor);
                current = neighbor;
                eval_current = eval_neighbor;

                if eval_current > eval_best {
                    best = current.clone();
                    eval_best = eval_current;
                    improved = true;
                }
            }
        }

        if improved {
            stagnant_levels = 0;
        } else {
            stagnant_levels += 1;
            if stagnant_levels >= STAGNATION_LIMIT {
                current = best.clone();
                stagnant_levels = 0;
            }
        }

        temperature *= alpha;
        eval_current = evaluate(planner, &current, temperature, t0);
        eval_best = evaluate(planner, &best, temperature, t0);
    }

    repair_polish(planner, best, Some(&pinned_ids))
}

/// SA 後の仕上げ: 依存違反中のタスクを取り除いて貪欲に再配置し、
/// T=0 の評価が改善する場合のみ採用する。
fn repair_polish(planner: &Planner, best: Plan, pinned_ids: Option<&FxHashSet<usize>>) -> Plan {
    let mut index: Vec<Option<(Point, Point)>> = vec![None; planner.tasks.len()];
    for (s, e, id) in &best.schedules {
        if *id < index.len() {
            index[*id] = Some((*s, *e));
        }
    }

    let mut violators: FxHashSet<usize> = FxHashSet::default();
    for task in &planner.tasks {
        if let Some(p) = pinned_ids
            && p.contains(&task.id)
        {
            continue;
        }
        let Some((start, _)) = index[task.id] else {
            continue;
        };
        for dep_id in &task.depends {
            if let Some(Some((_, dep_end))) = index.get(*dep_id)
                && *dep_end > start
            {
                violators.insert(task.id);
            }
        }
    }

    if violators.is_empty() {
        return best;
    }

    // 違反タスクを後ろへ動かすとその依存元も違反し得るため、推移的な依存元も再配置対象にする。
    loop {
        let mut grew = false;
        for task in &planner.tasks {
            if violators.contains(&task.id) {
                continue;
            }
            if let Some(p) = pinned_ids
                && p.contains(&task.id)
            {
                continue;
            }
            if task.depends.iter().any(|d| violators.contains(d)) {
                violators.insert(task.id);
                grew = true;
            }
        }
        if !grew {
            break;
        }
    }

    let mut remaining = Vec::new();
    let mut destroyed: Vec<usize> = Vec::new();
    for sched in &best.schedules {
        if violators.contains(&sched.2) {
            destroyed.push(sched.2);
        } else {
            remaining.push(*sched);
        }
    }

    let rebuilt = Plan {
        schedules: greedy_rebuild(planner, &remaining, &destroyed),
    };

    if evaluate(planner, &rebuilt, 0.0, 1.0) > evaluate(planner, &best, 0.0, 1.0) {
        rebuilt
    } else {
        best
    }
}

fn build_initial_partial(planner: &Planner, pinned: &[(Point, Point, usize)]) -> Plan {
    let pinned_ids: FxHashSet<usize> = pinned.iter().map(|(_, _, id)| *id).collect();

    let mut unpinned: Vec<usize> = planner
        .tasks
        .iter()
        .filter(|t| !pinned_ids.contains(&t.id))
        .map(|t| t.id)
        .collect();

    unpinned.sort_by(|a, b| {
        planner
            .freeness(*a)
            .partial_cmp(&planner.freeness(*b))
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let mut schedules: Vec<(Point, Point, usize)> = pinned.to_vec();

    for task_id in unpinned {
        let task = &planner.tasks[task_id];
        let dur = (task.cost_estimate.avg as i64).max(1);

        let earliest = compute_earliest(planner, &schedules, task);
        if let Some((start, end)) = try_place(planner, &schedules, task, earliest, dur) {
            schedules.push((start, end, task_id));
        } else {
            let last_end = schedules
                .iter()
                .map(|(_, e, _)| e.0)
                .max()
                .unwrap_or(planner.now.0);
            let fallback_start = Point(last_end).max(planner.now);
            let fallback_end = Point(fallback_start.0 + dur);
            schedules.push((fallback_start, fallback_end, task_id));
        }
    }

    Plan { schedules }
}

fn generate_neighbor_partial(
    planner: &Planner,
    current: &Plan,
    rng: &mut impl Rng,
    pinned_ids: &FxHashSet<usize>,
) -> Plan {
    let unpinned: Vec<usize> = current
        .schedules
        .iter()
        .filter(|(_, _, id)| !pinned_ids.contains(id))
        .map(|(_, _, id)| *id)
        .collect();

    if unpinned.is_empty() {
        return current.clone();
    }

    let unpinned_positions: Vec<usize> = current
        .schedules
        .iter()
        .enumerate()
        .filter(|(_, (_, _, id))| !pinned_ids.contains(id))
        .map(|(i, _)| i)
        .collect();

    let r = rng.random_range(0..100u32) as i32;

    let result = match r {
        0..=19 => {
            let idx = rng.random_range(0..unpinned_positions.len());
            let pos = unpinned_positions[idx];
            neighbor_shift_at(planner, current, pos, rng)
        }
        20..=39 => {
            if unpinned.len() < 2 {
                return current.clone();
            }
            let a_idx = rng.random_range(0..unpinned_positions.len());
            let a_pos = unpinned_positions[a_idx];
            let mut b_idx = rng.random_range(0..unpinned_positions.len());
            if b_idx == a_idx {
                b_idx = (a_idx + 1) % unpinned_positions.len();
            }
            let b_pos = unpinned_positions[b_idx];
            neighbor_swap_at(current, a_pos, b_pos)
        }
        40..=54 => {
            let idx = rng.random_range(0..unpinned_positions.len());
            let pos = unpinned_positions[idx];
            neighbor_duration_at(current, pos, rng)
        }
        55..=69 => {
            if unpinned.len() < 2 {
                return current.clone();
            }
            neighbor_reorder_partial(current, &unpinned_positions, rng)
        }
        70..=84 => neighbor_repair_depend(planner, current, rng, Some(pinned_ids)),
        _ => neighbor_lns_partial(planner, current, rng, pinned_ids),
    };

    result.unwrap_or_else(|| current.clone())
}

fn neighbor_shift_at(
    planner: &Planner,
    current: &Plan,
    idx: usize,
    rng: &mut impl Rng,
) -> Option<Plan> {
    let (start, end, task_id) = current.schedules[idx];
    let dur = end.0 - start.0;
    let range = shift_range(current, dur, rng);
    let k = rand_range(rng, -range, range + 1);
    let new_start_0 = (start.0 + k).max(planner.now.0);
    let mut new_scheds = current.schedules.to_vec();
    new_scheds[idx] = (Point(new_start_0), Point(new_start_0 + dur), task_id);
    Some(Plan {
        schedules: new_scheds,
    })
}

fn neighbor_swap_at(current: &Plan, a: usize, b: usize) -> Option<Plan> {
    if a == b {
        return None;
    }
    let (a_s, a_e, a_id) = current.schedules[a];
    let (b_s, b_e, b_id) = current.schedules[b];
    let a_dur = a_e.0 - a_s.0;
    let b_dur = b_e.0 - b_s.0;
    let mut new_scheds = current.schedules.to_vec();
    new_scheds[a] = (b_s, Point(b_s.0 + a_dur), a_id);
    new_scheds[b] = (a_s, Point(a_s.0 + b_dur), b_id);
    Some(Plan {
        schedules: new_scheds,
    })
}

fn neighbor_duration_at(current: &Plan, idx: usize, rng: &mut impl Rng) -> Option<Plan> {
    let (start, end, task_id) = current.schedules[idx];
    let dur = end.0 - start.0;
    if dur <= 1 {
        return None;
    }
    let delta: i64 = if rng.random::<bool>() { 1 } else { -1 };
    let new_dur = dur + delta;
    if new_dur < 1 {
        return None;
    }
    let mut new_scheds = current.schedules.to_vec();
    new_scheds[idx] = (start, Point(start.0 + new_dur), task_id);
    Some(Plan {
        schedules: new_scheds,
    })
}

fn neighbor_reorder_partial(
    current: &Plan,
    unpinned_positions: &[usize],
    rng: &mut impl Rng,
) -> Option<Plan> {
    let schedules = &current.schedules;
    let a_idx = rng.random_range(0..unpinned_positions.len());
    let a = unpinned_positions[a_idx];
    let mut b_idx = rng.random_range(0..unpinned_positions.len());
    if b_idx == a_idx {
        b_idx = (a_idx + 1) % unpinned_positions.len();
    }
    let b = unpinned_positions[b_idx];

    let (a_s, _a_e, _a_id) = schedules[a];
    let (b_s, _b_e, _b_id) = schedules[b];

    let (first, second) = if a_s.0 <= b_s.0 { (a, b) } else { (b, a) };
    let f_s = schedules[first].0;
    let f_e = schedules[first].1;
    let f_dur = f_e.0 - f_s.0;
    let s_s = schedules[second].0;
    let s_e = schedules[second].1;
    let s_dur = s_e.0 - s_s.0;

    let mut new_scheds = schedules.to_vec();
    new_scheds[first] = (s_s, Point(s_s.0 + f_dur), schedules[first].2);
    new_scheds[second] = (f_s, Point(f_s.0 + s_dur), schedules[second].2);

    Some(Plan {
        schedules: new_scheds,
    })
}

fn neighbor_lns_partial(
    planner: &Planner,
    current: &Plan,
    rng: &mut impl Rng,
    pinned_ids: &FxHashSet<usize>,
) -> Option<Plan> {
    let schedules = &current.schedules;
    if schedules.is_empty() {
        return None;
    }

    let unpinned: Vec<(usize, Point, Point)> = schedules
        .iter()
        .filter(|(_, _, id)| !pinned_ids.contains(id))
        .map(|(s, e, id)| (*id, *s, *e))
        .collect();

    if unpinned.is_empty() {
        return None;
    }

    let pivot_idx = rng.random_range(0..schedules.len());
    let (pivot_start, pivot_end, _) = schedules[pivot_idx];

    let window_size = ((pivot_end.0 - pivot_start.0) * 2)
        .max(4)
        .min(schedules.iter().map(|(s, e, _)| e.0 - s.0).sum::<i64>() / 3 + 1);

    let window_start = pivot_start.0 - rand_range(rng, 0, window_size / 2 + 1);
    let window_end = window_start + window_size;

    let mut destroyed_ids = Vec::new();
    let mut remaining = Vec::new();
    for sched in schedules {
        if !pinned_ids.contains(&sched.2) && sched.0.0 >= window_start && sched.0.0 < window_end {
            destroyed_ids.push(sched.2);
        } else {
            remaining.push(*sched);
        }
    }

    let rebuilt = greedy_rebuild(planner, &remaining, &destroyed_ids);

    Some(Plan { schedules: rebuilt })
}

fn is_tabu(tabu: &TabuList, plan: &Plan) -> bool {
    plan.schedules
        .iter()
        .any(|(s, e, id)| tabu.contains(*id, *s, e.0 - s.0))
}

fn mark_tabu(tabu: &mut TabuList, plan: &Plan) {
    for (s, e, id) in &plan.schedules {
        tabu.push(*id, *s, e.0 - s.0);
    }
}

fn generate_neighbor(planner: &Planner, current: &Plan, rng: &mut impl Rng) -> Plan {
    let r = rng.random_range(0..100u32) as i32;

    let result = match r {
        0..=19 => neighbor_shift(planner, current, rng),
        20..=39 => neighbor_swap(planner, current, rng),
        40..=54 => neighbor_duration(planner, current, rng),
        55..=69 => neighbor_reorder(planner, current, rng),
        70..=84 => neighbor_repair_depend(planner, current, rng, None),
        _ => neighbor_lns(planner, current, rng),
    };

    result.unwrap_or_else(|| current.clone())
}

fn neighbor_shift(planner: &Planner, current: &Plan, rng: &mut impl Rng) -> Option<Plan> {
    let schedules = &current.schedules;
    if schedules.is_empty() {
        return None;
    }
    let idx = rng.random_range(0..schedules.len());
    let (start, end, task_id) = schedules[idx];
    let dur = end.0 - start.0;
    let range = shift_range(current, dur, rng);
    let k = rand_range(rng, -range, range + 1);

    let new_start_0 = (start.0 + k).max(planner.now.0);
    let mut new_scheds = schedules.to_vec();
    new_scheds[idx] = (Point(new_start_0), Point(new_start_0 + dur), task_id);
    Some(Plan {
        schedules: new_scheds,
    })
}

fn neighbor_swap(_planner: &Planner, current: &Plan, rng: &mut impl Rng) -> Option<Plan> {
    let schedules = &current.schedules;
    if schedules.len() < 2 {
        return None;
    }
    let a = rng.random_range(0..schedules.len());
    let mut b = rng.random_range(0..schedules.len());
    if b == a {
        b = (a + 1) % schedules.len();
    }

    let (a_s, a_e, a_id) = schedules[a];
    let (b_s, b_e, b_id) = schedules[b];
    let a_dur = a_e.0 - a_s.0;
    let b_dur = b_e.0 - b_s.0;

    let mut new_scheds = schedules.to_vec();
    new_scheds[a] = (b_s, Point(b_s.0 + a_dur), a_id);
    new_scheds[b] = (a_s, Point(a_s.0 + b_dur), b_id);
    Some(Plan {
        schedules: new_scheds,
    })
}

fn neighbor_duration(_planner: &Planner, current: &Plan, rng: &mut impl Rng) -> Option<Plan> {
    let schedules = &current.schedules;
    if schedules.is_empty() {
        return None;
    }
    let idx = rng.random_range(0..schedules.len());
    let (start, end, task_id) = schedules[idx];
    let dur = end.0 - start.0;
    if dur <= 1 {
        return None;
    }
    let delta: i64 = if rng.random::<bool>() { 1 } else { -1 };
    let new_dur = dur + delta;
    if new_dur < 1 {
        return None;
    }

    let mut new_scheds = schedules.to_vec();
    new_scheds[idx] = (start, Point(start.0 + new_dur), task_id);
    Some(Plan {
        schedules: new_scheds,
    })
}

fn neighbor_reorder(_planner: &Planner, current: &Plan, rng: &mut impl Rng) -> Option<Plan> {
    let schedules = &current.schedules;
    if schedules.len() < 2 {
        return None;
    }
    let a = rng.random_range(0..schedules.len());
    let mut b = rng.random_range(0..schedules.len());
    if b == a {
        b = (a + 1) % schedules.len();
    }

    let (a_s, _a_e, _a_id) = schedules[a];
    let (b_s, _b_e, _b_id) = schedules[b];

    let (first, second) = if a_s.0 <= b_s.0 { (a, b) } else { (b, a) };
    let f_s = schedules[first].0;
    let f_e = schedules[first].1;
    let f_dur = f_e.0 - f_s.0;
    let s_s = schedules[second].0;
    let s_e = schedules[second].1;
    let s_dur = s_e.0 - s_s.0;

    let mut new_scheds = schedules.to_vec();
    new_scheds[first] = (s_s, Point(s_s.0 + f_dur), schedules[first].2);
    new_scheds[second] = (f_s, Point(f_s.0 + s_dur), schedules[second].2);

    Some(Plan {
        schedules: new_scheds,
    })
}

/// 依存違反を一つ選んで修復する: 依存先の終了直後に依存元タスクを移動。
/// `pinned_ids` が Some の場合、pinned タスクは移動しない。
fn neighbor_repair_depend(
    planner: &Planner,
    current: &Plan,
    rng: &mut impl Rng,
    pinned_ids: Option<&FxHashSet<usize>>,
) -> Option<Plan> {
    let schedules = &current.schedules;
    let mut index: Vec<Option<(Point, Point)>> = vec![None; planner.tasks.len()];
    for (s, e, id) in schedules {
        if *id < index.len() {
            index[*id] = Some((*s, *e));
        }
    }

    let mut violations: Vec<(usize, Point)> = Vec::new();
    for task in &planner.tasks {
        if let Some(p) = pinned_ids
            && p.contains(&task.id)
        {
            continue;
        }
        let Some((start, _)) = index[task.id] else {
            continue;
        };
        let mut latest_dep_end: Option<Point> = None;
        for dep_id in &task.depends {
            if let Some(Some((_, dep_end))) = index.get(*dep_id)
                && *dep_end > start
            {
                latest_dep_end = Some(latest_dep_end.map_or(*dep_end, |m| m.max(*dep_end)));
            }
        }
        if let Some(dep_end) = latest_dep_end {
            violations.push((task.id, dep_end));
        }
    }

    if violations.is_empty() {
        return None;
    }

    let (task_id, new_start) = violations[rng.random_range(0..violations.len())];
    let pos = schedules.iter().position(|(_, _, id)| *id == task_id)?;
    let (start, end, _) = schedules[pos];
    let dur = end.0 - start.0;

    let mut new_scheds = schedules.to_vec();
    new_scheds[pos] = (new_start, Point(new_start.0 + dur), task_id);
    Some(Plan {
        schedules: new_scheds,
    })
}

fn neighbor_lns(planner: &Planner, current: &Plan, rng: &mut impl Rng) -> Option<Plan> {
    let schedules = &current.schedules;
    if schedules.is_empty() {
        return None;
    }

    let pivot_idx = rng.random_range(0..schedules.len());
    let (pivot_start, pivot_end, _) = schedules[pivot_idx];

    let window_size = ((pivot_end.0 - pivot_start.0) * 2)
        .max(4)
        .min(schedules.iter().map(|(s, e, _)| e.0 - s.0).sum::<i64>() / 3 + 1);

    let window_start = pivot_start.0 - rand_range(rng, 0, window_size / 2 + 1);
    let window_end = window_start + window_size;

    let mut destroyed_ids = Vec::new();
    let mut remaining = Vec::new();
    for sched in schedules {
        if sched.0.0 >= window_start && sched.0.0 < window_end {
            destroyed_ids.push(sched.2);
        } else {
            remaining.push(*sched);
        }
    }

    let rebuilt = greedy_rebuild(planner, &remaining, &destroyed_ids);

    Some(Plan { schedules: rebuilt })
}

fn greedy_rebuild(
    planner: &Planner,
    existing: &[(Point, Point, usize)],
    task_ids: &[usize],
) -> Vec<(Point, Point, usize)> {
    let mut scheds = existing.to_vec();

    let mut pending: Vec<usize> = task_ids.to_vec();
    pending.sort_by(|a, b| {
        planner
            .freeness(*a)
            .partial_cmp(&planner.freeness(*b))
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let destroyed: FxHashSet<usize> = task_ids.iter().copied().collect();
    let mut placed: FxHashSet<usize> = FxHashSet::default();

    // 依存先が先に配置されるよう、配置可能なタスクから複数パスで配置する。
    while !pending.is_empty() {
        let mut progressed = false;
        let mut next_pending = Vec::new();

        for task_id in pending {
            let task = &planner.tasks[task_id];
            let deps_ready = task
                .depends
                .iter()
                .all(|d| !destroyed.contains(d) || placed.contains(d));
            if !deps_ready {
                next_pending.push(task_id);
                continue;
            }

            place_one(planner, &mut scheds, task_id);
            placed.insert(task_id);
            progressed = true;
        }

        if !progressed {
            for task_id in next_pending {
                place_one(planner, &mut scheds, task_id);
            }
            break;
        }
        pending = next_pending;
    }

    scheds
}

fn place_one(planner: &Planner, scheds: &mut Vec<(Point, Point, usize)>, task_id: usize) {
    let task = &planner.tasks[task_id];
    // build_initial と同様、avg=0 のタスクは dur=1 として配置する。
    // さもないと iCal 由来の avg=0 タスクが LNS/repair_polish の再構築で
    // サイレントにドロップされてしまう (inclusion_bonus の不整合)。
    let dur = (task.cost_estimate.avg as i64).max(1);
    let earliest = compute_earliest(planner, scheds, task);
    if let Some((start, end)) = try_place(planner, scheds, task, earliest, dur) {
        scheds.push((start, end, task_id));
    } else {
        // build_initial と同様、配置できない場合は末尾に fallback してタスクを落とさない。
        let last_end = scheds
            .iter()
            .map(|(_, e, _)| e.0)
            .max()
            .unwrap_or(planner.now.0);
        let fallback_start = Point(last_end).max(planner.now).max(earliest);
        scheds.push((fallback_start, Point(fallback_start.0 + dur), task_id));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::rng;

    fn test_planner(tasks: Vec<Task>) -> Planner {
        Planner {
            tasks,
            now: Point(0),
            per: 5,
            sleep: SleepConfig::disabled(),
            previous_schedule: vec![],
        }
    }

    #[test]
    fn build_initial_dependency_order() {
        let t0 = Task {
            id: 0,
            start: Some(Point(0)),
            end: Point(10),
            cost_estimate: NormalDist { avg: 2, sigma: 0 },
            depends: vec![],
            parallelizable: false,
            allows_parallel: false,
            abandonability: 0.5,
        };
        let t1 = Task {
            id: 1,
            start: Some(Point(0)),
            end: Point(10),
            cost_estimate: NormalDist { avg: 2, sigma: 0 },
            depends: vec![0],
            parallelizable: false,
            allows_parallel: false,
            abandonability: 0.5,
        };
        let p = test_planner(vec![t0, t1]);
        let plan = build_initial(&p);
        assert_eq!(plan.schedules.len(), 2, "both tasks should be scheduled");
        let t0_entry = plan.schedules.iter().find(|(_, _, id)| *id == 0).unwrap();
        let t1_entry = plan.schedules.iter().find(|(_, _, id)| *id == 1).unwrap();
        assert!(
            t0_entry.1.0 <= t1_entry.0.0,
            "task 0 must end before task 1 starts"
        );
    }

    #[test]
    fn build_initial_schedules_all() {
        let t0 = Task {
            id: 0,
            start: Some(Point(0)),
            end: Point(10),
            cost_estimate: NormalDist { avg: 2, sigma: 0 },
            depends: vec![],
            parallelizable: false,
            allows_parallel: false,
            abandonability: 0.5,
        };
        let t1 = Task {
            id: 1,
            start: Some(Point(0)),
            end: Point(10),
            cost_estimate: NormalDist { avg: 2, sigma: 0 },
            depends: vec![],
            parallelizable: false,
            allows_parallel: false,
            abandonability: 0.5,
        };
        let p = test_planner(vec![t0, t1]);
        let plan = build_initial(&p);
        assert_eq!(plan.schedules.len(), 2, "all tasks should be scheduled");
    }

    #[test]
    fn sa_lns_finds_buffer_ordering() {
        let t0 = Task {
            id: 0,
            start: Some(Point(0)),
            end: Point(5),
            cost_estimate: NormalDist { avg: 1, sigma: 0 },
            depends: vec![],
            parallelizable: false,
            allows_parallel: false,
            abandonability: 0.5,
        };
        let t1 = Task {
            id: 1,
            start: Some(Point(0)),
            end: Point(5),
            cost_estimate: NormalDist { avg: 1, sigma: 2 },
            depends: vec![],
            parallelizable: false,
            allows_parallel: false,
            abandonability: 0.5,
        };
        let p = test_planner(vec![t0, t1]);
        let mut rng = rng();
        let plan = sa_lns(&p, &mut rng);

        assert_eq!(plan.schedules.len(), 2, "both tasks should be scheduled");

        let b_entry = plan.schedules.iter().find(|(_, _, id)| *id == 1).unwrap();
        let a_entry = plan.schedules.iter().find(|(_, _, id)| *id == 0).unwrap();

        let b_score = evaluate(&p, &plan, 0.0, 1.0);
        let swapped = Plan {
            schedules: vec![
                (a_entry.0, a_entry.1, b_entry.2),
                (b_entry.0, b_entry.1, a_entry.2),
            ],
        };
        let swapped_score = evaluate(&p, &swapped, 0.0, 1.0);

        assert!(
            b_score >= swapped_score,
            "B→A should score at least as well as A→B: b_score={b_score} swapped={swapped_score}"
        );
    }

    #[test]
    fn sa_lns_respects_dependencies() {
        let t0 = Task {
            id: 0,
            start: Some(Point(0)),
            end: Point(10),
            cost_estimate: NormalDist { avg: 2, sigma: 0 },
            depends: vec![],
            parallelizable: false,
            allows_parallel: false,
            abandonability: 0.5,
        };
        let t1 = Task {
            id: 1,
            start: Some(Point(0)),
            end: Point(10),
            cost_estimate: NormalDist { avg: 2, sigma: 0 },
            depends: vec![0],
            parallelizable: false,
            allows_parallel: false,
            abandonability: 0.5,
        };
        let p = test_planner(vec![t0, t1]);
        let mut rng = rng();
        let plan = sa_lns(&p, &mut rng);

        let t0_entry = plan.schedules.iter().find(|(_, _, id)| *id == 0).unwrap();
        let t1_entry = plan.schedules.iter().find(|(_, _, id)| *id == 1).unwrap();
        assert!(t0_entry.1.0 <= t1_entry.0.0, "SA must respect dependencies");
    }

    // Regression: zero-avg tasks (e.g. iCal imports with avg_minutes=0) must
    // not be silently dropped by greedy_rebuild/place_one. build_initial
    // places them with dur=1, so the rebuild path must be consistent.
    #[test]
    fn greedy_rebuild_keeps_zero_avg_task() {
        let zero = Task {
            id: 0,
            start: Some(Point(0)),
            end: Point(100),
            cost_estimate: NormalDist { avg: 0, sigma: 0 },
            depends: vec![],
            parallelizable: false,
            allows_parallel: false,
            abandonability: 0.5,
        };
        let other = Task {
            id: 1,
            start: Some(Point(0)),
            end: Point(100),
            cost_estimate: NormalDist { avg: 3, sigma: 0 },
            depends: vec![],
            parallelizable: false,
            allows_parallel: false,
            abandonability: 0.5,
        };
        let p = test_planner(vec![zero, other]);

        // Destroy both and rebuild from empty — both must come back.
        let rebuilt = greedy_rebuild(&p, &[], &[0, 1]);
        assert_eq!(
            rebuilt.len(),
            2,
            "zero-avg task must not be dropped by greedy_rebuild: {rebuilt:?}"
        );
        assert!(rebuilt.iter().any(|(_, _, id)| *id == 0));
    }

    // Regression: repair_polish must not drop a zero-avg violator. Even when
    // the violator has avg=0, it should be re-placed rather than removed.
    #[test]
    fn repair_polish_keeps_zero_avg_violator() {
        let dep = Task {
            id: 0,
            start: Some(Point(0)),
            end: Point(100),
            cost_estimate: NormalDist { avg: 5, sigma: 0 },
            depends: vec![],
            parallelizable: false,
            allows_parallel: false,
            abandonability: 0.5,
        };
        // zero-avg task that depends on dep, but is scheduled before dep ends.
        let violator = Task {
            id: 1,
            start: Some(Point(0)),
            end: Point(100),
            cost_estimate: NormalDist { avg: 0, sigma: 0 },
            depends: vec![0],
            parallelizable: false,
            allows_parallel: false,
            abandonability: 0.5,
        };
        let p = test_planner(vec![dep, violator]);
        // Force a dependency violation: dep ends at 5, violator starts at 0.
        let bad = Plan {
            schedules: vec![(Point(0), Point(5), 0), (Point(0), Point(1), 1)],
        };
        let polished = repair_polish(&p, bad, None);
        assert_eq!(
            polished.schedules.len(),
            2,
            "zero-avg violator must not be dropped by repair_polish: {:?}",
            polished.schedules
        );
        assert!(polished.schedules.iter().any(|(_, _, id)| *id == 1));
    }
}
