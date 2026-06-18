use anyhow::Result;
use rusqlite::Connection;

mod embedded {
    use refinery::embed_migrations;
    embed_migrations!("migrations");
}

pub fn run_migrations(conn: &mut Connection) -> Result<()> {
    embedded::migrations::runner().run(conn)?;
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
    let mut conn = Connection::open(path)?;
    conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")?;
    embedded::migrations::runner().run(&mut conn)?;
    Ok(conn)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() -> Connection {
        let mut conn = Connection::open_in_memory().unwrap();
        embedded::migrations::runner().run(&mut conn).unwrap();
        conn
    }

    #[test]
    fn migration_backfills_fts_for_preexisting_rows() {
        let mut conn = Connection::open_in_memory().unwrap();
        // Simulate a pre-FTS (Phase 1) database: memories table + a row, no FTS index.
        conn.execute_batch(
            "CREATE TABLE memories (id TEXT PRIMARY KEY, layer TEXT NOT NULL, type TEXT NOT NULL,
             title TEXT NOT NULL, content TEXT NOT NULL, source TEXT, project TEXT,
             created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL);",
        )
        .unwrap();
        conn.execute(
            "INSERT INTO memories (id, layer, type, title, content, created_at, updated_at)
             VALUES ('mem_old','personal','preference','old note','legacy kubernetes content',1,1)",
            [],
        )
        .unwrap();
        embedded::migrations::runner().run(&mut conn).unwrap();
        let hits: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM memories_fts WHERE memories_fts MATCH 'kubernetes'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(hits, 1, "pre-existing row was not backfilled into FTS");
    }

    #[test]
    fn migration_creates_memories_and_tags_tables() {
        let conn = setup();
        let mut stmt = conn
            .prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
            .unwrap();
        let tables: Vec<String> = stmt
            .query_map([], |row| row.get(0))
            .unwrap()
            .map(|r| r.unwrap())
            .collect();
        assert!(
            tables.contains(&"memories".to_string()),
            "memories table missing"
        );
        assert!(tables.contains(&"tags".to_string()), "tags table missing");
    }

    #[test]
    fn migration_is_idempotent() {
        let mut conn = Connection::open_in_memory().unwrap();
        embedded::migrations::runner().run(&mut conn).unwrap();
        embedded::migrations::runner().run(&mut conn).unwrap();
    }

    #[test]
    fn migration_creates_fts_and_edges() {
        let conn = setup();
        let mut stmt = conn
            .prepare(
                "SELECT name FROM sqlite_master WHERE name IN ('memories_fts','edges') ORDER BY name",
            )
            .unwrap();
        let names: Vec<String> = stmt
            .query_map([], |r| r.get(0))
            .unwrap()
            .map(|r| r.unwrap())
            .collect();
        assert!(names.contains(&"edges".to_string()), "edges table missing");
        assert!(
            names.contains(&"memories_fts".to_string()),
            "memories_fts missing"
        );
    }

    #[test]
    fn fts_index_is_populated_by_insert_trigger() {
        let conn = setup();
        conn.execute(
            "INSERT INTO memories (id, layer, type, title, content, created_at, updated_at)
             VALUES ('mem_x','personal','preference','Rust testing','use cargo test and clippy',1,1)",
            [],
        )
        .unwrap();
        let hits: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM memories_fts WHERE memories_fts MATCH 'clippy'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(hits, 1, "FTS insert trigger did not index the row");
    }

    #[test]
    fn fts_delete_trigger_removes_from_index() {
        let conn = setup();
        conn.execute(
            "INSERT INTO memories (id, layer, type, title, content, created_at, updated_at)
             VALUES ('mem_x','personal','preference','Rust testing','clippy lints',1,1)",
            [],
        )
        .unwrap();
        conn.execute("DELETE FROM memories WHERE id='mem_x'", [])
            .unwrap();
        let hits: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM memories_fts WHERE memories_fts MATCH 'clippy'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(hits, 0, "FTS delete trigger did not remove the row");
    }

    #[test]
    fn migration_creates_kv_table() {
        let conn = setup();
        let exists: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE name='kv'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(exists, 1, "kv table not created");
    }

    #[test]
    fn migration_creates_feedback_and_conflicts() {
        let conn = setup();
        let mut stmt = conn
            .prepare(
                "SELECT name FROM sqlite_master WHERE name IN ('feedback','conflicts') ORDER BY name",
            )
            .unwrap();
        let names: Vec<String> = stmt
            .query_map([], |r| r.get(0))
            .unwrap()
            .map(|r| r.unwrap())
            .collect();
        assert_eq!(names, vec!["conflicts".to_string(), "feedback".to_string()]);
    }
}
