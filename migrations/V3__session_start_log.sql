CREATE TABLE session_start_log (
    id TEXT PRIMARY KEY,
    project_name TEXT NOT NULL,
    project_path TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    max_tokens INTEGER NOT NULL,
    used_tokens INTEGER NOT NULL,
    memories_recalled INTEGER NOT NULL,
    truncated INTEGER NOT NULL,
    loaded_json TEXT NOT NULL,
    skipped_json TEXT NOT NULL
);

CREATE INDEX idx_session_start_log_created_at ON session_start_log (created_at DESC);
