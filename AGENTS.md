# AGENTS.md

## Project Overview

takusu is a planner that automatically builds user schedules and a voice assistant using LLM as the UI. The design document is `main.typ` (in Japanese).

- **License**: MIT
- **Repository**: https://github.com/satler-git/takusu

## Tech Stack

- **Language**: Rust (edition 2024, stable toolchain)
- **Python**: FunASR server for STT (SenseVoice-Small model, managed via `uv`)
- **Kotlin**: Planned for Android app
- **Version Control**: Jujutsu (`jj`) + Git (GitHub) — **Jujutsu is the preferred VCS in this workspace.** See [Version Control Workflow](#version-control-workflow) below.
- **Nix**: `flake.nix` provides the dev shell (direnv with `use flake`)

## Agent Notifications

When a task is finished or when you have a question for the user, send a desktop notification via `dunstify` so the user is alerted even when not watching the terminal.

```sh
dunstify "takusu agent" "task finished: <short summary>"
dunstify "takusu agent" "question: <short question>"
```

- Fire the notification **once** at the very end of the task, or when you need user input to proceed.
- Keep the body short (one line). Do not dump long output into the notification.
- Use a meaningful summary: what was done, or what you need from the user.

## Project Structure

```
takusu/
├── main.typ                  # 設計ドキュメント (Typst)
├── Cargo.toml                # Rust workspace root
├── crates/
│   ├── takusu-core/          # Core planner (data types, scheduling algorithm)
│   │   ├── src/lib.rs        #   Public API (Point, Task, NormalDist, Planner, Plan, RescheduleRange)
│   │   ├── src/evaluate.rs   #   Evaluation function (10 components)
│   │   ├── src/anneal.rs     #   SA + LNS + Tabu Search (full + partial)
│   │   ├── src/solver.rs     #   Parallel restart + solve/solve_partial entry points
│   │   ├── benches/plan.rs   #   Criterion benchmark (25 tasks)
│   │   └── examples/daily.rs #   Example: 1-day schedule with 9 tasks
│   ├── takusu-local/         # Local server (axum + SQLite, uses takusu-local-lib)
│   ├── takusu-local-lib/     # Business logic library (shared by takusu-local and takusu-cli)
│   │   ├── src/
│   │   │   ├── app.rs        #   TakusuApp: all business logic + two storage backends
│   │   │   ├── config.rs     #   LocalConfig (env-based, TAKUSU_* prefix)
│   │   │   ├── error.rs      #   AppError enum
│   │   │   ├── storage_sqlite.rs  # SqliteStorage (direct sqlx)
│   │   │   ├── storage_workers.rs # WorkersStorage (HTTP → Cloudflare Worker)
│   │   │   ├── token_cache.rs     # TTL-based token verification cache
│   │   │   └── auth.rs       #   SHA-256 token hashing
│   │   └── migrations/
│   │       ├── 001_init.sql        # DB schema
│   │       ├── 002_google_cal.sql  # Google Calendar settings & event mappings
│   │       ├── 003_settings.sql   # User settings (tz, sleep_start, sleep_end)
│   │       └── 004_indexes.sql
│   ├── takusu-storage/       # Pluggable storage trait + shared types
│   │   └── src/
│   │       ├── storage.rs    #   Async Storage trait
│   │       ├── model.rs      #   Shared types (TaskRow, HabitRow, ScheduleEntry, etc.)
│   │       └── error.rs      #   StorageError enum
│   ├── takusu-ical/          # iCalendar parser (pure, no HTTP dependency)
│   │   └── src/lib.rs        #   parse_ical() → Vec<IcalTask>
│   ├── takusu-habit/         # Recurrence rule engine (RRULE expansion)
│   │   └── src/lib.rs
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
│   │       ├── config.py     #   Server configuration
│   │       └── server.py     #   WebSocket server (SenseVoice-Small model)
│   ├── takusu-client/         # HTTP client library for takusu REST API
│   │   └── src/lib.rs         #   Client, all request/response types
│   ├── takusu-cli/            # CLI client (clap derive, editor-based task editing)
│   │   └── src/
│   │       ├── main.rs        #   CLI entry point + subcommand routing
│   │       ├── editor.rs      #   $EDITOR-based task editing (format/parse/open)
│   │       ├── display_rich.rs#   Table display (comfy-table + colored)
│   │       └── display_simple.rs # Plain-text display
│   ├── takusu-worker/         # Cloudflare Worker (Rust/WASM + D1)
│   │   └── src/
│   ├── takusu-util/           # Shared utilities (duration parsing, datetime parsing, token generation)
│   │   └── src/lib.rs
│   └── google-cal/            # Google Calendar API client (reqwest + OAuth2)
│       └── src/lib.rs         #   Client, sync(), delete_all(), OAuth2 helpers
├── flake.nix                 # Nix development environment
├── rust-toolchain.toml       # Rust toolchain config
├── .envrc                    # direnv config
├── .devin/skills/            # Devin CLI skills (thin wrappers around scripts/)
│   ├── issue-view/SKILL.md
│   ├── issue-assign/SKILL.md
│   ├── pr-watch/SKILL.md
│   ├── jj-resolve/SKILL.md
│   └── discord-notify/SKILL.md
└── scripts/                  # Agent + user helper scripts
    ├── issue-view.sh         # GitHub issue list/show (label/assignee/state filters)
    ├── issue-assign.sh       # Assign an unassigned issue to the current user
    ├── pr-watch.sh           # PR CI/review/comment snapshot + polling watch loop
    ├── jj-resolve.sh         # Jujutsu conflict list/show/edit/mark/merge/abort
    └── discord-notify.sh     # Discord webhook sender (DISCORD_WEBHOOK_URL env)
```

## Development Environment

Use `nix develop` or `direnv allow` to enter the development shell. The flake provides:
- `rust-bin` (from `rust-toolchain.toml`: stable + rustfmt + clippy)
- `cargo-expand`, `cargo-nextest`

### Key Commands

| Command | Description |
|---------|-------------|
| `cargo check` | Type-check all crates |
| `cargo fmt` / `treefmt` | Format code |
| `cargo clippy` | Lint |
| `cargo nextest run --workspace` | Run all tests (~171 across crates) |
| `cargo nextest run -p takusu-core` | Run core planner tests (45) |
| `cargo nextest run -p takusu-local` | Run local server integration tests (39) |
| `cargo nextest run -p takusu-ical` | Run iCal parser tests (15) |
| `cargo bench -p takusu-core` | Run benchmark (~148ms for 25 tasks) |
| `cargo test -p takusu-worker` | Run takusu-worker unit tests (6 auth tests) |
| `cargo test -p takusu-worker --test auth -- --ignored` | Run takusu-worker auth integration tests (requires `wrangler`) |
| `cargo run --example daily` | Run daily schedule example |
| `cargo run -p takusu-cli -- --help` | Run CLI client |
| `cargo run -p takusu-local` | Start local server |
| `cd funasr_server && uv run python -m funasr_server` | Start FunASR STT server |
| `cd funasr_server && ruff check src/` | Lint Python code |
| `cd funasr_server && ruff format --check src/` | Check Python formatting |
| `cargo run -p takusu-audio-cli -- speak --text "..."` | Synthesize speech with Irodori-TTS |
| `./scripts/irodori-tts-server.sh` | Start Irodori-TTS inference server (clones to `$XDG_CACHE_HOME`) |
| `nix run .#irodori-tts-server` | Same as above, via Nix |
| `./scripts/issue-view.sh list [--label L] [--assignee A] [--state S]` | List GitHub issues (TSV: number, title, labels, assignees, state) |
| `./scripts/issue-view.sh show <N>` | Show issue title, body, labels, assignees, and full comment thread |
| `./scripts/issue-assign.sh <N> [<N>...] [--assignee <user>]` | Assign an unassigned issue to the current user (or another user) |
| `./scripts/pr-watch.sh show <PR>` | One-shot snapshot of PR CI checks, reviews, and comments |
| `./scripts/pr-watch.sh watch <PR> [--interval N] [--max N]` | Poll a PR and print diffs to stdout (no notification); default 60s |
| `./scripts/jj-resolve.sh list\|status\|show\|edit\|mark\|merge\|abort` | Inspect and resolve Jujutsu merge conflicts |
| `./scripts/discord-notify.sh "text" \| --title T --desc D` | Send a message/embed to Discord (`$DISCORD_WEBHOOK_URL`) |

## Version Control Workflow

This workspace uses **Jujutsu (`jj`) as the primary VCS**, backed by a Git remote on GitHub. Prefer `jj` commands over raw `git` for everyday work.

### Why Jujutsu

- Commits are first-class and amendable without history-rewriting ceremony.
- Working copy is always a commit (`@`); edits become a commit automatically.
- Rebases, splits, and squashes are cheap and safe.

### Common Commands

| Command | Purpose |
|---------|---------|
| `jj st` | Show working copy status |
| `jj log -r 'main..@'` | Show commits ahead of main |
| `jj new` | Create a new empty change on top of `@` |
| `jj squash` | Squash `@` into its parent |
| `jj amend` | Amend `@` with working copy changes (default behavior, but explicit form useful) |
| `jj rebase -r <rev> -d <dest>` | Rebase a change onto another |
| `jj split <rev>` | Split a change into two interactively |
| `jj describe` | Edit the description of `@` |
| `jj git push` | Push to the Git remote (GitHub) |
| `jj git fetch` | Fetch from the Git remote |

### Basic Workflow

The default loop for any task is:

1. Do the work (explore, edit, run `cargo check` / `cargo nextest run` / `cargo clippy` as needed).
2. `jj describe` to write a commit message for `@` (present tense, lowercase first word, no trailing period).
3. `jj git push --change` to push the current change to GitHub (creates/updates a branch named after the change-id).
4. `gh pr create` (or `gh pr edit` if the PR already exists) to open or update the pull request.

**Agents do steps 2–4 themselves**: after finishing the work, the agent writes the commit message, pushes the change, and creates/updates the PR without asking the user. Repeat per change. Use `jj new` to start a fresh change on top of `@`, and `jj squash` / `jj amend` to consolidate work before pushing. Rebase onto `main` with `jj git fetch && jj rebase -r @ -d main` before pushing if `main` has moved.

### Conventions

- **Commit messages**: write in the present tense, lowercase first word, no trailing period. Match the style of recent history (`iroiro fix`, `chore: fmt`, `separate takusu-local`).
- **Pushing**: use `jj git push --change` to push a single change as a reviewable branch (this is the default for PR work). Plain `jj git push` pushes all bookmarks; prefer `--change` for feature work.
- **Branches**: this repo uses a single `main` bookmark; feature work happens in separate change-ids and is rebased onto `main` before push.
- **Do not rewrite `main`**: never force-push or rebase `main` itself. Rebase your own changes onto `main` instead.
- **Git compatibility**: `git` commands still work for read-only inspection (`git log`, `git diff`) since `.jj` backs onto `.git`. Prefer `jj` for anything that mutates history.
- **Issue closing**: link issues to PRs using `Closes #N` lines in the PR body so GitHub auto-closes them on merge. Do **not** post "Fixed in #N" comments on the issues themselves.

## Agent Helpers

Five shell scripts in `scripts/` wrap common agent workflows. Each has a
matching thin Devin skill in `.devin/skills/<name>/SKILL.md` so the agent
can invoke them via `/issue-view`, `/issue-assign`, `/pr-watch`, `/jj-resolve`,
or `/discord-notify`. **Prefer the scripts over raw `gh`/`jj`/`curl`** — they
produce stable, agent-friendly output and centralize the flag spelling.

### `issue-view.sh` — GitHub issue viewer

Wraps `gh issue list` / `gh issue view`. Plain-text output (TSV for `list`,
markdown for `show`) so the agent can parse it without TTY-dependent color
codes.

```sh
./scripts/issue-view.sh list [--label <label>] [--assignee <user|@me|unassigned>] \
                             [--state <open|closed|all>] [--limit <N>] [--search <query>]
./scripts/issue-view.sh show <number>
```

- `list` output: `number\ttitle\tlabels\tassignees\tstate` (one issue per line).
- `show` output: title, labels, assignees, body, then the full comment thread.
- Use `--assignee unassigned` to find untriaged issues.

### `issue-assign.sh` — GitHub issue self-assignment

Assigns an issue to the current user (or another user) only if it has zero
assignees. Safe for agents to call before starting work.

```sh
./scripts/issue-assign.sh <number> [<number>...] [--assignee <user>]
```

- Output (non-TTY): `number\tassignee(s)\tstatus`, where status is `assigned` or `already-assigned`.
- No-op when the issue already has an assignee.

### `pr-watch.sh` — PR CI/review/comment watcher

Wraps `gh pr view --json ...` and presents a stable snapshot of CI checks,
reviews, and comments. Two modes:

```sh
./scripts/pr-watch.sh show <PR>                       # one-shot snapshot
./scripts/pr-watch.sh watch <PR> [--interval 60] [--max 0]  # polling loop
```

- `show` prints the full snapshot once.
- `watch` loops, printing only sections that changed since the last snapshot
  (`--- <section> changed ---` with `<<< before` / `>>> after`). Default
  interval 60s; `--max 0` = unlimited.
- **No desktop/Discord notification** — output goes to stdout only. The agent
  reads stdout and decides what to do (e.g. reply to a review comment, or
  report CI failure to the user).
- Run `watch` in a background shell (`run_in_background: true`) and poll with
  `get_output` to integrate with the agent loop.

### `jj-resolve.sh` — Jujutsu conflict resolver

Wraps `jj resolve --list` and friends. Use after any `jj rebase` / `jj merge`
/ `jj git fetch` that might conflict.

```sh
./scripts/jj-resolve.sh list          # conflicted file paths (or "no conflicts")
./scripts/jj-resolve.sh status        # "N conflicted file(s)"
./scripts/jj-resolve.sh show [<file>] # conflict marker line numbers
./scripts/jj-resolve.sh edit <file>   # open $EDITOR on the file
./scripts/jj-resolve.sh mark <file>   # verify the file is resolved (no markers + not in jj resolve --list)
./scripts/jj-resolve.sh merge <file>  # launch a 3-way merge tool via jj resolve
./scripts/jj-resolve.sh abort         # print recovery guidance
```

The standard agent workflow is: `list` → `show <file>` → read the file →
`edit` (or use the `edit` tool) to remove conflict markers → `mark <file>`
to verify → repeat until `status` reports 0. jj has no explicit "mark
resolved" command — a file is considered resolved once all conflict markers
are removed, so `mark` just verifies that condition. Use `merge` if you
prefer a 3-way merge tool over manual marker editing.

### `discord-notify.sh` — Discord webhook sender

Sends a message or embed to Discord via webhook. The URL is read from
`$DISCORD_WEBHOOK_URL` (set in `.envrc` or shell config); the script never
prints it.

```sh
./scripts/discord-notify.sh "plain text"
./scripts/discord-notify.sh --title "T" --desc "D" [--color 0xRRGGBB|#RRGGBB|RRGGBB|decimal] [--url <link>]
./scripts/discord-notify.sh --json <payload.json>
echo '{"content":"hi"}' | ./scripts/discord-notify.sh --stdin
```

- `--color` accepts `0xRRGGBB`, `#RRGGBB`, `RRGGBB`, or a decimal integer.
- `--quiet` suppresses the `discord: sent` confirmation.
- This is **separate from the `dunstify` desktop notifications** in
  "Agent Notifications" above — `dunstify` is for local terminal alerts,
  `discord-notify.sh` is for off-terminal pings.

### Skill invocation

Each helper has a Devin skill in `.devin/skills/<name>/SKILL.md` that
documents the script and tells the agent when to use it. Skills are picked
up at session start; restart the session after adding a new one. The skill
files are thin (just documentation + `allowed-tools: [exec, read]`) — the
real logic lives in the shell scripts so it can be used outside Devin too.

## Workspace Dependencies

| Crate | Version | Used by | Notes |
|-------|---------|---------|-------|
| `thiserror` | 2.0 | workspace | Error derive macro |
| `jiff` | 0.2.21 | takusu-core, takusu-local, takusu-cli | Date/time handling |
| `rand` | 0.8 | takusu-core | RNG for SA |
| `rustc-hash` | 2.1 | takusu-core | `FxHashSet` (faster than std) |
| `rayon` | 1.10 | takusu-core | Parallel SA restarts |
| `criterion` | 0.5 | takusu-core (dev) | Benchmarking |
| `tokio` | 1.52.0 | workspace | Async runtime (full features) |
| `axum` | 0.8 | takusu-local | HTTP framework |
| `sqlx` | 0.8 (sqlite) | takusu-local, takusu-local-lib | SQLite async driver |
| `serde` / `serde_json` | 1 / 1 | takusu-local, takusu-ical, takusu-client | Serialization |
| `uuid` | 1 (v7) | takusu-local, takusu-local-lib | ID generation |
| `sha2` | 0.10 | takusu-local-lib | Token hashing |
| `tower-http` | 0.6 (cors,trace) | takusu-local | HTTP middleware |
| `tracing` / `tracing-subscriber` | 0.1 / 0.3 | takusu-local, takusu-local-lib | Logging |
| `async-trait` | 0.1 | takusu-local-lib | Async trait |
| `reqwest` | 0.13 (rustls) | google-cal, takusu-local-lib, takusu-client, takusu-audio | HTTP client |
| `clap` | 4 (derive,env) | takusu-cli | CLI argument parsing |
| `comfy-table` | 7 | takusu-cli | Rich table display |
| `cpal` | 0.18.1 | takusu-audio | Audio input |
| `tokio-tungstenite` | 0.29 | takusu-audio | WebSocket client (FunASR) |
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
| parallel_violation | `−overlap_slots × W_PARALLEL_VIOL` | 500.0 | Proportional to illegal overlap (near-hard constraint) |
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

## takusu-local API

### Authentication

- Root token: `TAKUSU_ROOT_TOKEN` env var (format: `tsk_` + UUID v7)
- Issued tokens: stored as SHA-256 hash in `tokens` table
- Any valid token can issue new tokens (trust chain)
- Revocation is per-token, no cascade
- All `/api/*` endpoints require `Authorization: Bearer <token>`
- `/health` requires no auth

### Endpoints

- **Task**: CRUD + iCal import (`/api/tasks`, `/api/tasks/import/ical`)
- **Habit**: CRUD (`/api/habits`)
- **Schedule**: get/generate/reschedule/move/clear (`/api/schedule/*`)
- **Settings**: get/update (`/api/settings`) — tz, sleep_start, sleep_end
- **Token**: issue/list/revoke (`/api/tokens`)
- **Sync**: Google Calendar settings/OAuth/trigger (`/api/sync/*`)

### Testing

Integration tests use `axum::Router::oneshot()` with in-memory SQLite.
No external HTTP server needed. Run with `cargo nextest run -p takusu-local`.

### Key Architecture Decisions

- **takusu-local-lib** is the core business logic, used by both `takusu-local` (server) and `takusu-cli` (client).
- **Pluggable storage**: `takusu-storage` provides the `Storage` trait. Two implementations: `SqliteStorage` (direct sqlx) and `WorkersStorage` (HTTP → Cloudflare Worker).
- **CLI uses takusu-local-lib directly**: No network round-trip; `takusu-cli` initializes `TakusuApp` with a storage backend (`TAKUSU_STORAGE=sqlite|workers`).
- **Single active schedule**: `schedules` table has one row (`id = 'active'`), UPSERT on generate
- **Task CRUD does not auto-reschedule**: responses include `unscheduled_count`
- **Move entry with validation**: `PATCH /api/schedule/entries/:task_id` returns 409
  with warnings on violations; `force: true` overrides
- **iCal import skips duplicates**: by `ical_uid` column (unique index)
- **Token hashing**: tokens stored as SHA-256, full token only returned on creation
- **Google Calendar sync**: schedule generate/reschedule/move/clear triggers sync
  inline (no fire-and-forget). `google-cal` crate does diff-based sync.
- **Generate uses `now` as start**: `POST /api/schedule/generate` no longer accepts `from`
  or `until`; the start time is always the current time and the horizon is derived from
  task deadlines. The planner schedules all eligible tasks regardless of an upper bound.
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
  function, not exact SAT solving. Tasks are discretized into 5-minute slots.
- **Voice Assistant**: Android `VoiceInteractionService` + server for FunASR/LLM
  processing. FunASR (SenseVoice-Small) provides fast, accurate Japanese STT via
  WebSocket (~0.35s for 6s audio on CPU).
  LLM fills in missing information (estimates, etc.) using memory of past similar tasks.
- **Task model**: Includes start time, deadline, cost estimate (normal distribution),
  dependencies, parallelizability, and `abandonability` (deadline flexibility).
- **Documentation**: `README.md` (overview), `ARCHITECTURE.md` (structure),
  the design document (`main.typ`), and this file serve as the primary
  project documentation.

## takusu-cli

CLI client using clap derive with nested subcommands: `task`, `schedule`, `token`, `sync`.

- **Uses takusu-local-lib directly**: no network round-trip
- **Storage backends**: `TAKUSU_STORAGE=sqlite` (default) or `TAKUSU_STORAGE=workers`
- **Display modes**: `--mode rich` (comfy-table) / `--mode simple` (plain text)
- **Status display**: colored in rich mode (Yellow=pending, Green=scheduled, DarkYellow=in_progress,
  DarkCyan=completed, DarkGrey=skipped); simple mode uses markers ([ ], [~], [>], [x], [-])
- **Status update**: `task update --status <value>` or `task edit` includes status field
- **Task list filter**: `task list --status <value>`
- **Editor-based editing**: `task edit <ID>` writes task fields to a temp file,
  opens `$EDITOR` (default `vi`), then parses the saved file and sends PATCH.
  Lines starting with `#` are comments. Empty values are not updated.
- **Subcommands**: `task {list,show,create,edit,update,replace,delete,status}`,
  `schedule {get,generate,reschedule,move,clear}`,
  `token {create,list,revoke}`, `sync {settings,setup,oauth-url,oauth-callback,trigger}`,
  `habit {list,show,create,edit,update,replace,delete}`

## takusu-client

Standalone HTTP client library for the takusu REST API. Reused by any future client (Android Kotlin, etc.).

- Types mirror `takusu-storage` model.rs request/response structs (`TaskRow`, `CreateTask`, `UpdateTask`, etc.)
- `Client` struct holds `base_url` + `token`, all methods are async
- Error type: `ClientError { Http, Api { status, body } }` — no `thiserror` dependency

## Code Style

- **Add a comment whenever the reason for writing code a certain way is non-obvious.** If a future reader might ask "why is this done this way?", add a comment explaining the rationale. This is especially important for performance optimizations, workarounds for external library quirks, safety invariants that aren't type-checked, and cases where the seemingly "cleaner" approach would be wrong.
- Uses `thiserror` for error types
- Module-level docs (`//!`) in each source file describe algorithm details
- Modules organized by domain (core, local, audio, google-cal)
- Workspace-level dependency versions defined in root `Cargo.toml`
- `FxHashSet` over `HashSet` for performance-critical collections
- Edition 2024: `gen` is a reserved keyword → use `r#gen` for rand trait methods
- **reqwest**: Use `rustls-tls` feature (not default native-tls) to avoid OpenSSL dependency

## Hacks / Brittle Code — Do Not Remove Casually

These patterns look suspicious but exist for real reasons. If you need to change them, understand the context first:

### `sqlx::AssertSqlSafe` in dynamic SQL
**Files:** `takusu-local/src/handlers/task.rs`, `schedule.rs`, `sync.rs`; `takusu-local-lib/src/storage_sqlite.rs`, etc.

Dynamic SQL with parameterized `?` placeholders suppresses sqlx's compile-time verification. Safe today because all user values go through `?` bindings, but removes sqlx's guard against future accidental string interpolation. If refactoring, replace with `sqlx::query_builder` or array binding.

### `TAKUSU_WORKERS_URL` `|` split hack
**File:** `takusu-local/src/main.rs`

```rust
cfg.workers_url().split('|').next()
```

The config crate's env separator collides with `TAKUSU_WORKERS_URL` containing `://`. The `|` split is a fragile workaround. The second segment (after `|`) is unused/undocumented.

### ~~Fire-and-forget Google Calendar sync~~ FIXED
**Files:** `takusu-local/src/handlers/schedule.rs`, `sync.rs`; `takusu-local-lib/src/app.rs`

`tokio::spawn` was replaced with awaited `do_sync()` calls. Sync now runs inline during the request.

### `COALESCE` prevents clearing fields to NULL
**File:** `takusu-worker/src/handlers/tasks.rs`, `habits.rs`

```sql
UPDATE tasks SET title=COALESCE(?1,title), ...
```

`Option::None` (from `serde_json::Value::Null` → `Option::None`) binds as `JsValue::NULL`, so `COALESCE(NULL, title)` keeps the old value. There is no way to clear a field. Fixing this requires distinguishing "not provided" from "explicitly set to null".

### `LIKE` prefix matching for short IDs
**Files:** `takusu-local/src/handlers/task.rs`, `habit.rs`; `takusu-local-lib/src/storage_sqlite.rs`

```sql
SELECT id FROM tasks WHERE id LIKE ? || '%'
```

Forces a full table scan and is vulnerable to `_`/`%` pattern injection. The entire short-ID UX depends on this pattern.

### `point_to_iso` hardcoded 5-minute slots
**Files:** `takusu-local-lib/src/app.rs` and ~8 other locations

Magic number `5` (slot length in minutes) is duplicated across crates with no shared constant. Changing slot granularity requires updating every site.

### ~~Duplicated integration test patterns~~ NOTED
**Files:** `takusu-local/tests/integration.rs`, `phase4.rs`, `workers_e2e.rs`

Integration tests share code patterns. Full deduplication into a shared test-utils crate is planned.

### ~~`_unused_jsvalue_marker` dead code~~ REMOVED
**File:** `takusu-worker/src/handlers/tokens.rs`

Removed the dead code function and unused `JsValue` import.

### ~~410 (Gone) treated as success in Google Calendar delete~~ FIXED
**File:** `google-cal/src/lib.rs`

Magic number `410` is now a named constant `ALREADY_DELETED`.

### ~~`get_settings_or_default` swallows DB errors~~ FIXED
**Files:** `takusu-local-lib/src/app.rs`

Now returns `Result<SettingsRow, AppError>`. DB errors propagate correctly; `NotFound` still falls back to defaults.

### ~~Sync `.ok()` silently drops DB errors~~ FIXED
**Files:** `takusu-local-lib/src/app.rs`

All DB operations propagate errors via `?` with `.map_err()`.

### ~~Unsafe `set_len` in audio recording~~ REMOVED
**File:** `takusu-audio/src/record.rs`

Replaced with simple `buf.push()` loop. The unsafe optimization was unnecessary overhead.

### ~~`generate_neighbor_partial` only uses 3 of 5 neighbor types~~ FIXED
**File:** `takusu-core/src/anneal.rs`

Added `neighbor_reorder_partial` and `neighbor_lns_partial` operators. The partial variant now uses the same 5 neighbor types with identical probability distribution as the full variant (shift 25%/swap 25%/duration 20%/reorder 15%/lns 15%).

### ~~Auth middleware not applied to CRUD endpoints~~ FIXED
**File:** `takusu-worker/src/router.rs`

`require_auth()` is now called for every `/api/*` route except `/api/auth/verify` in the `dispatch` function. Tokens (`tasks`, `habits`, `schedule`, `settings`, `sync`, `tokens`) all require `Authorization: Bearer <token>`.

### `freeness()` name is counterintuitive
**File:** `takusu-core/src/lib.rs`

High "freeness" means the task has slack time and is deprioritized. Low freeness → prioritized first. The name suggests the opposite convention.

## Benchmark

```sh
cargo bench -p takusu-core
# plan/plan 25 tasks: ~148 ms (4 parallel SA chains)
```

25 tasks with random dependencies, σ values, and deadlines.
Run on debug with `cargo run --example daily` for a human-readable 9-task example.
