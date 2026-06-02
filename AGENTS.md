# AGENTS.md

## Project Overview

takusu is a planner that automatically builds user schedules and a voice assistant using LLM as the UI. The design document is `main.typ` (in Japanese).

- **License**: MIT
- **Repository**: https://github.com/satler-git/takusu

## Tech Stack

- **Language**: Rust (edition 2024, stable toolchain)
- **Python**: Planned for audio processing (models exported via ONNX, no Python runtime needed)
- **Kotlin**: Planned for Android app
- **Version Control**: Jujutsu (`jj`) + Git (GitHub)
- **Nix**: `flake.nix` provides the dev shell (direnv with `use flake`)

## Project Structure

```
takusu/
├── main.typ                  # 設計ドキュメント (Typst)
├── Cargo.toml                # Rust workspace root
├── crates/
│   ├── takusu-core/          # Core planner (data types, scheduling algorithm)
│   │   ├── src/lib.rs        #   Public API (Point, Task, NormalDist, Planner, Plan)
│   │   ├── src/evaluate.rs   #   Evaluation function (8 components)
│   │   ├── src/anneal.rs     #   SA + LNS + Tabu Search
│   │   ├── src/solver.rs     #   Parallel restart + solve entry point
│   │   ├── benches/plan.rs   #   Criterion benchmark (25 tasks)
│   │   └── examples/daily.rs #   Example: 1-day schedule with 9 tasks
│   ├── takusu-serve/         # REST API server (Planner server)
│   │   └── src/main.rs       #   (placeholder: "Hello, world!")
│   ├── takusu-audio/         # Audio processing (recording + Whisper STT)
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── record.rs     #   (empty)
│   │       └── transcription.rs
│   └── google-cal/           # Google Calendar sync
│       └── src/lib.rs        #   (placeholder)
├── flake.nix                 # Nix development environment
├── rust-toolchain.toml       # Rust toolchain config
└── .envrc                    # direnv config
```

## Development Environment

Use `nix develop` or `direnv allow` to enter the development shell. The flake provides:
- `rust-bin` (from `rust-toolchain.toml`: stable + rustfmt + clippy)
- `z3` (SAT solver)
- `cargo-expand`, `cargo-nextest`

### Key Commands

| Command | Description |
|---------|-------------|
| `cargo check` | Type-check all crates |
| `cargo fmt` / `treefmt` | Format code |
| `cargo clippy` | Lint |
| `cargo nextest run -p takusu-core` | Run 18 tests |
| `cargo bench -p takusu-core` | Run benchmark (~148ms for 25 tasks) |
| `cargo run --example daily` | Run daily schedule example |
| `cargo test -p takusu-core --doc` | Run 2 doc-tests |

## Workspace Dependencies

| Crate | Version | Used by | Notes |
|-------|---------|---------|-------|
| `thiserror` | 2.0 | workspace | Error derive macro |
| `jiff` | 0.2.21 | takusu-core | Date/time handling |
| `rand` | 0.8 | takusu-core | RNG for SA |
| `rustc-hash` | 2.1 | takusu-core | `FxHashSet` (faster than std) |
| `rayon` | 1.10 | takusu-core | Parallel SA restarts |
| `criterion` | 0.5 | takusu-core (dev) | Benchmarking |
| `tokio` | 1.52.0 | workspace | Async runtime (full features) |
| `cpal` | 0.17.3 | takusu-audio | Audio input |
| `whisper-rs` | 0.16.0 | takusu-audio | Whisper STT |
| `hf-hub` | 0.5.0 | takusu-audio | HuggingFace model download |

## Core Planner Design (takusu-core)

### Algorithm

- **SA + LNS + Tabu Search**: Simulated Annealing with 5 neighbor types
  (shift 25% / swap 25% / duration 20% / reorder 15% / LNS 15%)
- **Parallel restarts**: `rayon` — 1 chain per CPU core (max 4), best solution selected
- **Constraint annealing**: Dependency penalty proportional to `(1 - T/T₀)`, allowing
  feasibility-boundary crossing at high temperature, hard constraint at T→0
