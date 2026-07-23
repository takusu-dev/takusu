# Failed Optimization Trials

This file records optimization experiments that did not improve all benchmarks, so future work can learn from them.

> This file and any skill/workflow updates should be kept in a separate `jj` change from the failed optimization code. Do not abandon them when abandoning a failed experiment.

---

## 2026-07-18: `jemallocator` 0.5.4 fails to build on glibc 2.42

### Change

- Tried adding `jemallocator = "0.5.4"` as an optional global allocator for `takusu-core`.

### Result

- `jemalloc-sys` fails to compile with glibc 2.42 in this Nix dev shell.
  Error: `returning 'char *' from a function with return type 'int' makes integer from pointer without a cast` in `src/malloc_io.c`.
- The `tikv-jemallocator` 0.6.0 fork builds and performs best, so the experiment was switched to that crate.

### Status

- `jemallocator` (original crate) was not kept. `tikv-jemallocator` was adopted instead.

---

## 2026-07-18: `mimalloc` 0.1.50 global allocator

### Change

- Added `mimalloc = "0.1.50"` as an optional global allocator alongside `jemalloc`.

### Result

- `mimalloc` improved release `score_check` and `profile` slightly, but did not show significant change on the `realworld` 7d/30d Criterion benches.
- Its debug build regressed `score_check` compared to the system allocator.
- It complicated the feature matrix and `--all-features` handling.

### Status

- Removed. Only `jemalloc` is kept as an optional/default allocator for `takusu-core`.

---

## 2026-07-17: Reuse evaluate scratch buffers + inline union calculation

### Change

- `crates/takusu-core/src/evaluate.rs`
  - Added thread-local reusable scratch buffers for the public `evaluate()` to avoid allocating `sorted`/`index` on every call.
  - Changed `evaluate_with_scratch()` to take an `index` scratch buffer instead of allocating `build_index()` each call.
  - Inlined `union_length_sorted()` inside `sleep_score()` and `daily_load_score()` to avoid pushing into a shared `pair_scratch` buffer.
  - Removed the `pair_scratch` parameter and the unused `union_length_sorted()` helper.
- `crates/takusu-core/src/anneal.rs`
  - Updated `sa_lns()`, `sa_lns_partial()`, and `repair_polish()` to pass an `index` scratch buffer to `evaluate_with_scratch()`.

### Rationale

Profiling (`score_check` and `profile` examples) showed `daily_load_score`, `sleep_score`, `build_index`, and allocation routines (`_int_malloc`, `copy_nonoverlapping`) as top self-time contributors. Reusing buffers and eliminating per-call `Vec` pushes were expected to speed up both `evaluate()` and the SA hot loop.

### Baseline (before change)

- `cargo run -p takusu-core --example score_check`:
  - score: `-1844.372500`
  - total time: `5.981s`
  - mean time per evaluate: `59.805051 µs`
- `cargo bench -p takusu-core --bench realworld`:
  - `plan realworld habits (7d)`: `70.881 ms`
  - `plan realworld habits (30d)`: `1.1982 s`
  - `plan_partial realworld habits (14d, 5 pinned)`: `281.25 ms`
  - `plan_in_range realworld habits (14d, days 2-7)`: `62.569 ms`

### Result (after change)

- `cargo run -p takusu-core --example score_check`:
  - score: `-1844.372500`
  - total time: `1.717s`
  - mean time per evaluate: `17.165584 µs`
- `cargo bench -p takusu-core --bench realworld`:
  - `plan realworld habits (7d)`: `34.635 ms` (**-51.1%**)
  - `plan realworld habits (30d)`: `569.79 ms` (**-52.5%**)
  - `plan_partial realworld habits (14d, 5 pinned)`: `371.86 ms` (**+32.1%**, p=0.11, high variance/outliers)
  - `plan_in_range realworld habits (14d, days 2-7)`: `102.38 ms` (**+63.6%**, p=0.00)

### Observation

The change dramatically improved `score_check` and full `plan()` benchmarks, but caused a clear regression in `plan_in_range` and a possible regression in `plan_partial`. The public `evaluate()` and full-plan path improved, while the partial/range paths did not. The cause is not yet understood; possible explanations include benchmark noise, `thread_local` `RefCell` overhead interacting with the `solve_partial` `max_by` path, or an unintended interaction between the new scratch-buffer lifetimes and `sa_lns_partial`/`build_initial_partial`.

### Follow-up

Re-running `plan_in_range` against the actual parent commit (not the Criterion-stored baseline) showed the public `evaluate()` `thread_local` `RefCell` approach was responsible for the measured slowdown. Removing the thread-local and allocating fresh `sorted`/`index` buffers in `evaluate()`:

