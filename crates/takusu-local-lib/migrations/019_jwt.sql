-- Migrate tokens from SHA-256 hashes to JWT metadata.
-- We recreate the table because SQLite cannot drop a UNIQUE column directly.
-- Old tokens are copied into jti for reference, but they cannot be used as JWTs
-- because they were not signed with the new JWT secret. Users must reissue tokens.

PRAGMA foreign_keys = OFF;

CREATE TABLE tokens_new (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    jti         TEXT,
    scope       TEXT NOT NULL DEFAULT 'read-write',
    label       TEXT,
    created_by  TEXT NOT NULL,
    created_at  TEXT NOT NULL DEFAULT (datetime('now')),
    revoked_at  TEXT,
    expires_at  TEXT
);

INSERT INTO tokens_new (id, jti, scope, label, created_by, created_at, revoked_at)
SELECT id, token_hash, 'read-write', label, created_by, created_at, revoked_at FROM tokens;

DROP TABLE tokens;
ALTER TABLE tokens_new RENAME TO tokens;

CREATE INDEX IF NOT EXISTS idx_tokens_jti ON tokens(jti);

PRAGMA foreign_keys = ON;
