# AGENTS.md

## Project Overview

takusu is a planner that automatically builds user schedules and a voice assistant using LLM as the UI. The design document is `main.typ` (in Japanese).

- **License**: MIT
- **Repository**: https://github.com/satler-git/takusu

## Tech Stack

- **Language**: Rust (edition 2024, stable toolchain)
- **Python**: FunASR server for STT (SenseVoice-Small model, managed via `uv`)
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
│   │   ├── src/lib.rs        #   Public API (Point, Task, NormalDist, Planner, Plan, RescheduleRange)
│   │   ├── src/evaluate.rs   #   Evaluation function (8 components)
│   │   ├── src/anneal.rs     #   SA + LNS + Tabu Search (full + partial)
│   │   ├── src/solver.rs     #   Parallel restart + solve/solve_partial entry points
│   │   ├── benches/plan.rs   #   Criterion benchmark (25 tasks)
│   │   └── examples/daily.rs #   Example: 1-day schedule with 9 tasks
│   ├── takusu-serve/         # REST API server (axum + SQLite)
│   │   ├── SPEC.md           #   API仕様書
│   │   ├── src/
│   │   │   ├── main.rs       #   Entry point, server startup
│   │   │   ├── app.rs        #   Router & AppState definition
│   │   │   ├── auth.rs       #   Bearer token auth middleware + SHA-256 hashing
│   │   │   ├── db.rs         #   SQLite pool init & migrations
│   │   │   ├── error.rs      #   AppError enum (NotFound/BadRequest/Unauthorized/Conflict/Internal)
│   │   │   ├── model.rs      #   DB row structs & request/response types
│   │   │   └── handler/
│   │   │       ├── task.rs   #   Task CRUD + iCal import
│   │   │       ├── habit.rs  #   Habit CRUD
│   │   │       ├── schedule.rs # Schedule generate/reschedule/move/clear
│   │   │       ├── settings.rs # Settings CRUD (tz, sleep_start, sleep_end)
│   │   │       ├── sync.rs   #   Google Calendar sync settings/OAuth/trigger
│   │   │       └── token.rs  #   Token issue/list/revoke
│   │   ├── migrations/
│   │   │   ├── 001_init.sql        # DB schema
│   │   │   ├── 002_google_cal.sql  # Google Calendar settings & event mappings
│   │   │   └── 003_settings.sql   # User settings (tz, sleep_start, sleep_end)
│   │   └── tests/integration.rs     # 24 integration tests (axum oneshot)
│   ├── takusu-ical/          # iCalendar parser (pure, no HTTP dependency)
│   │   └── src/lib.rs        #   parse_ical() → Vec<IcalTask>
│   ├── takusu-audio/         # Audio processing (recording + STT/TTS backends)
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── record.rs     #   Microphone recording (cpal)
│   │       ├── funasr.rs    #   FunASR WebSocket client (SenseVoice backend)
│   │       └── tts.rs       #   TTS client (Irodori-TTS)
│   ├── takusu-audio-cli/     # CLI for audio recording, transcription, and TTS
│   │   └── src/main.rs      #   STT via FunASR, TTS via Irodori-TTS
│   ├── funasr_server/        # Python WebSocket server for FunASR STT
│   │   ├── pyproject.toml
│   │   └── src/funasr_server/
│   │       ├── __init__.py
│   │       ├── __main__.py
│   │       ├── config.py    #   Server configuration
│   │       └── server.py    #   WebSocket server (SenseVoice-Small model)
│   ├── takusu-client/         # HTTP client library for takusu-serve API
│   │   └── src/lib.rs         #   Client, all request/response types
│   ├── takusu-cli/            # CLI client (clap derive, editor-based task editing)
│   │   └── src/
│   │       ├── main.rs        #   CLI entry point + subcommand routing
│   │       ├── editor.rs      #   $EDITOR-based task editing (format/parse/open)
│   │       ├── display_rich.rs#   Table display (comfy-table + colored)
│   │       └── display_simple.rs # Plain-text display
│   └── google-cal/            # Google Calendar API client (reqwest + OAuth2)
│       └── src/lib.rs         #   Client, sync(), delete_all(), OAuth2 helpers
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
| `cargo nextest run --workspace` | Run all 86 tests |
| `cargo nextest run -p takusu-core` | Run core planner tests |
| `cargo nextest run -p takusu-serve` | Run integration tests (19) |
| `cargo nextest run -p takusu-ical` | Run iCal parser tests (5) |
| `cargo bench -p takusu-core` | Run benchmark (~148ms for 25 tasks) |
| `cargo run --example daily` | Run daily schedule example |
| `cargo run -p takusu-cli -- --help` | Run CLI client |
| `cd funasr_server && uv run python -m funasr_server` | Start FunASR STT server |
| `cd funasr_server && ruff check src/` | Lint Python code |
| `cd funasr_server && ruff format --check src/` | Check Python formatting |
| `cargo run -p takusu-audio-cli -- speak --text "..."` | Synthesize speech with Irodori-TTS |
| `./scripts/irodori-tts-server.sh` | Start Irodori-TTS inference server (clones to `$XDG_CACHE_HOME`) |
| `nix run .#irodori-tts-server` | Same as above, via Nix |

