CREATE TABLE IF NOT EXISTS memories (
    id         TEXT PRIMARY KEY,
    layer      TEXT NOT NULL,
    type       TEXT NOT NULL,
    title      TEXT NOT NULL,
    content    TEXT NOT NULL,
    source     TEXT,
    project    TEXT,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS tags (
    memory_id TEXT REFERENCES memories(id) ON DELETE CASCADE,
    tag       TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_tags_memory_id ON tags(memory_id);

CREATE TABLE IF NOT EXISTS edges (
    id           TEXT PRIMARY KEY,
    source_id    TEXT NOT NULL REFERENCES memories(id),
    target_id    TEXT NOT NULL REFERENCES memories(id),
    relationship TEXT NOT NULL,
    weight       REAL DEFAULT 1.0,
    inferred_by  TEXT NOT NULL,
    status       TEXT DEFAULT 'accepted',
    confidence   REAL,
    reason       TEXT,
    created_at   INTEGER NOT NULL,
    updated_at   INTEGER NOT NULL,
    UNIQUE(source_id, target_id, relationship)
);

CREATE TABLE IF NOT EXISTS feedback (
    id         TEXT PRIMARY KEY,
    memory_id  TEXT REFERENCES memories(id),
    edge_id    TEXT REFERENCES edges(id),
    type       TEXT NOT NULL,
    note       TEXT,
    status     TEXT DEFAULT 'open',
    created_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS conflicts (
    id          TEXT PRIMARY KEY,
    memory_id   TEXT REFERENCES memories(id),
    winner      TEXT NOT NULL,
    loser       TEXT NOT NULL,
    winner_src  TEXT NOT NULL,
    loser_src   TEXT NOT NULL,
    detected_at INTEGER NOT NULL,
    status      TEXT DEFAULT 'open'
);

CREATE TABLE IF NOT EXISTS kv (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

CREATE VIRTUAL TABLE IF NOT EXISTS memories_fts USING fts5(
    title, content, content='memories', content_rowid='rowid'
);

CREATE TRIGGER IF NOT EXISTS memories_ai AFTER INSERT ON memories BEGIN
    INSERT INTO memories_fts(rowid, title, content)
    VALUES (new.rowid, new.title, new.content);
END;

CREATE TRIGGER IF NOT EXISTS memories_ad AFTER DELETE ON memories BEGIN
    INSERT INTO memories_fts(memories_fts, rowid, title, content)
    VALUES ('delete', old.rowid, old.title, old.content);
END;

CREATE TRIGGER IF NOT EXISTS memories_au AFTER UPDATE ON memories BEGIN
    INSERT INTO memories_fts(memories_fts, rowid, title, content)
    VALUES ('delete', old.rowid, old.title, old.content);
    INSERT INTO memories_fts(rowid, title, content)
    VALUES (new.rowid, new.title, new.content);
END;

INSERT INTO memories_fts(memories_fts) VALUES('rebuild');
