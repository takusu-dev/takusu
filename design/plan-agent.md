# takusu LLM Agent + Memory / Progress Management Plan

## Summary

Implement the voice-assistant half of the takusu vision (`main.typ` §音声アシスタント):
an LLM agent that operates the planner through tool calling, plus the memory, safe change
management, schedule orchestration, and progress features it depends on.

The implementation proceeds sequentially with one agent. Each work item (WI) should leave the
repository working and tested before the next begins. Contracts in this document may be refined
when implementation reveals a problem, but the document and all affected clients must be updated
in the same change.

The LLM-facing tool set is:

- planner reads: task, habit, schedule, and settings lookup;
- planner mutations: task/habit CRUD through auditable change sets;
- schedule preview and commit;
- memory search/save/update/delete and similar-task lookup;
- progress start/pause/report/complete/split;
- skills list/read and confirmed add/edit.

Audio capture and playback are application I/O, not LLM tools. The CLI or future Android/server
adapter records audio, transcribes it, calls the agent for one turn, and speaks the returned text.
The LLM must never activate the microphone by itself.

## Product invariants from `main.typ`

The implementation must preserve these behaviors across all work items:

1. **Explain inference**: when the LLM fills a missing deadline, estimate, sigma, dependency, or
   abandonability, the response and change record say what was inferred and why.
2. **Visible and reversible planner changes**: task/habit changes made by the agent are queryable
   by the app and can be reverted when the target has not subsequently changed.
3. **Confirm risky effects**: deletion, persistent skill edits, and schedule changes that displace
   tasks, leave tasks unscheduled, or reduce sleep require user confirmation before commit.
4. **Batch scheduling work**: consecutive task edits mark the schedule dirty; they do not run the
   planner independently. Recompute after an explicit schedule request, an explicit user request,
   or at the end of a configurable quiet period.
5. **Search before guessing**: for an unknown proper noun, search memory first, ask the user if it
   is still unknown, then save the answer. For a missing estimate, inspect similar completed tasks
   before using model knowledge.
6. **Use active work time**: progress estimates and actual duration use explicit active work
   sessions, not wall-clock time between first start and completion.
7. **Stable references**: user-facing task references use `display_id`. Habit-generated task IDs
   are scoped by habit and must use an unambiguous form such as `h<habit_display_id>#<task_display_id>`.

## Architecture

```text
takusu-agent (library + CLI binary)
├── src/lib.rs        # AgentSession, tool loop, history, system context
├── src/tool.rs       # Tool trait, ToolRegistry, structured tool results
├── src/llm.rs        # OpenAI chat-completions adapter
├── src/tools/
│   ├── takusu.rs     # planner reads, change sets, schedule preview/commit
│   ├── skills.rs     # skills list/read/add/edit with confirmation
│   ├── memory.rs     # memory CRUD and similar tasks
│   └── progress.rs   # work sessions, progress, completion, splitting
├── src/audio.rs      # application-level STT/TTS adapter; not registered as tools
└── src/bin/agent.rs  # text mode and push-to-talk CLI

takusu server/storage/client extensions
├── migrations/014_agent_changes.sql
├── migrations/015_memory.sql
├── migrations/016_progress.sql
├── /api/agent/change-sets/*
├── /api/schedule/preview
├── /api/memory/* and /api/tasks/similar
└── /api/tasks/:id/work/*, /progress, and /split
```

The current latest local migration is `013_habit_task_display_id.sql`; therefore this plan starts
at 014. Both SQLite/local and D1/Worker implementations must expose the same behavior.

### Runtime data flow

```text
CLI/Android input
  → record/VAD
  → STT
  → AgentSession::run_turn(text)
  → LLM/tool loop
  → structured TurnResult { text, changes, schedule_state }
  → UI renders text and change receipts
  → optional TTS/playback
```

`AgentSession` is independent of the transport. The first executable is the CLI, but a later
HTTP/WebSocket adapter must be able to own sessions without moving planner or audio logic into the
binary. Only one turn may mutate a session at a time; concurrent calls are serialized per session.

## Shared contracts

### Tool contract