- **Evaluation caching**: `eval_current` / `eval_best` cached per temperature step;
  only `eval_neighbor` computed per iteration (3-4x speedup over naive)
- **Cooling**: α=0.93, `iter_per_temp = tasks × 20`, T₀ = total_avg × 0.1

### Evaluation Function (8 components)

All penalty weights are positive. Penalties are applied as negative contributions.

| Component | Formula | Weight | Notes |
|-----------|---------|--------|-------|
| deadline_score | `slack × W_EARLY` (early) / `slack × W_LATE × (1−abandonability)` (late) | 1.0 / 20.0 | Early bonus capped at 50 |
| start_score | `(sched_start−start) × W_START` | 5.0 | Only when scheduled too early |
| depend_score | `−violation_slots × W_DEPEND_BASE × (1−T/T₀)` | 100.0 | Proportional, constraint-annealed |
| buffer_score | `sigma × remaining_slots × W_BUFFER` | 2.0 | High-sigma tasks get more buffer |
| duration_score | `−deficit² × W_SHORT` / `deficit × W_OVER` | 3.0 / 0.5 | Quadratic for too-short |
| sleep_score | per-day: `−sleep_used × W_NORMAL`, `−deficit² × W_SEVERE` below 3h | 4.0 / 15.0 | 3h hard threshold |
| parallel_violation | `−overlap_slots × W_PARALLEL_VIOL` | 50.0 | Proportional to illegal overlap |
| inclusion_bonus | `+W_INCLUSION × scheduled_tasks` | 10.0 | Rewards keeping tasks |

### Penalty Design Principle

**Penalties proportional to violation magnitude**, not flat per-occurrence.
This ensures SA gradients guide towards feasibility rather than oscillating.
- `depend_score`: violation of 90 slots costs 90× more than 1 slot
- `parallel_violation`: overlap of 10 slots costs 10× more than 1 slot

### abandonability Semantics

- **High** (= 0.8–1.0): deadline miss is acceptable. `deadline_score` penalty
  is multiplied by `(1 − abandonability)`. Task is **never dropped**.
- **Low** (= 0.0–0.2): deadline must be met. Full penalty.
- Tasks are always scheduled (no dropping). If impossible, they get placed
  at end of schedule and SA minimizes total penalty.

### Time Model

- 1 slot = 5 minutes. `Point(i64)` = number of 5-min slots from epoch.
- `Point::from_timestamp(ts, 5)` for jiff conversion.
- `Point::from_raw(n)` for direct slot value.
- Sleep window: defined in slots relative to `day_start`. Default: 22:00–06:00.

## Key Design Decisions (from main.typ)

- **Planner**: Uses heuristic algorithms (simulated annealing) with an evaluation
  function, not exact SAT solving despite z3 being in the dev shell.
  Tasks are discretized into 5-minute slots.
- **Voice Assistant**: Android `VoiceInteractionService` + server for Whisper/LLM
  processing. LLM fills in missing information (estimates, etc.) using memory
  of past similar tasks.
- **Task model**: Includes start time, deadline, cost estimate (normal distribution),
  dependencies, parallelizability, and `abandonability` (deadline flexibility).
- **No existing README** — the design document (`main.typ`) and this file serve
  as the primary project documentation.

## Code Style

- No comments by default (existing code has minimal comments)
- Uses `thiserror` for error types
- Module-level docs (`//!`) in each source file describe algorithm details
- Modules organized by domain (core, serve, audio, google-cal)
- Workspace-level dependency versions defined in root `Cargo.toml`
- `FxHashSet` over `HashSet` for performance-critical collections
- Edition 2024: `gen` is a reserved keyword → use `r#gen` for rand trait methods

## Benchmark

```sh
cargo bench -p takusu-core
# plan/plan 25 tasks: ~148 ms (4 parallel SA chains)
```

25 tasks with random dependencies, σ values, and deadlines.
Run on debug with `cargo run --example daily` for a human-readable 9-task example.
