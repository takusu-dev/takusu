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
