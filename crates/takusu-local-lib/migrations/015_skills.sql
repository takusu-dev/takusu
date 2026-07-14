-- Skills table (#WI-6). Stores agent skill definitions with TOML front-matter metadata.
CREATE TABLE IF NOT EXISTS skills (
    slug        TEXT PRIMARY KEY,
    name        TEXT NOT NULL,
    description TEXT NOT NULL,
    body        TEXT NOT NULL,
    built_in    INTEGER NOT NULL DEFAULT 0,
    created_at  TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at  TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX IF NOT EXISTS idx_skills_built_in ON skills(built_in);
