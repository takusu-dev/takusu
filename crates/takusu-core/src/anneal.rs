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
    let mut cursor = earliest;

    loop {
        let candidate_end = Point(cursor.0 + dur);

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
    let iter_per_temp = task_count * 20;

    let mut tabu = TabuList::new(task_count * 2);
    let mut temperature = t0;

    let mut eval_current = evaluate(planner, &current, temperature, t0);
    let mut eval_best = eval_current;

    while temperature > t_min {
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
                }
            }
        }

        temperature *= alpha;
        eval_current = evaluate(planner, &current, temperature, t0);
        eval_best = evaluate(planner, &best, temperature, t0);
    }

    best
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
    let iter_per_temp = unpinned_count.max(1) * 20;

    let mut tabu = TabuList::new(task_count * 2);
    let mut temperature = t0;

    let mut eval_current = evaluate(planner, &current, temperature, t0);
    let mut eval_best = eval_current;

    while temperature > t_min {
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
                }
            }
        }

        temperature *= alpha;
        eval_current = evaluate(planner, &current, temperature, t0);
        eval_best = evaluate(planner, &best, temperature, t0);
    }

    best
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

    let r = rng.random_range(0..100u32) as i32;
    let idx = rng.random_range(0..unpinned.len());
    let target_id = unpinned[idx];

    let pos = current
        .schedules
        .iter()
        .position(|(_, _, id)| *id == target_id)
        .unwrap();

    let (start, end, task_id) = current.schedules[pos];
    let dur = end.0 - start.0;

    match r {
        0..=49 => {
            let range = (dur / 2).max(1);
            let k = rand_range(rng, -range, range + 1);
            let new_start_0 = (start.0 + k).max(planner.now.0);
            let mut new_scheds = current.schedules.to_vec();
            new_scheds[pos] = (Point(new_start_0), Point(new_start_0 + dur), task_id);
            Plan {
                schedules: new_scheds,
            }
        }
        50..=69 => {
            if dur <= 1 {
                return current.clone();
            }
            let delta: i64 = if rng.random::<bool>() { 1 } else { -1 };
            let new_dur = dur + delta;
            if new_dur < 1 {
                return current.clone();
            }
            let mut new_scheds = current.schedules.to_vec();
            new_scheds[pos] = (start, Point(start.0 + new_dur), task_id);
            Plan {
                schedules: new_scheds,
            }
        }
        _ => {
            if unpinned.len() < 2 {
                return current.clone();
            }
            let other_idx = rng.random_range(0..unpinned.len());
            if other_idx == idx {
                return current.clone();
            }
            let other_id = unpinned[other_idx];
            let other_pos = current
                .schedules
                .iter()
                .position(|(_, _, id)| *id == other_id)
                .unwrap();

            let (a_s, a_e, a_id) = current.schedules[pos];
            let (b_s, b_e, b_id) = current.schedules[other_pos];
            let a_dur = a_e.0 - a_s.0;
            let b_dur = b_e.0 - b_s.0;

            let mut new_scheds = current.schedules.to_vec();
            new_scheds[pos] = (b_s, Point(b_s.0 + a_dur), a_id);
            new_scheds[other_pos] = (a_s, Point(a_s.0 + b_dur), b_id);
            Plan {
                schedules: new_scheds,
            }
        }
    }
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
        0..=24 => neighbor_shift(planner, current, rng),
        25..=49 => neighbor_swap(planner, current, rng),
        50..=69 => neighbor_duration(planner, current, rng),
        70..=84 => neighbor_reorder(planner, current, rng),
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
    let range = (dur / 2).max(1);
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

    let mut to_place: Vec<usize> = task_ids.to_vec();
    to_place.sort_by(|a, b| {
        planner
            .freeness(*a)
            .partial_cmp(&planner.freeness(*b))
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    for task_id in to_place {
        let task = &planner.tasks[task_id];
        let dur = task.cost_estimate.avg as i64;
        if dur == 0 {
            continue;
        }

        let earliest = compute_earliest(planner, &scheds, task);
        if let Some((start, end)) = try_place(planner, &scheds, task, earliest, dur) {
            scheds.push((start, end, task_id));
        }
    }

    scheds
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
}
