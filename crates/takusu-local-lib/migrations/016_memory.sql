-- Memory tables (#WI-7). Stores user-confirmed facts, proper nouns, and task notes.
CREATE TABLE IF NOT EXISTS memories (
    id               TEXT PRIMARY KEY,
    kind             TEXT NOT NULL CHECK(kind IN ('proper_noun', 'fact', 'task_note')),
    key              TEXT NOT NULL,
    normalized_key   TEXT NOT NULL,
    content          TEXT NOT NULL,
    normalized_content TEXT NOT NULL,
    subject_type     TEXT NOT NULL DEFAULT '',
    subject_id       TEXT NOT NULL DEFAULT '',
    source           TEXT NOT NULL CHECK(source IN ('user_confirmed', 'agent_inferred', 'imported')),
    revision         INTEGER NOT NULL DEFAULT 1 CHECK(revision >= 1),
    created_at       TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at       TEXT NOT NULL DEFAULT (datetime('now')),
    last_used_at     TEXT
);

CREATE UNIQUE INDEX IF NOT EXISTS uq_memories_logical_key
    ON memories(kind, normalized_key, subject_type, subject_id);
CREATE INDEX IF NOT EXISTS idx_memories_normalized_key
    ON memories(normalized_key);
CREATE INDEX IF NOT EXISTS idx_memories_subject
    ON memories(subject_type, subject_id);
CREATE INDEX IF NOT EXISTS idx_memories_kind_updated
    ON memories(kind, updated_at DESC);

-- Idempotency receipts for approved memory operations.
CREATE TABLE IF NOT EXISTS memory_operations (
    operation_id     TEXT PRIMARY KEY,
    request_hash     TEXT NOT NULL,
    response_json    TEXT NOT NULL,
    created_at       TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_memory_operations_created_at
    ON memory_operations(created_at);