## Workspace Dependencies

| Crate | Version | Used by | Notes |
|-------|---------|---------|-------|
| `thiserror` | 2.0 | workspace | Error derive macro |
| `jiff` | 0.2.21 | takusu-core, takusu-serve | Date/time handling |
| `rand` | 0.8 | takusu-core | RNG for SA |
| `rustc-hash` | 2.1 | takusu-core | `FxHashSet` (faster than std) |
| `rayon` | 1.10 | takusu-core | Parallel SA restarts |
| `criterion` | 0.5 | takusu-core (dev) | Benchmarking |
| `tokio` | 1.52.0 | workspace | Async runtime (full features) |
| `axum` | 0.8 | takusu-serve | HTTP framework |
| `sqlx` | 0.8 (sqlite) | takusu-serve | SQLite async driver |
| `serde` / `serde_json` | 1 / 1 | takusu-serve, takusu-ical | Serialization |
| `uuid` | 1 (v7) | takusu-serve | ID generation |
| `sha2` | 0.10 | takusu-serve | Token hashing |
| `tower-http` | 0.6 (cors,trace) | takusu-serve | HTTP middleware |
| `tracing` / `tracing-subscriber` | 0.1 / 0.3 | takusu-serve | Logging |
| `async-trait` | 0.1 | takusu-serve | Async trait |
| `reqwest` | 0.13 (rustls) | google-cal, takusu-serve, takusu-client, takusu-audio | HTTP client |
| `clap` | 4 (derive,env) | takusu-cli | CLI argument parsing |
| `comfy-table` | 7 | takusu-cli | Rich table display |
| `jiff` | 0.2.21 | takusu-core, takusu-serve, takusu-cli | Date/time handling |
| `serde` / `serde_json` | 1 / 1 | takusu-serve, takusu-ical, takusu-client | Serialization |
| `cpal` | 0.17.3 | takusu-audio | Audio input |

| `tokio-tungstenite` | 0.26 | takusu-audio | WebSocket client (FunASR) |
| `futures-util` | 0.3 | takusu-audio | Async stream utilities |

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
- **Partial rescheduling**: `plan_partial(pinned)` pins tasks, optimizes the rest.
  `plan_in_range(range, schedule, extra_pinned)` identifies out-of-range tasks
  from current schedule and reschedules only within-range tasks.

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
- `SleepConfig::from_local(per, tz, start_h, start_m, end_h, end_m)`: converts
  local clock times to slot-based `SleepConfig`, computing `day_start` from timezone
  offset. Used by `parse_sleep` in the server to make "recommended" timezone-aware.

