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

## 2026-07-18: current `@` (kpzytzys 8e01c02a) before allocator experiments

- `cargo run -p takusu-core --example score_check` (debug):
  - score `-1844.372500`
  - total `2.268s`
  - mean `22.678947 µs`
- `cargo run -p takusu-core --example score_check --release`:
  - score `-1844.372500`
  - total `0.159s`
  - mean `1.594242 µs`
- `time ./target/release/examples/profile` (20 full `plan()` calls):
  - real `~3.11s` (three runs: 3.227s, 3.135s, 2.980s)
- `cargo bench -p takusu-core --bench realworld`:
  - `plan realworld habits (7d)`: `40.865 ms`
  - `plan realworld habits (30d)`: `749.92 ms`
  - `plan_partial realworld habits (14d, 5 pinned)`: `241.08 ms`
  - `plan_in_range realworld habits (14d, days 2-7)`: `84.368 ms`

## 2026-07-18: with `jemalloc` feature (`--features jemalloc`)

- `cargo run -p takusu-core --example score_check` (debug, `CFLAGS='-O2 -U_FORTIFY_SOURCE'`):
  - score `-1844.372500`
  - total `1.534s`
  - mean `15.343852 µs`
- `cargo run -p takusu-core --example score_check --release`:
  - score `-1844.372500`
  - total `0.126s`
  - mean `1.261039 µs`
- `time ./target/release/examples/profile` (20 full `plan()` calls):
  - real `~2.06s` (three runs: 2.135s, 2.000s, 2.036s)
- `cargo bench -p takusu-core --bench realworld --features jemalloc`:
  - `plan realworld habits (7d)`: `27.430 ms`
  - `plan realworld habits (30d)`: `474.38 ms`
  - `plan_partial realworld habits (14d, 5 pinned)`: `148.36 ms`
  - `plan_in_range realworld habits (14d, days 2-7)`: `53.667 ms`

### Notes

- `jemalloc` (via `tikv-jemallocator` 0.6.0) is now the **default** global allocator for `takusu-core`.
- `mimalloc` 0.1.50 remains available as an opt-in feature (`--no-default-features --features mimalloc`).
- A workspace `Cargo.toml` profile override (`[profile.dev|test.package."tikv-jemalloc-sys"] opt-level = 3`) fixes the `_FORTIFY_SOURCE` warnings-as-errors in Nix dev shells with newer glibc.
- arm64 build verified: `cargo ndk -t aarch64-linux-android check -p takusu-core` succeeds with the default `jemalloc` feature.

## 2026-07-18: release profile tuning (`codegen-units = 1`, `lto = "thin"`)

Compared `opt-level = "s"`, `"z"`, and `3` for `takusu-core` release builds.
All runs use `jemalloc` default.

- `opt-level = 3` (speed):
  - `cargo run -p takusu-core --example score_check --release`: `0.106s`, `1.056395 µs`
  - `time ./target/release/examples/profile` (20 calls): `~1.79s` (1.827s, 1.790s, 1.765s)
  - `cargo bench -p takusu-core --bench realworld`:
    - `plan realworld habits (7d)`: `25.464 ms`
    - `plan realworld habits (30d)`: `474.81 ms`
    - `plan_partial realworld habits (14d, 5 pinned)`: `142.47 ms`
    - `plan_in_range realworld habits (14d, days 2-7)`: `49.816 ms`
- `opt-level = "s"` (size):
  - `score_check` release: `0.135s`, `1.347508 µs`
  - `profile` 20 calls: `~2.43s` (2.445s, 2.431s, 2.412s)
- `opt-level = "z"` (aggressive size):
  - `score_check` release: `0.185s`, `1.850095 µs`
  - `profile` 20 calls: `~4.53s` (4.481s, 4.507s, 4.638s)

### Notes

- `opt-level = 3` is the fastest across all measured `takusu-core` workloads.
- `"s"` and `"z"` regress runtime, with `"z"` being dramatically slower.
- Final workspace `Cargo.toml` uses `codegen-units = 1`, `lto = "thin"`, `opt-level = 3`.
