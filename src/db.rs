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
        CREATE INDEX IF NOT EXISTS idx_tags_memory_id ON tags(memory_id);",
    )?;
    Ok(())
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
}