## Text-to-Speech (takusu-audio)

### Backend

- **Irodori-TTS**: OpenAI-compatible `POST /v1/audio/speech`. Reference voices are loaded from `IRODORI_VOICES_DIR` (default `./refs`). Uses the base `Aratako/Irodori-TTS-500M-v3` model; VoiceDesign/caption control is not exposed.

### Client API

- `TtsBackend::Irodori`
- `TtsClient::new(config)` + `synthesize(request)` returns `Vec<u8>` audio bytes
- `pick_reference_voice(refs_dir)` selects the first audio file under `./refs/`

### CLI

```sh
cargo run -p takusu-audio-cli -- speak --text "こんにちは"
```

Default reference audio directory is `./refs/`. Place a WAV/MP3/FLAC/etc. file there and the CLI auto-picks it; use `--reference` to override.

### TTS server

- `scripts/irodori-tts-server.sh` — clones `Aratako/Irodori-TTS-Server` to `$XDG_CACHE_HOME/takusu/irodori-tts-server` and runs it via `uv run --extra cpu --python 3.11`.
- Requires `git` and `uv` on `PATH`.
- `nix run .#irodori-tts-server` provides the same script with `git`, `uv`, and `ffmpeg` bundled.
- `IRODORI_VOICES_DIR` defaults to `./refs` for Irodori-TTS.

## takusu-serve API

### Authentication

- Root token: `TAKUSU_ROOT_TOKEN` env var (format: `tsk_` + UUID v7)
- Issued tokens: stored as SHA-256 hash in `tokens` table
- Any valid token can issue new tokens (trust chain)
- Revocation is per-token, no cascade
- All `/api/*` endpoints require `Authorization: Bearer <token>`
- `/health` requires no auth

### Endpoints

See `SPEC.md` for full API specification. Summary:

- **Task**: CRUD + iCal import (`/api/tasks`, `/api/tasks/import/ical`)
- **Habit**: CRUD (`/api/habits`)
- **Schedule**: get/generate/reschedule/move/clear (`/api/schedule/*`)
- **Settings**: get/update (`/api/settings`) — tz, sleep_start, sleep_end
- **Token**: issue/list/revoke (`/api/tokens`)
- **Sync**: Google Calendar settings/OAuth/trigger (`/api/sync/*`)

### Testing

Integration tests use `axum::Router::oneshot()` with in-memory SQLite.
No external HTTP server needed. Run with `cargo nextest run -p takusu-serve`.

### Key Architecture Decisions

- **Single active schedule**: `schedules` table has one row (`id = 'active'`), UPSERT on generate
- **Task CRUD does not auto-reschedule**: responses include `unscheduled_count`
- **Move entry with validation**: `PATCH /api/schedule/entries/:task_id` returns 409
  with warnings on violations; `force: true` overrides
- **iCal import skips duplicates**: by `ical_uid` column (unique index)
- **Token hashing**: tokens stored as SHA-256, full token only returned on creation
- **Google Calendar sync is fire-and-forget**: schedule generate/reschedule/move/clear
  triggers background sync via `tokio::spawn`. Response is returned before sync completes.
  Sync uses a `tokio::sync::Mutex` lock to prevent concurrent runs. Failed syncs retry
  up to 3 times with exponential backoff (5s, 10s, 20s).
- **google-cal crate**: standalone Google Calendar API client. `Client::sync()` does
  diff-based sync (create/update/delete against existing mappings). `Client::delete_all()`
  removes all events. OAuth2 flow: `oauth_url()` → user authorize → `exchange_code()`.
- **AppState has sync_lock**: `Arc<Mutex<()>>` serialized via the lock inside the retry loop
  (lock acquired per attempt, released before sleep).
- **Generate uses `now` as start**: `POST /api/schedule/generate` no longer accepts `from`;
  the start time is always the current time. Only `until` is required.
