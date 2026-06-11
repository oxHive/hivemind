use std::sync::Mutex;
use anyhow::Result;
use rusqlite::Connection;
use crate::model::{NewMemory, MemoryEntry, Layer, MemoryType};

fn gen_id() -> String {
    format!("mem_{}", uuid::Uuid::new_v4().simple())
}

fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
}

pub struct SqliteStore {
    pub(crate) conn: Mutex<Connection>,
}

impl SqliteStore {
    pub fn new(conn: Connection) -> Self {
        Self { conn: Mutex::new(conn) }
    }

    pub fn store(&self, new: NewMemory) -> Result<String> {
        let id = gen_id();
        let now = now_secs();
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO memories (id, layer, type, title, content, source, project, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            rusqlite::params![
                id,
                new.layer.to_string(),
                new.memory_type.to_string(),
                new.title,
                new.content,
                new.source,
                new.project,
                now,
                now
            ],
        )?;
        for tag in &new.tags {
            conn.execute(
                "INSERT INTO tags (memory_id, tag) VALUES (?1, ?2)",
                rusqlite::params![id, tag],
            )?;
        }
        Ok(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{db, model::{Layer, MemoryType, NewMemory}};

    fn open_test_store() -> SqliteStore {
        let conn = Connection::open_in_memory().unwrap();
        db::create_schema(&conn).unwrap();
        SqliteStore::new(conn)
    }

    fn sample() -> NewMemory {
        NewMemory {
            title: "golang preferences".to_string(),
            content: "Use uber/zap for logging, sqlc for DB, pgx v5 driver".to_string(),
            layer: Layer::Personal,
            memory_type: MemoryType::Preference,
            tags: vec!["golang".to_string(), "preferences".to_string()],
            project: None,
            source: Some("test".to_string()),
        }
    }

    #[test]
    fn store_returns_mem_prefixed_id() {
        let s = open_test_store();
        let id = s.store(sample()).unwrap();
        assert!(id.starts_with("mem_"), "id was: {id}");
    }

    #[test]
    fn store_persists_row_and_tags() {
        let s = open_test_store();
        let id = s.store(sample()).unwrap();
        let conn = s.conn.lock().unwrap();
        let title: String = conn
            .query_row("SELECT title FROM memories WHERE id=?1", [&id], |r| r.get(0))
            .unwrap();
        assert_eq!(title, "golang preferences");
        let tag_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM tags WHERE memory_id=?1", [&id], |r| r.get(0))
            .unwrap();
        assert_eq!(tag_count, 2);
    }
}
