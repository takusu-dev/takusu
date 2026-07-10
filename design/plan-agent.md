# takusu LLM Agent + Memory / Progress Management Plan

## Summary

Implement the voice-assistant half of the takusu vision (main.typ §voice assistant): an LLM
agent that talks to the user (STT/TTS) and operates the planner via tool calling, plus the
server-side **memory** and **progress management** features the agent depends on.

Target tool set for the agent: `speak`, `listen`, `skills` (list/read/add/edit),
`memory` (save/search), and `takusu` (task/habit/schedule CRUD + progress reporting).

This plan is split into **independent work items (WI)** so that multiple agents can work in
parallel without touching the same files. All shared contracts (traits, schemas, endpoint
shapes, migration numbers) are fixed in this document; a work item may rely on another item's
*contract* without waiting for its *implementation*.

## Architecture

```
takusu-agent (new crate, lib + bin)
├── src/lib.rs        # Agent loop, ToolRegistry, AgentConfig
├── src/tool.rs       # Tool trait + ToolError (contract, Phase 0)
├── src/llm.rs        # OpenAI-compatible chat client with tool calling (WI-2)
├── src/tools/
│   ├── audio.rs      # speak / listen (WI-3)
│   ├── takusu.rs     # planner API tools (WI-4)
│   ├── skills.rs     # skills_list / skills_read / skills_add / skills_edit (WI-5)
│   ├── memory.rs     # memory_save / memory_search / similar_tasks (WI-7)
│   └── progress.rs   # task_start / task_progress / task_complete (WI-9)
└── src/bin/agent.rs  # CLI: push-to-talk loop + `--text` mode (WI-10)

takusu-local-lib / takusu-local / takusu-storage / takusu-client (extended)
├── migrations/005_memory.sql    # memories table + tasks FTS (WI-6)
├── migrations/006_progress.sql  # progress columns + progress_events (WI-8)
├── /api/memory/*, /api/tasks/similar          (WI-6)
└── /api/tasks/:id/progress, estimate correction (WI-8)
```

Data flow: mic → `takusu-audio::record` → FunASR STT → LLM (tool loop) → takusu REST API
→ response text → Irodori-TTS → playback.

## Phase 0 — Shared contracts (must land first, single small change)

One agent lands this scaffold; everything else is parallel afterwards.

### 0.1 Crate scaffold