- **Task status tracking**: tasks have a `status` column with 5 states:
  `pending` (not yet scheduled), `scheduled` (in current schedule), `in_progress` (being worked on),
  `completed` (done), `skipped` (explicitly skipped). Status is changeable via `task status <id> <value>`
  or `task update --status <value>`.
  - **Generate includes pending/scheduled only**: query is `status IN ('pending', 'scheduled')`.
    `in_progress`, `completed`, and `skipped` tasks are excluded from schedule generation.
  - **Generate sets scheduled**: all tasks included in a generate become `status='scheduled'`.
  - **Reschedule**: queries tasks with `status IN ('pending', 'scheduled')`.
  - **Clear schedule**: does NOT reset task status (tasks stay `scheduled`; must be manually set to `pending`).

## Key Design Decisions (from main.typ)

- **Planner**: Uses heuristic algorithms (simulated annealing) with an evaluation
  function, not exact SAT solving despite z3 being in the dev shell.
  Tasks are discretized into 5-minute slots.
- **Voice Assistant**: Android `VoiceInteractionService` + server for FunASR/LLM
  processing. FunASR (SenseVoice-Small) provides fast, accurate Japanese STT via
  WebSocket (~0.35s for 6s audio on CPU). Whisper.cpp available as offline fallback.
  LLM fills in missing information (estimates, etc.) using memory of past similar tasks.
- **Task model**: Includes start time, deadline, cost estimate (normal distribution),
  dependencies, parallelizability, and `abandonability` (deadline flexibility).
- **No existing README** — the design document (`main.typ`) and this file serve
  as the primary project documentation.

## takusu-cli

CLI client using clap derive with nested subcommands: `task`, `schedule`, `token`, `sync`.

- **Display modes**: `--mode rich` (comfy-table) / `--mode simple` (plain text)
- **Auth**: `--token` flag or `TAKUSU_TOKEN` env var
- **Status display**: colored in rich mode (Yellow=pending, Green=scheduled, DarkYellow=in_progress,
  DarkCyan=completed, DarkGrey=skipped); simple mode uses markers ([ ], [~], [>], [x], [-])
- **Status update**: `task update --status <value>` or `task edit` includes status field
- **Task list filter**: `task list --status <value>
- **Editor-based editing**: `task edit <ID>` writes task fields to a temp file,
  opens `$EDITOR` (default `vi`), then parses the saved file and sends PATCH.
  Lines starting with `#` are comments. Empty values are not updated.
- **Subcommands**: `task {list,show,create,edit,update,replace,delete,status}`,
  `schedule {get,generate,reschedule,move,clear}`,
  `token {create,list,revoke}`, `sync {settings,setup,oauth-url,oauth-callback,trigger}`

## takusu-client

Standalone HTTP client library for takusu-serve. Reused by takusu-cli and any future client (Android Kotlin, etc.).

- Types mirror server `model.rs` request/response structs (`TaskRow`, `CreateTask`, `UpdateTask`, etc.)
- `Client` struct holds `base_url` + `token`, all methods are async
- Error type: `ClientError { Http, Api { status, body } }` — no `thiserror` dependency

## Code Style

- No comments by default (existing code has minimal comments)
- Uses `thiserror` for error types
- Module-level docs (`//!`) in each source file describe algorithm details
- Modules organized by domain (core, serve, audio, google-cal)
- Workspace-level dependency versions defined in root `Cargo.toml`
- `FxHashSet` over `HashSet` for performance-critical collections
- Edition 2024: `gen` is a reserved keyword → use `r#gen` for rand trait methods
- **reqwest**: Use `rustls-tls` feature (not default native-tls) to avoid OpenSSL dependency

## Benchmark

```sh
cargo bench -p takusu-core
# plan/plan 25 tasks: ~148 ms (4 parallel SA chains)
```

25 tasks with random dependencies, σ values, and deadlines.
Run on debug with `cargo run --example daily` for a human-readable 9-task example.