- `cargo run -p takusu-core --example score_check --release`: `1.931 µs` → `1.542 µs` (manual baseline vs. final code)
- manual `plan_in_range` over the 14d fixture: `87.9 ms` → `79.0 ms` per call

The Criterion `realworld` numbers varied wildly between runs (e.g. `realworld 7d` reported both `34 ms` and `53 ms` for the same code), so the initial `plan_in_range` regression was most likely measurement noise combined with the `thread_local` overhead. The `score_check` debug run (`28.4 µs` → `23.5 µs`) also improved.

### Status

The experiment was kept and refined: `evaluate_with_scratch()` still reuses scratch buffers in the SA hot loop, `union_length_sorted()` stays inlined, and the public `evaluate()` wrapper allocates per call rather than using `thread_local`.

---

## 2026-07-18: Skip passed intervals in `sleep_score`/`daily_load_score` scans

### Change

- `crates/takusu-core/src/evaluate.rs`
  - Added a `start_idx` cursor to `sleep_score()` and `daily_load_score()`.
  - Sliced `sorted` with `&sorted[start_idx..]` before calling `union_length_in_window()` so already-passed intervals are not re-scanned for every day/sleep window.

### Rationale

Profiling showed `daily_load_score`, `sleep_score`, and `union_length_in_window` as top self-time contributors. Since `sorted` is sorted by start and both scoring loops iterate over monotonically increasing windows, a running start index should skip intervals that ended before the current window.

### Baseline (before this experiment, after the habit score optimization)

- `cargo bench -p takusu-core --bench realworld`:
  - `plan realworld habits (7d)`: `21.326 ms`
  - `plan realworld habits (30d)`: `461.21 ms`
  - `plan_partial realworld habits (14d, 5 pinned)`: `148.74 ms`
  - `plan_in_range realworld habits (14d, days 2-7)`: `53.013 ms`

### Result (after adding per-window `start_idx` slices)

- First run:
  - `plan realworld habits (7d)`: `22.387 ms` (**+5.0%**)
  - `plan realworld habits (30d)`: `481.41 ms` (**+4.4%**)
  - `plan_partial realworld habits (14d, 5 pinned)`: `153.22 ms` (**+3.0%**)
  - `plan_in_range realworld habits (14d, days 2-7)`: `55.957 ms` (**+5.6%**)
- Second run (same binary, likely warmer allocator/CPU):
  - `plan realworld habits (7d)`: `21.433 ms`
  - `plan realworld habits (30d)`: `463.55 ms`
  - `plan_partial realworld habits (14d, 5 pinned)`: `149.49 ms`
  - `plan_in_range realworld habits (14d, days 2-7)`: `53.190 ms`

### Observation

The `start_idx` cursor added a small per-iteration overhead and did not yield a consistent improvement across the `realworld` suite. The 30d benchmark sometimes improved (first run +4%, second run ~same), but the 7d, `plan_partial`, and `plan_in_range` benches regressed or stayed flat. The `union_length_in_window()` scan is already cheap due to early `continue`/`break`, so the cost of maintaining and slicing from `start_idx` outweighs the benefit on these fixtures.

### Status

Abandoned. `sleep_score()` and `daily_load_score()` keep their original full-slice `union_length_in_window()` calls.

---

## 2026-07-19: single-pass habit sort by `(group, time_of_day)`

### Change

- `crates/takusu-core/src/evaluate.rs` (`habit_consistency_score()`)
  - Replaced group-only sort + per-group `tod` sort with a single `entries.sort_unstable_by_key(|e| (e.0, e.1))`.
  - Removed the second per-group `sort_unstable_by_key(|e| e.1)`.

### Rationale

Profiling showed `habit_consistency_score()` as a top self-time contributor (7.28%). Combining two sorts into one was expected to reduce comparison and key-extraction overhead.

### Baseline (before this experiment)

- `cargo run -p takusu-core --example score_check --release`:
  - score `-1844.372500`
  - total `0.084s`
  - mean `0.836737 µs`
- `cargo bench -p takusu-core --bench realworld`:
  - `plan realworld habits (7d)`: `22.064 ms`
  - `plan realworld habits (30d)`: `438.27 ms`
  - `plan_partial realworld habits (14d, 5 pinned)`: `142.35 ms`
  - `plan_in_range realworld habits (14d, days 2-7)`: `49.176 ms`

### Result (after single-pass tuple-key sort)

- `cargo run -p takusu-core --example score_check --release`:
  - score `-1844.372500`
  - total `0.138s`
  - mean `1.379904 µs` (**+64.9%**)
