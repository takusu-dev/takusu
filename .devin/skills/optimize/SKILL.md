---
name: optimize
description: Optimize takusu-core planner performance with benchmark-driven profiling and the change-based workflow
argument-hint: "[area]"
---

Optimize the performance of `takusu-core` (primarily `evaluate.rs` and `anneal.rs`) using a benchmark-driven, checkpoint-friendly workflow.

## Before changing code

1. Record a baseline.
   - `cargo bench -p takusu-core --bench realworld`
   - `cargo run -p takusu-core --example score_check`
   - `cargo run -p takusu-core --example score_check --release`
   - `cargo run -p takusu-core --example profile --release` (and `time ./target/release/examples/profile` for a stable wall-clock baseline)
   - Record the numbers in `design/optimization-baselines.md` with the fixture, command, and revision so later runs have an authoritative comparison.
2. Profile the hot path.
   - Use the `profile` skill (`/profile`) or run `./scripts/profile.sh --example score_check -p takusu-core`.
   - Read `target/profile/top-self.txt` and `target/profile/flamegraph.svg`.
3. Identify the biggest self-time contributors. Common hot spots:
   - `daily_load_score`, `sleep_score`, `task_and_depend_scores`
   - `generate_neighbor` allocations (`to_vec`, `Plan` clones)
   - `build_index`, sorting, `union_length_sorted`

## Making changes

- Work in small `jj` changes (`jj new`). Squash successful experiments into the parent change with `jj squash`.
  - If the parent change is the current optimization work, squash into it.
  - If there is no optimization change yet (e.g. you are on an empty `@` directly above `main`), make `@` the optimization change with `jj describe`, then start experiments as child changes on top of it.
- If an experiment regresses benchmarks, abandon it (`jj abandon @`) and return to the last good change.
- Prefer changes that reduce per-`evaluate` allocations and loop overhead:
  - Reuse `Vec` scratch buffers across SA iterations (`evaluate_with_scratch`).
  - Merge loops that iterate over the same data (e.g. deadline/start/duration/depend scores).
  - Avoid sorting already-sorted data; pass sorted slices down.
  - Skip failed neighbor generations instead of evaluating a clone of the current plan.
- Avoid adding new dependencies or unsafe code unless the user asks.
- Do not change the public `evaluate` signature or planner semantics unless required.

## Verification after each change

1. `cargo check -p takusu-core`
2. `cargo nextest run -p takusu-core`
3. `cargo clippy -p takusu-core`
4. `cargo fmt`
5. `cargo bench -p takusu-core --bench realworld` — compare to baseline.
6. `cargo run -p takusu-core --example score_check` — total time / mean time per evaluate should improve; score is a sanity check (same order of magnitude across runs).

## When to checkpoint / PR

- After each meaningful optimization chunk, `jj describe` and `jj git push --change`.
- Update the existing PR with `gh pr edit` or create a new one with `gh pr create`.
- Update docs/comments when score functions or workflows change.

## Recording failed experiments

- Document experiments that regress benchmarks in `design/fail-optimization.md` with the change, baseline, and results before abandoning them (`jj abandon @`).
- Keep `design/fail-optimization.md` and any skill/workflow updates in a separate `jj` change from the failed optimization code, and be careful not to abandon them.

## When to ask the user

- Before large architectural changes (e.g. in-place neighbor mutation, changing `Plan` invariants, adding dependencies).
- If benchmarks become noisy or multiple experiments regress.
- If the profiling output does not clearly point to a single bottleneck.
