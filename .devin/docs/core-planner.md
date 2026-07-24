# Core Planner Design (takusu-core)

## Algorithm

- **SA + LNS + Tabu Search**: Simulated Annealing with 5 neighbor types
  (shift 25% / swap 25% / duration 20% / reorder 15% / LNS 15%)
- **Parallel restarts**: `rayon` — 1 chain per CPU core (max 4), best solution
  selected
- **Constraint annealing**: Dependency penalty proportional to `(1 - T/T₀)`,
  allowing feasibility-boundary crossing at high temperature, hard constraint
  at T→0
- **Evaluation caching**: `eval_current` / `eval_best` cached per temperature
  step; only `eval_neighbor` computed per iteration (3-4x speedup over naive)
- **Cooling**: α=0.93, `iter_per_temp = tasks × 20`, T₀ = total_avg × 0.1
- **Partial rescheduling**: `plan_partial(pinned)` pins tasks, optimizes the
  rest. `plan_in_range(range, schedule, extra_pinned)` identifies out-of-range
  tasks from current schedule and reschedules only within-range tasks.

## Evaluation Function (8 components)

All penalty weights are positive. Penalties are applied as negative
contributions.

| Component | Formula | Weight | Notes |
|-----------|---------|--------|-------|
| deadline_score | `slack × W_EARLY` (early) / `slack × W_LATE × (1−abandonability)` (late) | 1.0 / 20.0 | Early bonus capped at 50 |
| start_score | `(sched_start−start) × W_START` | 5.0 | Only when scheduled too early |
| depend_score | `−violation_slots × W_DEPEND_BASE × (1−T/T₀)` | 100.0 | Proportional, constraint-annealed |
| buffer_score | `sigma × remaining_slots × W_BUFFER` | 2.0 | High-sigma tasks get more buffer |
| duration_score | `−deficit² × W_SHORT` / `deficit × W_OVER` | 3.0 / 0.5 | Quadratic for too-short |
| sleep_score | per-day: `−sleep_used × W_NORMAL`, `−deficit² × W_SEVERE` below 3h | 4.0 / 15.0 | 3h hard threshold |
| parallel_violation | `−overlap_slots × W_PARALLEL_VIOL` | 500.0 | Proportional to illegal overlap (near-hard constraint) |
| inclusion_bonus | `+W_INCLUSION × scheduled_tasks` | 10.0 | Rewards keeping tasks |

## Penalty Design Principle

**Penalties proportional to violation magnitude**, not flat per-occurrence.
This ensures SA gradients guide towards feasibility rather than oscillating.
- `depend_score`: violation of 90 slots costs 90× more than 1 slot
- `parallel_violation`: overlap of 10 slots costs 10× more than 1 slot

## abandonability Semantics

- **High** (= 0.8–1.0): deadline miss is acceptable. `deadline_score` penalty
  is multiplied by `(1 − abandonability)`. Task is **never dropped**.
- **Low** (= 0.0–0.2): deadline must be met. Full penalty.
- Tasks are always scheduled (no dropping). If impossible, they get placed at
  end of schedule and SA minimizes total penalty.

## Time Model

- 1 slot = 5 minutes. `Point(i64)` = number of 5-min slots from epoch.
- `Point::from_timestamp(ts, 5)` for jiff conversion.
- `Point::from_raw(n)` for direct slot value.
- Sleep window: defined in slots relative to `day_start`. Default: 22:00–06:00.
- `SleepConfig::from_local(per, tz, start_h, start_m, end_h, end_m)`: converts
  local clock times to slot-based `SleepConfig`, computing `day_start` from
  timezone offset. Used by `parse_sleep` in the server to make "recommended"
  timezone-aware.

## Benchmark

```sh
cargo bench -p takusu-core
# plan/plan 25 tasks: ~148 ms (4 parallel SA chains)
```

25 tasks with random dependencies, σ values, and deadlines.
Run on debug with `cargo run --example daily` for a human-readable 9-task
example.
