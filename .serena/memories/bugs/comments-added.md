# Comments added for future agents

## Files modified with explanatory comments

### `crates/takusu-core/src/lib.rs`
- **`plan_in_range`**: Documented the known limitation: dependencies FROM pinned tasks TO rescheduled tasks are NOT enforced. Sub-planner has no pinned tasks, so those deps are invisible to SA. Real-world impact is low (pinned tasks are typically out-of-range and already complete).
- **`freeness()`**: Documented counterintuitive naming: high freeness = deprioritized. The name suggests "more free = higher priority" but the reverse is true. Build-initial sorts by freeness ascending (lowest = most urgent = placed first).

### `crates/takusu-core/src/evaluate.rs`
- Added detailed weight rationale comments under "重みの根拠" section explaining why each constant has its value. Key insights:
  - W_SHORT=3 is quadratic (deficit²): avg-matching is critical; too-short is heavily penalized.
  - W_OVER=0.5 is linear: over-allocation is lightly penalized; packing more tasks is higher priority.
  - W_SLEEP_SEVERE=15 is quadratic below 3h: the hard threshold design philosophy is "sacrifice tasks before sacrificing sleep".
  - W_DEPEND_BASE=100: constraint-annealed via (1-T/T0); at T→0 it reaches max.

### `crates/takusu-core/src/anneal.rs`
- Added design rationale section explaining:
  - Tabu list key design (task_id + start + duration, not full hash)
  - LNS window sizing (pivot_duration*2, min 4, max 1/3 total task time)
  - greedy_rebuild freeness ordering
  - Partial mode O(n) scan for unpinned positions (justified by n<100)
- Added doc comment on `topological_order`: clarifies it's only input for freeness sort, not the placement order.
- Added doc comment on `build_initial`: greedy heuristic, not guaranteed to produce feasible solution; SA improves from it.

### `crates/takusu-core/src/solver.rs`
- Added comment on parallel strategy: seed = 0..num_chains, max 4 chains for diminishing returns.
- Added doc comment on `solve_partial`: explains delegation to `solve` when pinned is empty.

### `crates/takusu-local-lib/src/storage_sqlite.rs`
- Added comment on `verify_token`: the `hash == hash(root_token)` condition looks redundant (SHA-256 collision resistance) but is kept as safety net for cases where root_token is passed as hex hash.

### `crates/takusu-local-lib/src/app.rs`
- `iso_to_point`/`point_to_iso`: Added warning about the hardcoded 5-minute slot; references AGENTS.md known issue.
- `load_task_rows`: Documented that NotFound is silently ignored when specific task_ids are provided; designed for tolerance but could warn at API level.
- `build_planner`: Documented the id_to_idx dual-use pattern (first initialized with slice index, then overwritten with planner index; consistent as long as no partial failure).
- `generate_schedule`: Documented the double status filter (load_task_rows filters once, caller filters again for habit row safety) and why "scheduled" is included.
- `reschedule("tasks" mode)`: Documented the pinned filter logic.
- `move_entry`: Documented that only deadline is checked (not deps, sleep, parallel); intentional for user manual override.
- `sync_habit_tasks`: Documented the stale habit task cleanup: only deletes pending tasks; non-pending user-modified tasks are preserved.

## Unfixed bugs discovered

1. **`plan_in_range` dependency gap (documented)**: Dependencies from pinned→unpinned tasks not enforced during sub-planner SA. Mitigated by typical usage patterns.
2. **`move_entry` partial validation (documented)**: Only checks deadline; dependency/sleep/parallel violations are silently accepted with warnings only for deadline.
3. **`load_task_ids` silent drop (documented)**: Non-existent task IDs are silently ignored when specific task_ids are provided.
4. **`verify_token` redundant hash check (documented)**: `hash == hash(root_token)` is SHA-256-collision redundant; kept as safety for odd token formats.