- Add `crates/takusu-agent` to the workspace: `lib.rs` with `AgentConfig`, empty
  `ToolRegistry`, `mod tool;`, `mod tools { pub mod audio; pub mod takusu; pub mod skills;
  pub mod memory; pub mod progress; }` (all modules created as empty stubs so parallel
  branches don't conflict on `mod` declarations).
- Dependencies: `tokio`, `serde`, `serde_json`, `reqwest`, `async-trait`, `thiserror`,
  `takusu-audio`, `takusu-client`, `takusu-util`, `jiff` (all already in workspace).

### 0.2 Tool trait (frozen contract)

```rust
// crates/takusu-agent/src/tool.rs
#[async_trait::async_trait]
pub trait Tool: Send + Sync {
    fn name(&self) -> &'static str;
    fn description(&self) -> &'static str;
    /// JSON Schema for the arguments object (OpenAI function-calling format).
    fn parameters_schema(&self) -> serde_json::Value;
    async fn call(&self, args: serde_json::Value) -> Result<String, ToolError>;
}

#[derive(Debug, thiserror::Error)]
pub enum ToolError {
    #[error("invalid arguments: {0}")] InvalidArgs(String),
    #[error(transparent)] Other(#[from] anyhow-like boxed error), // Box<dyn Error + Send + Sync>
}
```

- Tool output is a plain `String` fed back to the LLM (JSON-encode structured data).
- `ToolRegistry`: `register(Box<dyn Tool>)`, `schemas() -> Vec<serde_json::Value>`,
  `call(name, args)`.

### 0.3 AgentConfig

Read from `$XDG_CONFIG_HOME/takusu/agent.toml` + env overrides (`TAKUSU_AGENT_*`):

```toml
[llm]
base_url = "https://api.openai.com/v1"   # any OpenAI-compatible endpoint
model = "gpt-4.1-mini"
api_key_env = "TAKUSU_LLM_API_KEY"

[server]
url = "http://127.0.0.1:3000"
token = "tsk_..."

[audio]
funasr_url = "ws://127.0.0.1:10095"
tts_url = "http://127.0.0.1:8088"
refs_dir = "./refs"

[skills]
dir = "~/.local/share/takusu/skills"     # default: $XDG_DATA_HOME/takusu/skills
```

### 0.4 Server-side contracts (frozen; implemented by WI-6 / WI-8)

Migration numbers are pre-assigned to avoid collisions: **005 = memory**, **006 = progress**.
Endpoint shapes and schemas are specified inside WI-6 / WI-8 below and must not be changed
without updating this document.

---

## Work items

Each WI is one jj change / PR, independently assignable. "Depends on" refers to *merged code*;
"contract" dependencies only require Phase 0 (this doc + scaffold).

### WI-1: Agent core loop

**Files**: `crates/takusu-agent/src/lib.rs`
**Depends on**: Phase 0. Uses `llm.rs` contract (WI-2) — develop against the trait below.

- `Agent::new(config, registry, llm)` and `Agent::run_turn(user_text) -> Result<String>`:
  standard tool-calling loop (send messages + tool schemas → execute tool calls → append
  results → repeat until plain-text answer; cap at e.g. 16 tool calls per turn).
- Conversation history kept in memory for the session (Vec of messages), trimmed to a
  configurable max length.
- System prompt template: current date/time (jiff, user tz from settings), role description
  in Japanese (assistant speaks Japanese), skills index injection point (see WI-5), and
  the rule that task references use `display_id`.
- Define the LLM abstraction so WI-2 can be swapped in:

```rust
#[async_trait::async_trait]
pub trait LlmClient: Send + Sync {
    async fn chat(&self, messages: &[Message], tools: &[serde_json::Value])
        -> Result<LlmResponse, LlmError>;
}
pub enum LlmResponse { Text(String), ToolCalls(Vec<ToolCall>) }
pub struct ToolCall { pub id: String, pub name: String, pub arguments: serde_json::Value }
pub enum Message { System(String), User(String), Assistant(...), ToolResult { call_id: String, content: String } }
```

**Verify**: unit tests with a mock `LlmClient` (scripted responses) asserting the loop
executes tools and terminates. `cargo nextest run -p takusu-agent`.

### WI-2: OpenAI-compatible LLM client

**Files**: `crates/takusu-agent/src/llm.rs`
**Depends on**: Phase 0 + `LlmClient` trait (WI-1 contract above).

- Hand-rolled `reqwest` client (matches repo style: google-cal, takusu-client are
  hand-rolled; do not add heavyweight SDK deps). `POST {base_url}/chat/completions` with
  `tools`, parse `tool_calls` / `content`.
- Retries with backoff on 429/5xx (max 3). Map errors to `LlmError`.
- Works with OpenAI, OpenRouter, and local servers (llama.cpp / vLLM) since all speak the
  same schema — no provider-specific code.

**Verify**: unit tests deserializing captured response fixtures; optional `--ignored`
integration test hitting a real endpoint when `TAKUSU_LLM_API_KEY` is set.

### WI-3: Audio tools (`speak`, `listen`) + playback

**Files**: `crates/takusu-agent/src/tools/audio.rs`; small addition to
`crates/takusu-audio/src/` (playback module).
**Depends on**: Phase 0.

- **Gap**: takusu-audio has no playback. Add `takusu-audio/src/play.rs` with
  `pub fn play_wav(bytes: &[u8]) -> Result<(), PlayError>` using `cpal` output stream
  (already a dep; avoids adding `rodio`). WAV parsing: minimal reader (16-bit PCM) in the
  same module — Irodori-TTS is asked for `response_format = "wav"`.
- `speak` tool: args `{ "text": string }` → `TtsClient::synthesize` (reference voice via
  `pick_reference_voice(refs_dir)`) → `play_wav`. Returns `"ok"`.
- `listen` tool: args `{ "max_seconds"?: number }` → `takusu_audio::record` →
  `FunASRClient::transcribe` → returns transcript string. (Used when the agent asks a
  follow-up question and waits for the answer.)
- Both tools degrade gracefully: if FunASR/TTS unreachable, return a ToolError message the
  LLM can relay ("音声サーバーに接続できません").

**Verify**: `cargo check -p takusu-audio -p takusu-agent`; manual test via
`cargo run -p takusu-audio-cli -- speak` equivalent; unit test for the WAV parser.

### WI-4: takusu planner tools

**Files**: `crates/takusu-agent/src/tools/takusu.rs`
**Depends on**: Phase 0; uses `takusu-client` as-is.

Thin wrappers over `takusu_client::Client`, exposed as separate tools (better for tool
calling than one mega-tool):

| Tool | Maps to | Notes |
|------|---------|-------|
| `list_tasks` | `list_tasks(TaskQuery)` | filters: status, due range |
| `get_task` | `get_task` | accepts `display_id` or UUID; resolve display_id via list |
| `create_task` | `create_task` | title, end_at, avg/sigma minutes, depends, abandonability |
| `update_task` | `update_task` | partial update incl. status |
| `delete_task` | `delete_task` | |
| `list_habits` / `create_habit` / `update_habit` / `delete_habit` | habit endpoints | |
| `get_schedule` | `get_schedule` | agent formats "today's plan" answers from this |
| `generate_schedule` / `reschedule` | schedule endpoints | |
| `get_settings` | `get_settings` | tz/sleep for date interpretation |

- Datetime arguments accepted as natural ISO strings; use `takusu-util` parsing helpers.
- Tool descriptions written in English, but mention Japanese utterance examples from
  main.typ (e.g. 「演習30題追加」→ create_task) to steer the model.

**Verify**: unit tests for arg-schema parsing + display_id resolution against a mocked
server (or `#[ignore]`d integration test against `cargo run -p takusu-local`).

### WI-5: Skills system (`skills_list` / `skills_read` / `skills_add` / `skills_edit`)

**Files**: `crates/takusu-agent/src/tools/skills.rs`
**Depends on**: Phase 0. Fully local, no server changes.

- A **skill** is a markdown file in `config.skills.dir` (`$XDG_DATA_HOME/takusu/skills/`),
  named `<slug>.md`, with a TOML front-matter block:

```markdown
+++
name = "weekly-review"
description = "How to run the user's weekly review flow"
+++
(free-form instructions the LLM follows when the skill is relevant)
```

- Tools:
  - `skills_list` → JSON array of `{name, description}` (also injected into the system
    prompt at session start so the LLM knows what exists).
  - `skills_read` `{name}` → full body.
  - `skills_add` `{name, description, body}` → create file (error if exists).
  - `skills_edit` `{name, old_string, new_string}` → exact string replacement (mirrors the
    editing style LLMs handle well; error if `old_string` not found/ambiguous).
- Path safety: slugify `name`, reject `/` and `..`.
- This gives the assistant user-teachable behavior ("今後こう対応して" → agent writes a
  skill) without code changes.

**Verify**: unit tests with `tempfile` dir covering add/read/edit/list + path traversal
rejection.

### WI-6: Memory — server side

**Files**: `crates/takusu-local-lib/migrations/005_memory.sql`, `storage_sqlite.rs`,
`storage_workers.rs`, `app.rs`, `takusu-storage/src/{storage.rs,model.rs}`,
`takusu-local` router, `takusu-worker` routes, `takusu-client` methods.
**Depends on**: nothing (pure server work). **Contract frozen below.**

Two memory functions from main.typ: proper nouns / free facts, and similar-task lookup for
estimates.

**Schema (`005_memory.sql`)**:

```sql
CREATE TABLE memories (
    id         TEXT PRIMARY KEY,          -- UUID v7
    kind       TEXT NOT NULL CHECK(kind IN ('proper_noun','fact','task_note')),
    key        TEXT NOT NULL,             -- e.g. the proper noun itself
    content    TEXT NOT NULL,             -- explanation / expansion
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX idx_memories_key ON memories(key);
```

- Search: `LIKE`-based substring match on `key` and `content` (no FTS5 — must also work on
  D1/Workers; revisit embeddings later).
- **Similar tasks**: no new table. `GET /api/tasks/similar?q=<title>` returns completed
  tasks whose title matches (LIKE, token-split OR), each with `avg_minutes`,
  `sigma_minutes`, and — once WI-8 lands — actual duration. Implemented as a storage-trait
  method `find_similar_tasks(q, limit)`.

**Endpoints** (both takusu-local and takusu-worker):

- `POST /api/memory` `{kind, key, content}` → memory row
- `GET /api/memory/search?q=...&kind=...` → `[MemoryRow]`
- `DELETE /api/memory/:id`
- `GET /api/tasks/similar?q=...&limit=...` → `[TaskRow]`

**Client**: add `create_memory`, `search_memory`, `delete_memory`, `similar_tasks` to
`takusu-client`.

**Verify**: `cargo nextest run -p takusu-local` integration tests (create → search →
delete; similar-task lookup); `cargo test -p takusu-worker`.

### WI-7: Memory tools (agent side)

**Files**: `crates/takusu-agent/src/tools/memory.rs`
**Depends on**: Phase 0 + WI-6 *contract* (client method signatures above). Can be written
in parallel with WI-6 against the frozen endpoint shapes; merge after WI-6.

- `memory_save` `{kind, key, content}`, `memory_search` `{q, kind?}`,
  `memory_delete` `{id}` → wrap client.
- `similar_tasks` `{title}` → used by the LLM to fill in missing estimates when creating a
  task ("[固有名詞] p80まで 月曜提出" → search memory for the noun + similar tasks for the
  estimate).
- System prompt (WI-1) instructs: before `create_task` without an explicit estimate, call
  `similar_tasks`; when the user defines/uses an unknown proper noun, `memory_save` it.

**Verify**: unit tests with mocked client; `cargo nextest run -p takusu-agent`.

### WI-8: Progress management — server side

**Files**: `crates/takusu-local-lib/migrations/006_progress.sql`, storage impls, `app.rs`,
routers, `takusu-storage` model, `takusu-client`.
**Depends on**: nothing (parallel with WI-6; different migration number and mostly
different code paths — coordinate only on `takusu-storage/src/model.rs`, where each WI
adds its own new types; merge conflicts there are trivial). **Contract frozen below.**

**Schema (`006_progress.sql`)**:

```sql
ALTER TABLE tasks ADD COLUMN quantity_total INTEGER;             -- e.g. 20 (演習20題); NULL = non-quantitative
ALTER TABLE tasks ADD COLUMN quantity_done  INTEGER NOT NULL DEFAULT 0;
ALTER TABLE tasks ADD COLUMN quantity_unit  TEXT;                -- e.g. "題", "page"
ALTER TABLE tasks ADD COLUMN started_at     TEXT;                -- set when status → in_progress
ALTER TABLE tasks ADD COLUMN completed_at   TEXT;                -- set when status → completed

CREATE TABLE progress_events (
    id            TEXT PRIMARY KEY,       -- UUID v7
    task_id       TEXT NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
    at            TEXT NOT NULL DEFAULT (datetime('now')),
    quantity_done INTEGER,                -- cumulative count at this point (NULL for plain check-ins)
    note          TEXT
);
CREATE INDEX idx_progress_task ON progress_events(task_id, at);
```

**Endpoints**:

- `POST /api/tasks/:id/progress` `{quantity_done?, note?}` →
  records event; sets `started_at`/status=`in_progress` on first event; if
  `quantity_done >= quantity_total`, sets status=`completed` + `completed_at`.
  Response: `{task: TaskRow, estimate_updated: bool, new_avg_minutes?: i64}`.
- `GET /api/tasks/:id/progress` → `[ProgressEvent]`.
- `create_task`/`update_task` accept the new `quantity_*` fields.

**Estimate correction** (pure function in `takusu-local-lib`, unit-tested):

```
rate = (now - started_at) / quantity_done            // minutes per unit
projected_total = rate * quantity_total
new_avg = clamp(projected_total, 5, 24h); blend: 0.5*old + 0.5*projected
```

Applied on each progress event for quantitative in-progress tasks; also update
`sigma_minutes` from the spread of observed rates when ≥2 events exist. After updating the
estimate the server does **not** auto-reschedule (the agent decides whether to call
`reschedule`).

Additionally, on completion of any task record the **actual duration**
(`completed_at - started_at`, when both exist) — this feeds WI-6's `similar_tasks`
estimates (expose `actual_minutes` on TaskRow, computed on read).

**Client**: `report_progress(id, body)`, `list_progress(id)`.

**Verify**: unit tests for the correction function; takusu-local integration tests for the
event flow (start → progress → auto-complete); `cargo test -p takusu-worker`.

### WI-9: Progress tools + flows (agent side)

**Files**: `crates/takusu-agent/src/tools/progress.rs`
**Depends on**: Phase 0 + WI-8 contract. Parallel-safe like WI-7.

- `task_start` `{task}` → status update / first progress event (「着手した」).
- `task_progress` `{task, quantity_done?, note?}` → `report_progress`
  (「現在10題完了した」). Relays `estimate_updated` to the user ("見積もりを更新しました。
  再スケジュールしますか?") and calls `reschedule` on confirmation.
- `task_complete` `{task}` (「タスク完了」).
- **Task splitting**: no server support needed — when the user asks to split (or the LLM
  proposes it because remaining work exceeds the slot), the LLM composes existing tools:
  `update_task` (shrink quantity_total) + `create_task` (remainder with `depends` on the
  original). Document this recipe in the system prompt / a built-in skill.

**Verify**: unit tests with mocked client; end-to-end scenario test with mock LLM script
(start → progress → estimate update → reschedule confirmation).

### WI-10: Agent CLI

**Files**: `crates/takusu-agent/src/bin/agent.rs`
**Depends on**: WI-1 merged (needs the real loop); other tools can be missing (registry is
additive) so this can start early with `--text` mode only.

- `takusu-agent --text "今日の予定は?"` — one-shot text mode (no audio; fastest E2E test).
- `takusu-agent` — interactive voice loop: Enter to talk (push-to-talk via existing
  `record`'s Enter-to-stop) → STT → agent turn → TTS playback → repeat. `Ctrl-C` exits.
- `--no-tts` flag for text output; prints tool calls at `-v`.
- Hotword / VoiceInteractionService is **out of scope** (Android phase, later plan).

**Verify**: `cargo run -p takusu-agent -- --text "..."` against a running takusu-local +
LLM endpoint; document the manual smoke-test in the PR.

---

## Dependency graph / suggested assignment

```
Phase 0 (1 agent, small, land first)
├── WI-1 core loop ──┬── WI-10 CLI (after WI-1 merges)
├── WI-2 LLM client ─┘         (WI-1 & WI-2 parallel via LlmClient trait)
├── WI-3 audio tools           (independent)
├── WI-4 takusu tools          (independent)
├── WI-5 skills                (independent)
├── WI-6 memory server ──── WI-7 memory tools   (WI-7 parallel vs WI-6, merge after)
└── WI-8 progress server ── WI-9 progress tools (same)
```

- Up to **7 agents in parallel** after Phase 0 (WI-1..WI-6, WI-8); WI-7/9/10 follow.
- Only known file-collision point: `takusu-storage/src/model.rs` (WI-6 vs WI-8, additive
  types only) and `takusu-local` router (additive routes). Keep additions in separate
  blocks; rebase with `jj rebase -r @ -d main` before pushing.

## Future: embedding-based memory search (design memo)

Not a work item yet — this records the agreed direction for when LIKE-based recall (WI-6)
proves insufficient.

**Decision: no dedicated vector DB.** Memory holds hundreds to low thousands of rows
(proper nouns / facts / task notes), where brute-force cosine similarity is sub-millisecond;
an ANN index buys nothing at this scale.

- **Storage**: add an `embedding BLOB` column to `memories` (little-endian f32 array).
  Works identically on SQLite and D1, so both storage backends stay symmetric and the
  memory replicates with the same local ↔ Worker sync story as everything else — no
  separate vector-store replication, no extra cloud node.
- **Search**: new storage-trait method `search_memory_semantic(query_vec, limit)`.
  Both backends fetch candidate embeddings and rank by cosine in Rust (WASM on Workers).
- **Embeddings**: generated **locally via ONNX** (`ort` crate) with a small multilingual
  model (candidate: `multilingual-e5-small`, 384 dims — must handle Japanese). Runs
  offline and free, same philosophy as the FunASR/Irodori-TTS local servers. Embedding
  happens agent-side (or in takusu-local) on save/search; the server only stores/compares
  vectors and never depends on an embedding API.
- **Optional local optimization**: if the table grows large, `sqlite-vec` (`vec0` virtual
  table) can accelerate the SQLite backend. It cannot run on D1, so it stays a
  local-only fast path behind the same trait method.

**Rejected candidates**:

- *Cloudflare Vectorize* — proprietary, Workers-only; would break SqliteStorage/
  WorkersStorage symmetry.
- *RuVector* — embeddable Rust crate, but young (2025-11) with quality concerns; heavy
  dependency for no benefit at this scale.
- *HelixDB* — requires a server process (and thus a paid/free-tier cloud node plus its
  own replication and auth); overkill for personal-scale memory.

## Conventions for all work items

- One WI = one jj change: `jj describe` (present tense, lowercase), `jj git push --change`,
  `gh pr create`. Rebase onto `main` before push.
- Run `cargo fmt`, `cargo clippy`, `cargo nextest run -p <crate>` before pushing.
- Do not renumber migrations or alter frozen contracts; if a contract must change, update
  `design/plan-agent.md` in the same change and call it out in the PR description.

## Out of scope (future plans)

- Android VoiceInteractionService / hotword activation (separate plan after CLI E2E works).
- Embedding-based memory search (LIKE first; revisit if recall is poor — design memo above).
- External tools (Google Maps), gamification.
- Streaming STT / streaming LLM responses (one-shot everywhere first).
