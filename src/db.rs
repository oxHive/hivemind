use anyhow::Result;
use rusqlite::Connection;

pub fn create_schema(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS memories (
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
        END;",
    )?;
    // Backfill the FTS index from the canonical `memories` rows. This is needed
    // when upgrading a pre-FTS database; triggers keep the index in sync after
    // creation. `rebuild` is idempotent and cheap at expected data sizes. (A
    // COUNT-based "skip if in sync" guard does NOT work here: COUNT(*) on an
    // external-content FTS5 table reflects the content table, not the index.)
    conn.execute_batch("INSERT INTO memories_fts(memories_fts) VALUES('rebuild');")?;
    Ok(())
}

pub fn resolve_db_path() -> String {
    if let Ok(path) = std::env::var("HIVEMIND_DB_PATH") {
        return path;
    }
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    let dir = format!("{home}/.local/share/hivemind");
    std::fs::create_dir_all(&dir).ok();
    format!("{dir}/memory.db")
}

pub fn open(path: &str) -> Result<Connection> {
    let conn = Connection::open(path)?;
    conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")?;
    create_schema(&conn)?;
    Ok(conn)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_schema_backfills_fts_for_preexisting_rows() {
        let conn = Connection::open_in_memory().unwrap();
        // Simulate a pre-FTS (Phase 1) database: memories table + a row, no FTS index.
        conn.execute_batch(
            "CREATE TABLE memories (id TEXT PRIMARY KEY, layer TEXT NOT NULL, type TEXT NOT NULL,
             title TEXT NOT NULL, content TEXT NOT NULL, source TEXT, project TEXT,
             created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL);",
        ).unwrap();
        conn.execute(
            "INSERT INTO memories (id, layer, type, title, content, created_at, updated_at)
             VALUES ('mem_old','personal','preference','old note','legacy kubernetes content',1,1)",
            [],
        ).unwrap();
        // Upgrading the schema must create the FTS index and backfill the existing row.
        create_schema(&conn).unwrap();
        let hits: i64 = conn
            .query_row("SELECT COUNT(*) FROM memories_fts WHERE memories_fts MATCH 'kubernetes'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(hits, 1, "pre-existing row was not backfilled into FTS");
    }

    #[test]
    fn create_schema_creates_memories_and_tags_tables() {
        let conn = Connection::open_in_memory().unwrap();
        create_schema(&conn).unwrap();

        let mut stmt = conn
            .prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
            .unwrap();
        let tables: Vec<String> = stmt
            .query_map([], |row| row.get(0))
            .unwrap()
            .map(|r| r.unwrap())
            .collect();

        assert!(tables.contains(&"memories".to_string()), "memories table missing");
        assert!(tables.contains(&"tags".to_string()), "tags table missing");
    }

    #[test]
    fn create_schema_is_idempotent() {
        let conn = Connection::open_in_memory().unwrap();
        create_schema(&conn).unwrap();
        create_schema(&conn).unwrap();
    }

    #[test]
    fn create_schema_creates_fts_and_edges() {
        let conn = Connection::open_in_memory().unwrap();
        create_schema(&conn).unwrap();
        let mut stmt = conn
            .prepare("SELECT name FROM sqlite_master WHERE name IN ('memories_fts','edges') ORDER BY name")
            .unwrap();
        let names: Vec<String> = stmt
            .query_map([], |r| r.get(0)).unwrap().map(|r| r.unwrap()).collect();
        assert!(names.contains(&"edges".to_string()), "edges table missing");
        assert!(names.contains(&"memories_fts".to_string()), "memories_fts missing");
    }

    #[test]
    fn fts_index_is_populated_by_insert_trigger() {
        let conn = Connection::open_in_memory().unwrap();
        create_schema(&conn).unwrap();
        conn.execute(
            "INSERT INTO memories (id, layer, type, title, content, created_at, updated_at)
             VALUES ('mem_x','personal','preference','Rust testing','use cargo test and clippy',1,1)",
            [],
        ).unwrap();
        let hits: i64 = conn
            .query_row("SELECT COUNT(*) FROM memories_fts WHERE memories_fts MATCH 'clippy'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(hits, 1, "FTS insert trigger did not index the row");
    }

    #[test]
    fn fts_delete_trigger_removes_from_index() {
        let conn = Connection::open_in_memory().unwrap();
        create_schema(&conn).unwrap();
        conn.execute(
            "INSERT INTO memories (id, layer, type, title, content, created_at, updated_at)
             VALUES ('mem_x','personal','preference','Rust testing','clippy lints',1,1)",
            [],
        ).unwrap();
        conn.execute("DELETE FROM memories WHERE id='mem_x'", []).unwrap();
        let hits: i64 = conn
            .query_row("SELECT COUNT(*) FROM memories_fts WHERE memories_fts MATCH 'clippy'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(hits, 0, "FTS delete trigger did not remove the row");
    }
}