- `cargo bench -p takusu-core --bench realworld`:
  - `plan realworld habits (7d)`: `22.130 ms` (no significant change)
  - `plan realworld habits (30d)`: `461.28 ms` (**+5.2%**)
  - `plan_partial realworld habits (14d, 5 pinned)`: `866.71 ms` (**+509%**, high variance: 587–1255 ms)
  - `plan_in_range realworld habits (14d, days 2-7)`: `147.81 ms` (**+200%**, high variance: 111–236 ms)

### Observation

The single-pass tuple-key sort regressed `score_check` and the `realworld` suite. The `plan_partial` and `plan_in_range` paths saw especially large, high-variance slowdowns, suggesting the score change interacted badly with the partial SA landscape (more accepted neighbors, more `Plan` clones, and/or more `greedy_rebuild` work in `repair_polish`).

### Status

Abandoned. `habit_consistency_score()` reverts to the original group sort + per-group `tod` sort.

---

## 2026-07-19: `TabuList` backed by `FxHashSet` for O(1) lookups

### Change

- `crates/takusu-core/src/anneal.rs` (`TabuList`)
  - Added an `FxHashSet<(usize, i64, i64)>` alongside the `VecDeque` FIFO.
  - `push()` removes the oldest entry from both the queue and the set, then inserts the new key.
  - `contains()` checks the set instead of scanning the `VecDeque`.

### Rationale

`is_tabu()` is called for every generated neighbor and previously scanned the `VecDeque` (capacity `2 * task_count`) for each schedule entry. A hash set was expected to turn that O(capacity) lookup into O(1).

### Baseline (before this experiment)

- `cargo bench -p takusu-core --bench realworld`:
  - `plan realworld habits (7d)`: `22.064 ms`
  - `plan realworld habits (30d)`: `438.27 ms`
  - `plan_partial realworld habits (14d, 5 pinned)`: `142.35 ms`
  - `plan_in_range realworld habits (14d, days 2-7)`: `49.176 ms`

### Result (after `FxHashSet` TabuList)

- `cargo bench -p takusu-core --bench realworld`:
  - `plan realworld habits (7d)`: `27.222 ms` (**+23.4%**)
  - `plan realworld habits (30d)`: `496.49 ms` (**+13.3%**)
  - `plan_partial realworld habits (14d, 5 pinned)`: `161.07 ms` (**+13.2%**)
  - `plan_in_range realworld habits (14d, days 2-7)`: `54.017 ms` (**+9.8%**)

### Observation

For the small tabu capacities in these fixtures (`2 * task_count`), the hash-set insert/remove and hashing overhead in `push()`/`contains()` outweighs the benefit of O(1) lookup. The linear scan of a small `VecDeque` is faster in practice.

### Status

Abandoned. `TabuList` keeps the original `VecDeque`-only linear scan.

---

## 2026-07-19: `union_length_in_window` raw `i64` arithmetic / `#[inline(always)]`

### Change

- `crates/takusu-core/src/evaluate.rs` (`union_length_in_window()`)
  - Tried replacing `Point` temporaries with raw `i64` values.
  - Tried marking the function `#[inline(always)]`.
  - Tried keeping `Point` with `#[inline(always)]`.

### Rationale

`union_length_in_window()` is a top self-time contributor. Avoiding `Point` construction and forcing inlining were expected to reduce per-day scan overhead.

### Baseline (before this experiment)

- `cargo run -p takusu-core --example score_check --release`:
  - score `-1844.372500`
  - total `0.084s`
  - mean `0.836737 µs`
- `cargo bench -p takusu-core --bench realworld`:
  - `plan realworld habits (7d)`: `22.064 ms`
  - `plan realworld habits (30d)`: `438.27 ms`
  - `plan_partial realworld habits (14d, 5 pinned)`: `142.35 ms`
  - `plan_in_range realworld habits (14d, days 2-7)`: `49.176 ms`

### Result

- Raw `i64` + `#[inline]`:
  - `score_check` release: `1.85 µs` (**+121%**)
- Raw `i64` + `#[inline(always)]`:
  - `score_check` release: `2.61 µs` (**+212%**)
- `Point` + `#[inline(always)]`:
  - `score_check` release: `1.69 µs` (**+102%**)
- `Point` + `#[inline(always)]` (full `realworld` bench):
  - `plan 7d`: `31.97 ms` (**+44.9%**)
  - `plan 30d`: `742.20 ms` (**+69.4%**)
  - `plan_partial`: `296.66 ms` (**+108%**)
  - `plan_in_range`: `86.95 ms` (**+76.9%**)

### Observation

Changing the function body or forcing inlining perturbed the compiler's optimization decisions enough that `sleep_score()`/`daily_load_score()` no longer inlined well with `evaluate_with_scratch()`. The original `Point` implementation with `#[inline]` is the fastest on this workload, so it should be left untouched.

### Status

Abandoned. `union_length_in_window()` reverts to the original `Point`/`#[inline]` implementation.