```rust
#[async_trait::async_trait]
pub trait Tool: Send + Sync {
    fn name(&self) -> &'static str;
    fn description(&self) -> &'static str;
    fn parameters_schema(&self) -> serde_json::Value;
    async fn call(&self, args: serde_json::Value) -> Result<ToolOutput, ToolError>;
}

pub struct ToolOutput {
    /// JSON or text returned to the LLM.
    pub content: String,
    /// Change receipts collected for the application UI.
    pub changes: Vec<ChangeReceipt>,
    pub schedule_dirty: bool,
}
```

Recoverable errors such as invalid arguments, not found, and optimistic-conflict errors are added
as tool-result messages so the LLM can correct its request. Authentication failures, transport
failures after retry, malformed provider responses, and session cancellation fail the turn.

The per-turn limit counts actual tool calls, including multiple calls returned in one LLM response.
Mutating calls are sequential even when the provider returns them together. Read-only calls may be
parallelized later.

### Agent configuration

Read `$XDG_CONFIG_HOME/takusu/agent.toml`, then apply
`TAKUSU_AGENT__<SECTION>__<KEY>` overrides.

```toml
[llm]
base_url = "https://api.openai.com/v1"
model = "gpt-4.1-mini"
api_key_env = "TAKUSU_LLM_API_KEY"
max_context_tokens = 32000
max_tool_calls = 16
request_timeout_seconds = 60

[server]
url = "http://127.0.0.1:3000"
token = "tsk_..."

[audio]
funasr_url = "ws://127.0.0.1:10095"
tts_url = "http://127.0.0.1:8088"
refs_dir = "./refs"

[skills]
dir = "~/.local/share/takusu/skills"

[schedule]
quiet_period_seconds = 30
```

The system context is rebuilt for each turn from server settings and contains the user's timezone,
current zoned time, dirty-schedule state, available skills index, and task-reference rules. It must
not use the host timezone as a substitute after settings have loaded.

### Change-set contract

All agent task/habit mutations use an atomic server-side change set rather than directly chaining
CRUD endpoints. Migration 014 adds monotonic `revision` columns to tasks/habits and the following
logical tables (exact SQL may follow existing backend conventions):

```sql
CREATE TABLE agent_change_sets (
    id               TEXT PRIMARY KEY,
    idempotency_key  TEXT NOT NULL UNIQUE,
    summary          TEXT NOT NULL,
    inferred_fields  TEXT NOT NULL DEFAULT '[]',
    status           TEXT NOT NULL CHECK(status IN ('applied','reverted')),
    created_at       TEXT NOT NULL DEFAULT (datetime('now')),
    reverted_at      TEXT
);

CREATE TABLE agent_change_operations (
    id               TEXT PRIMARY KEY,
    change_set_id    TEXT NOT NULL REFERENCES agent_change_sets(id),
    sequence         INTEGER NOT NULL,
    operation        TEXT NOT NULL,
    target_type      TEXT NOT NULL,
    target_id        TEXT NOT NULL,
    before_json      TEXT,
    after_json       TEXT,
    target_revision  INTEGER
);

CREATE TABLE planner_state (
    id               TEXT PRIMARY KEY DEFAULT 'active',
    revision         INTEGER NOT NULL DEFAULT 0,
    schedule_dirty   BOOLEAN NOT NULL DEFAULT 0,
    dirty_since      TEXT,
    updated_at       TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE schedule_previews (
    id               TEXT PRIMARY KEY,
    base_revision    INTEGER NOT NULL,
    request_json     TEXT NOT NULL,
    result_json      TEXT NOT NULL,
    impact_json      TEXT NOT NULL,
    created_at       TEXT NOT NULL DEFAULT (datetime('now')),
    committed_at     TEXT
);
```

Every task/habit mutation, including existing non-agent endpoints, increments the target revision
and the global planner revision and marks the schedule dirty. This makes dirty state survive CLI
exit/server restart and lets preview/undo detect intervening edits.

```json
POST /api/agent/change-sets
{
  "idempotency_key": "session-id:turn-id:mutation-index",
  "summary": "演習を30題追加",
  "inferred_fields": [
    {"field":"avg_minutes","value":90,"reason":"類似タスク #42 の実績"}
  ],
  "operations": [
    {"type":"create_task","body":{}}
  ]
}
```

Supported operation types initially are `create_task`, `update_task`, `delete_task`,
`create_habit`, `update_habit`, and `delete_habit`. The server validates all operations first and
applies the change set atomically on both supported backends; partial application is an error. It
returns:

