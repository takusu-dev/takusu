# Optimization Baselines

This file records stable baseline numbers for `takusu-core` performance work.

- Record numbers **before** starting an experiment.
- Use the same machine / load conditions when comparing.
- The `realworld` Criterion bench can be noisy on shared machines; prefer the
  release examples + `time ./target/release/examples/profile` for a stable
  wall-clock baseline.

## 2026-07-18: `main` before evaluate scratch-buffer optimization

- `cargo run -p takusu-core --example score_check` (debug):
  - score `-1844.372500`
  - total `2.837s`
  - mean `28.370907 µs`
- `cargo run -p takusu-core --example score_check --release`:
  - score `-1844.372500`
  - total `0.193s`
  - mean `1.931344 µs`
- `time ./target/release/examples/profile` (20 full `plan()` calls):
  - real `~3.51s` (three runs: 3.514s, 3.472s, 3.538s)
- `plan_in_range` over 14d fixture, 20 calls (manual `range_check` example):
  - total `1.758s`
  - mean `87.889 ms`

## 2026-07-18: after evaluate scratch-buffer + inline union optimization

- `cargo run -p takusu-core --example score_check` (debug):
  - score `-1844.372500`
  - total `2.350s`
  - mean `23.498439 µs`
- `cargo run -p takusu-core --example score_check --release`:
  - score `-1844.372500`
  - total `0.154s`
  - mean `1.541711 µs`
- `time ./target/release/examples/profile` (20 full `plan()` calls):
  - real `~3.45s` (three runs: 3.599s, 3.430s, 3.328s)
- `plan_in_range` over 14d fixture, 20 calls:
  - total `1.580s`
  - mean `79.0 ms`
