ALTER TABLE memories ADD COLUMN layer TEXT NOT NULL DEFAULT 'workspace';
ALTER TABLE memories ADD COLUMN memory_type TEXT NOT NULL DEFAULT 'project';

ALTER TABLE conflicts ADD COLUMN local_content TEXT NOT NULL DEFAULT '';

CREATE TABLE IF NOT EXISTS sync_journal (
    memory_id   TEXT PRIMARY KEY,
    content     TEXT NOT NULL,
    updated_at  INTEGER NOT NULL,
    recorded_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS _meta (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