```json
{
  "id": "change-set-id",
  "summary": "演習を30題追加",
  "status": "applied",
  "receipts": [
    {"operation":"create_task","target_type":"task","target_id":"...",
     "before":null,"after":{},"target_revision":"updated_at value"}
  ],
  "schedule_dirty": true
}
```

Endpoints:

- `POST /api/agent/change-sets` — apply once by idempotency key;
- `GET /api/agent/change-sets?limit=...` — app-visible recent changes;
- `GET /api/agent/change-sets/:id` — full before/after and inference details;
- `POST /api/agent/change-sets/:id/revert` — apply the recorded inverse operation.

Revert uses optimistic concurrency: updates succeed only if affected records still match the
receipt revisions, deleted records are restored only if their IDs remain unused, and created
records are removed only if they remain at the recorded revision. A conflict is reported rather
than overwriting later user edits. Delete receipts retain enough data to restore the deleted object
and its relationships. Secrets are never stored in change payloads.

Low-risk creates and updates may apply immediately and be reported with an undo option. Deletes
must be proposed in dialogue and applied only after explicit confirmation. Every mutating request
has an idempotency key so a timeout or repeated voice transcript cannot create duplicates.

### Schedule orchestration contract

A successful task/habit change sets `schedule_dirty = true`; it does not automatically call the
planner. Before answering schedule questions, or after the configured quiet period, the agent calls:

```json
POST /api/schedule/preview
{
  "mode":"full|partial",
  "from":null,
  "until":null,
  "task_ids":null,
  "pinned":[],
  "sleep":"recommended"
}
```

The endpoint runs the planner without replacing the active schedule, stores the exact candidate,
and returns:

```json
{
  "preview_id": "preview-id",
  "base_revision": 42,
  "entries": [],
  "unscheduled_task_ids": [],
  "displaced_task_ids": [],
  "sleep_minutes_before": 480,
  "sleep_minutes_after": 420,
  "warnings": []
}
```

A preview is high impact when it creates unscheduled tasks, displaces an existing fixed/user-edited
commitment, or reduces sleep. Threshold details may be refined, but the response must provide the
facts rather than asking the LLM to infer impact from raw entries. High-impact previews require
confirmation. Commit the exact candidate with `POST /api/schedule/previews/:id/commit`; the server
rejects the commit if `planner_state.revision != base_revision`, if the preview is expired/already
committed, or if another schedule commit won the race. A successful commit atomically replaces the
active schedule, marks the preview committed, clears `schedule_dirty`, and returns the schedule plus
the same impact summary. Expired uncommitted previews are deleted by bounded retention cleanup.

## Sequential work items

### WI-0: Existing scaffold and core loop

**Status**: implemented by the existing takusu-agent scaffold and core-loop changes.

Keep the current `AgentConfig`, `ToolRegistry`, `LlmClient` abstraction, message types, and scripted
mock tests. Before later WIs rely on the loop, align it with the shared contracts above:

- count actual tool calls;
- feed recoverable tool errors back to the model;
- serialize turns per `AgentSession`;
- return a `TurnResult` containing response text and collected change receipts;
- trim by estimated/provider token count and preserve complete tool-call groups;
- inject server timezone and the real skills index rather than empty placeholders.

**Verify**: scripted tests for correction after invalid arguments, multiple calls respecting the
limit, concurrent-turn serialization, history trimming, and receipt collection.

### WI-1: OpenAI chat-completions adapter

**Files**: `crates/takusu-agent/src/llm.rs`.

Implement a hand-written `reqwest` adapter for `POST {base_url}/chat/completions`, including tool
schemas, multiple tool calls, nullable content, and provider error bodies. Retry 429 and retryable
5xx responses with bounded exponential backoff and jitter, but retry mutating tool execution only
through its idempotency key. Apply request timeouts and support cancellation.

“OpenAI-compatible” is a tested baseline, not a guarantee that every OpenRouter, llama.cpp, or
vLLM version has identical behavior. Keep provider capabilities behind `LlmClient`; document and
test each supported endpoint using captured fixtures.

**Verify**: fixture-based serialization/deserialization tests, retry classification tests, and an
ignored real-endpoint smoke test when the configured API key exists.

### WI-2: Text CLI and application-level audio

**Files**: `crates/takusu-agent/src/audio.rs`, `src/bin/agent.rs`, and a playback module in
`takusu-audio`. Remove the obsolete `src/tools/audio.rs` stub and its module registration when the
application-level adapter replaces it.

Implement `takusu-agent --text "今日の予定は?"` first. Then add push-to-talk:

1. the CLI records and transcribes one utterance;
2. it calls one agent turn;
3. it prints the response and change receipts;
4. unless `--no-tts` is set, it synthesizes and plays the response;
5. it waits for the next user-initiated recording.

Playback accepts the actual Irodori response formats and validates WAV headers, sample format,
channel count, and sample rate before opening a `cpal` output stream. Audio/STT/TTS failures do not
corrupt the conversation or retry planner mutations. Add timeouts and cancellation around all
network and device operations.

Do not register `listen` or `speak` as LLM tools. VAD/noise suppression and streaming can be added
behind the same audio adapter later.

**Verify**: text-mode E2E with a mock LLM, WAV parser tests, and manual STT/TTS smoke tests.

### WI-3: Planner read tools and task references

**Files**: `crates/takusu-agent/src/tools/takusu.rs` plus focused client additions if needed.

Implement read-only tools first:

- `list_tasks`, `get_task`;
- `list_habits`, `get_habit`;
- `get_schedule`;
- `get_settings`.

Resolve global task `display_id` directly and habit task references only with their habit scope.
Do not resolve by fetching an arbitrary filtered page. Add a dedicated server/client lookup if the
existing API cannot resolve an identifier uniquely. Datetime interpretation uses server timezone
and `takusu-util`; tool responses include normalized absolute timestamps so the LLM can explain its
interpretation.

**Verify**: argument-schema, timezone boundary, global ID, habit-scoped ID, missing ID, and
ambiguous-reference tests.

### WI-4: Safe mutations and schedule orchestration

**Files**: `014_agent_changes.sql`, storage trait/models and both backends, local/Worker routes,
`takusu-client`, and `tools/takusu.rs`.

Implement the change-set and schedule-preview contracts above. Add mutation tools for task/habit
create/update/delete, `preview_schedule`, `commit_schedule`, `list_changes`, and `revert_change`.
Mutation tool arguments include the model's `inferred_fields`; the agent's final response names
those inferred values and provides the change-set ID.

Cache the persisted planner dirty state in the session and refresh it from server responses.
Multiple conversational edits may produce multiple auditable change sets, but trigger at most one
schedule preview after the quiet period or when a schedule answer requires fresh data. A rejected
schedule preview leaves the schedule dirty; the user may keep the underlying task changes, edit
them, or undo their change sets.

**Verify**: local integration tests for atomicity, idempotency, app-visible history, successful
revert, revert conflict, delete confirmation flow, preview without persistence, impact warnings,
and dirty-state clearing. Run corresponding Worker tests.

### WI-5: Skills with persistent-write safety

**Files**: `crates/takusu-agent/src/tools/skills.rs`.

A skill is UTF-8 markdown under `$XDG_DATA_HOME/takusu/skills`, named `<slug>.md`, with TOML front
matter:

```markdown
+++
name = "weekly-review"
description = "How to run the user's weekly review flow"
+++
(free-form instructions)
```

Tools:

- `skills_list` and `skills_read` are read-only;
- `skills_propose_add` and `skills_propose_edit` validate input and return a bounded, expiring
  proposal ID plus a diff without writing;
- `skills_apply` accepts that proposal ID only after explicit user confirmation and rejects stale
  proposals if the source file changed.

Validate front matter, names, UTF-8, and configurable file/body limits. Reject `/`, `..`, absolute
paths, non-regular files, and symlinks. Built-in skills are read-only. Store a bounded local backup
for rollback and include skill changes in `TurnResult`; never follow instructions from task text,
memory, or fetched content to persist a skill unless the user explicitly requested teaching the
assistant.

Refresh the skills index after a confirmed write and at session start.

**Verify**: temp-directory tests for validation, traversal and symlink rejection, ambiguous edits,
size limits, confirmation, backup/rollback, and prompt-index refresh.

### WI-6: Memory server

**Files**: `015_memory.sql`, storage trait/models and both backends, app/routes, and
`takusu-client`.

Schema:

```sql
CREATE TABLE memories (
    id           TEXT PRIMARY KEY,
    kind         TEXT NOT NULL CHECK(kind IN ('proper_noun','fact','task_note')),
    key          TEXT NOT NULL,
    normalized_key TEXT NOT NULL,
    content      TEXT NOT NULL,
    subject_type TEXT,
    subject_id   TEXT,
    source       TEXT,
    created_at   TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at   TEXT NOT NULL DEFAULT (datetime('now')),
    last_used_at TEXT
);
CREATE INDEX idx_memories_normalized_key ON memories(normalized_key);
CREATE INDEX idx_memories_subject ON memories(subject_type, subject_id);
```

Endpoints:

- `POST /api/memory` — create or explicitly upsert by normalized key/kind/subject;
- `PATCH /api/memory/:id`;
- `GET /api/memory/search?q=...&kind=...&limit=...`;
- `DELETE /api/memory/:id`;
- `GET /api/tasks/similar?q=...&limit=...`.

Normalize Japanese/Latin width and case consistently in Rust for both backends. Search `key` and
`content` with OR semantics and deterministic ranking: exact normalized key, key prefix, key
substring, then content substring, with recency as a tie-breaker. Do not depend only on whitespace
tokenization because Japanese text often has none. Similar tasks are completed tasks ranked by
normalized title overlap and return estimate fields plus active `actual_minutes` once WI-8 exists.

The current repository is a single-workspace data model. If multi-user ownership is introduced,
all memory, task, progress, schedule, and change-set queries must be scoped together; adding only a
`user_id` to memory would not be sufficient.

**Verify**: create/upsert/update/search/delete, Japanese normalization/ranking, subject lookup,
similar completed tasks, limits, and local/Worker parity.

### WI-7: Memory tools and inference flow

**Files**: `crates/takusu-agent/src/tools/memory.rs` and system-context rules.

Implement `memory_save`, `memory_update`, `memory_search`, `memory_delete`, and `similar_tasks`.
The system flow is:

1. search a proper noun before treating it as unknown;
2. if no adequate result exists, ask the user rather than inventing its meaning;
3. save only after the user supplies or confirms the meaning;
4. before creating a task without an estimate, inspect similar completed tasks;
5. state whether the estimate came from history, the user, or model knowledge.

Memory deletion is destructive and requires confirmation. Do not automatically persist arbitrary
conversation content as memory.

**Verify**: scripted turns for memory hit, miss→question→save, estimate from similar task, fallback
estimate disclosure, and confirmed deletion.

### WI-8: Active-session progress management

**Files**: `016_progress.sql`, storage trait/models and both backends, app/routes, and
`takusu-client`.

Add quantitative fields and split lineage to tasks:

```sql
ALTER TABLE tasks ADD COLUMN quantity_total INTEGER;
ALTER TABLE tasks ADD COLUMN quantity_done  INTEGER NOT NULL DEFAULT 0;
ALTER TABLE tasks ADD COLUMN quantity_unit  TEXT;
ALTER TABLE tasks ADD COLUMN completed_at   TEXT;
ALTER TABLE tasks ADD COLUMN split_from_task_id TEXT REFERENCES tasks(id);
ALTER TABLE tasks ADD COLUMN original_quantity_total INTEGER;

CREATE TABLE task_work_sessions (
    id         TEXT PRIMARY KEY,
    task_id    TEXT NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
    started_at TEXT NOT NULL,
    ended_at   TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE progress_events (
    id                TEXT PRIMARY KEY,
    task_id           TEXT NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
    at                TEXT NOT NULL DEFAULT (datetime('now')),
    quantity_done     INTEGER,
    delta_quantity    INTEGER,
    active_minutes    INTEGER NOT NULL,
    note              TEXT
);
```

Only one open work session is allowed per task. Operations are idempotent and validated:

- `POST /api/tasks/:id/work/start`;
- `POST /api/tasks/:id/work/pause`;
- `POST /api/tasks/:id/progress` with cumulative `quantity_done`;
- `POST /api/tasks/:id/work/complete`;
- `GET /api/tasks/:id/progress`;
- `POST /api/tasks/:id/split` with the retained and remainder quantities.

A lower cumulative quantity is treated as an explicit correction, not a new speed observation.
Reject negative quantities and require confirmation when a correction would materially change an
estimate. `quantity_done >= quantity_total` may propose completion, but should not silently close
an open session due only to a transcription error.

For each interval with `delta_quantity > 0` and positive new active time:

```text
minutes_per_unit = delta_active_minutes / delta_quantity
projected_total = minutes_per_unit * quantity_total
bounded_projection = clamp(projected_total, 5 minutes, 24 hours)
new_avg = round(0.5 * old_avg + 0.5 * bounded_projection)
```

With at least two valid interval observations, derive sigma from the sample standard deviation of
projected totals, cap it to the same bounds, and document the exact rounding. With fewer than two,
leave sigma unchanged. Completion `actual_minutes` is the sum of closed active work sessions, not
`completed_at - first_started_at`.

The split endpoint atomically preserves the original task history, records lineage, creates the
remainder, and establishes dependency if requested; the LLM must not emulate splitting by shrinking
historical totals through unrelated CRUD calls.

**Verify**: start/pause/resume, duplicate requests, cumulative progress, zero/decreasing/corrected
values, estimate and sigma math, completion active time, split lineage, and local/Worker parity.

### WI-9: Progress tools and schedule flow

**Files**: `crates/takusu-agent/src/tools/progress.rs`.

Implement `task_start`, `task_pause`, `task_progress`, `task_complete`, and `task_split`. Resolve
task references through WI-3. Report estimate changes and their evidence in the response. Progress
changes mark the schedule dirty; follow the same preview/impact/confirmation flow as WI-4 rather
than rescheduling directly inside the progress tool.

If the user says only “着手した” or “完了した” and multiple tasks are plausible, ask a focused
question. Quantity updates that imply completion, corrections, and splits are summarized before
commit when ambiguity exists.

**Verify**: scripted E2E scenarios for start→pause→resume→progress, estimate update, dirty schedule,
impact confirmation, completion, ambiguous references, correction, and split.

### WI-10: End-to-end hardening

Run complete text and voice scenarios against `takusu-local`, a mock LLM, and optionally a real LLM:

- “演習30題追加” with inferred estimate and visible receipt;
- unknown proper noun → memory miss → question → save → create;
- consecutive edits → one schedule preview;
- urgent task → sleep/ displacement warning → confirm or reject;
- undo a change and handle an optimistic conflict;
- start/pause/progress/complete using active time;
- split a task while preserving history;
- STT/TTS unavailable while text mode and planner state remain usable;
- repeated provider/transport request without duplicate mutation.

Add structured tracing with secret redaction, per-provider timeouts, graceful Ctrl-C cancellation,
and clear CLI rendering of tool activity at `-v`. Do not log tokens, raw audio, private memory
content, or complete LLM prompts by default.

**Verify**: `cargo fmt`, `cargo clippy --workspace`, `cargo nextest run --workspace`, Worker tests,
and documented manual audio smoke tests.

## Implementation order

```text
WI-0 harden existing core
  → WI-1 LLM adapter
  → WI-2 text CLI + audio adapter
  → WI-3 planner reads and references
  → WI-4 safe mutations + schedule preview
  → WI-5 safe skills
  → WI-6 memory server
  → WI-7 memory tools
  → WI-8 active-session progress server
  → WI-9 progress tools
  → WI-10 E2E hardening
```

Use one focused jj change per WI when practical. Because one agent implements the sequence, prefer
updating shared model/storage/router/client contracts once in the server WI and consuming them in
the immediately following agent WI. Do not develop against speculative parallel branches.

Before each push, rebase the current change onto `main`, run focused tests during development, then
run formatting, clippy, and the relevant crate/integration tests. If a contract changes, update this
document and all affected tests in the same change.

## Future: embedding-based memory search

Start with deterministic lexical search and measure failures before adding embeddings. If semantic
recall is justified:

- store a versioned multilingual embedding and model identifier with each memory;
- keep a common storage-trait method for SQLite and D1;
- generate embeddings locally when deployment constraints allow it;
- avoid a dedicated vector database at personal scale;
- account for D1 network transfer and WASM compute rather than assuming a remote full-table scan is
  sub-millisecond;
- rebuild embeddings when the model/version changes;
- retain lexical search as fallback.

`sqlite-vec` may be a local-only optimization behind the same trait. Cloudflare Vectorize or a
separate vector server should be considered only if measured scale or latency requires it.

## Out of scope

- Android `VoiceInteractionService`, hotword activation, and the production session transport;
- streaming audio/VAD/noise suppression beyond the adapter boundary;
- external tools such as Google Maps;
- gamification;
- semantic embeddings before lexical-search measurements;
- multi-user tenancy until ownership is defined consistently for all planner data.
