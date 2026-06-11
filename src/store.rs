use std::sync::Mutex;
use anyhow::Result;
use rusqlite::{Connection, OptionalExtension};
use crate::model::{NewMemory, MemoryEntry, Layer, MemoryType, StoreResult};

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

    pub fn store(&self, new: NewMemory) -> Result<StoreResult> {
        let id = gen_id();
        let now = now_secs();

        // Deduplicate tags, preserving insertion order.
        let mut seen = std::collections::HashSet::new();
        let tags: Vec<String> = new
            .tags
            .iter()
            .filter(|t| seen.insert((*t).clone()))
            .cloned()
            .collect();

        let mut conn = self.conn.lock().map_err(|_| anyhow::anyhow!("db connection mutex poisoned"))?;
        let tx = conn.transaction()?;

        tx.execute(
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

        for tag in &tags {
            tx.execute(
                "INSERT INTO tags (memory_id, tag) VALUES (?1, ?2)",
                rusqlite::params![id, tag],
            )?;
        }

        // Auto-connect: one directional edge to each distinct other memory
        // that shares at least one tag with this one.
        let mut auto_connected = 0usize;
        if !tags.is_empty() {
            let placeholders = vec!["?"; tags.len()].join(",");
            let sql = format!(
                "SELECT DISTINCT memory_id FROM tags WHERE memory_id != ?1 AND tag IN ({placeholders})"
            );
            let targets: Vec<String> = {
                let mut stmt = tx.prepare(&sql)?;
                let mut params: Vec<&dyn rusqlite::ToSql> = Vec::with_capacity(tags.len() + 1);
                params.push(&id);
                for t in &tags {
                    params.push(t);
                }
                stmt.query_map(params.as_slice(), |row| row.get::<_, String>(0))?
                    .collect::<rusqlite::Result<Vec<String>>>()?
            };
            for target in &targets {
                let edge_id = format!("edge_{}", uuid::Uuid::new_v4().simple());
                auto_connected += tx.execute(
                    "INSERT OR IGNORE INTO edges
                     (id, source_id, target_id, relationship, weight, inferred_by, status, created_at, updated_at)
                     VALUES (?1, ?2, ?3, 'shares_tag', 1.0, 'auto', 'accepted', ?4, ?4)",
                    rusqlite::params![edge_id, id, target, now],
                )?;
            }
        }

        tx.commit()?;
        Ok(StoreResult { id, auto_connected })
    }

    pub fn recall_by_id(&self, id: &str) -> Result<Option<MemoryEntry>> {
        let conn = self.conn.lock().map_err(|_| anyhow::anyhow!("db connection mutex poisoned"))?;
        let result = conn.query_row(
            "SELECT id, layer, type, title, content, source, project, created_at, updated_at
             FROM memories WHERE id = ?1",
            rusqlite::params![id],
            |row| Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, Option<String>>(5)?,
                row.get::<_, Option<String>>(6)?,
                row.get::<_, i64>(7)?,
                row.get::<_, i64>(8)?,
            )),
        );
        match result {
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
            Ok((rid, layer_s, type_s, title, content, source, project, created_at, updated_at)) => {
                Self::row_to_entry(&conn, rid, layer_s, type_s, title, content, source, project, created_at, updated_at).map(Some)
            }
        }
    }

    pub fn recall_by_title(&self, title: &str) -> Result<Option<MemoryEntry>> {
        let conn = self.conn.lock().map_err(|_| anyhow::anyhow!("db connection mutex poisoned"))?;
        let result = conn.query_row(
            "SELECT id, layer, type, title, content, source, project, created_at, updated_at
             FROM memories WHERE title = ?1 ORDER BY updated_at DESC LIMIT 1",
            rusqlite::params![title],
            |row| Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, Option<String>>(5)?,
                row.get::<_, Option<String>>(6)?,
                row.get::<_, i64>(7)?,
                row.get::<_, i64>(8)?,
            )),
        );
        match result {
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
            Ok((rid, layer_s, type_s, title, content, source, project, created_at, updated_at)) => {
                Self::row_to_entry(&conn, rid, layer_s, type_s, title, content, source, project, created_at, updated_at).map(Some)
            }
        }
    }

    pub fn search(&self, query: &str, limit: usize) -> Result<Vec<crate::model::SearchHit>> {
        let trimmed = query.trim();
        if trimmed.is_empty() {
            return Ok(Vec::new());
        }
        // Strip non-alphanumeric characters from each whitespace-separated token,
        // then quote the remainder.  Join with OR so that any matching word
        // surfaces a result.  This prevents FTS5 syntax errors from punctuation
        // or operator keywords in the input.
        let fts_query = trimmed
            .split_whitespace()
            .map(|t| t.chars().filter(|c| c.is_alphanumeric()).collect::<String>())
            .filter(|t| !t.is_empty())
            .map(|t| format!("\"{}\"", t.replace('"', "\"\"")))
            .collect::<Vec<_>>()
            .join(" OR ");

        if fts_query.is_empty() {
            return Ok(Vec::new());
        }

        let conn = self.conn.lock().map_err(|_| anyhow::anyhow!("db connection mutex poisoned"))?;
        let rows = {
            let mut stmt = conn.prepare(
                "SELECT m.id, m.title, m.layer, snippet(memories_fts, 1, '[', ']', '…', 12)
                 FROM memories_fts
                 JOIN memories m ON m.rowid = memories_fts.rowid
                 WHERE memories_fts MATCH ?1
                 ORDER BY rank
                 LIMIT ?2",
            )?;
            stmt
                .query_map(rusqlite::params![fts_query, limit as i64], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                    ))
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?
        };

        let mut hits = Vec::with_capacity(rows.len());
        for (id, title, layer_s, snippet) in rows {
            let layer = layer_s.parse::<Layer>()?;
            let tags = Self::fetch_tags(&conn, &id)?;
            hits.push(crate::model::SearchHit { id, title, snippet, layer, tags });
        }
        Ok(hits)
    }

    pub fn update(&self, id: &str, upd: crate::model::UpdateMemory) -> Result<bool> {
        let now = now_secs();
        let mut conn = self.conn.lock().map_err(|_| anyhow::anyhow!("db connection mutex poisoned"))?;
        let tx = conn.transaction()?;

        let existing: Option<(String, String)> = tx
            .query_row(
                "SELECT title, content FROM memories WHERE id = ?1",
                rusqlite::params![id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;

        let (old_title, old_content) = match existing {
            None => return Ok(false),
            Some(x) => x,
        };

        let new_title = upd.title.unwrap_or(old_title);
        let new_content = match upd.content {
            None => old_content,
            Some(c) if upd.merge_content => format!("{old_content}\n{c}"),
            Some(c) => c,
        };

        tx.execute(
            "UPDATE memories SET title = ?1, content = ?2, updated_at = ?3 WHERE id = ?4",
            rusqlite::params![new_title, new_content, now, id],
        )?;

        if let Some(tags) = upd.tags {
            tx.execute("DELETE FROM tags WHERE memory_id = ?1", rusqlite::params![id])?;
            let mut seen = std::collections::HashSet::new();
            for tag in tags.iter().filter(|t| seen.insert((*t).clone())) {
                tx.execute(
                    "INSERT INTO tags (memory_id, tag) VALUES (?1, ?2)",
                    rusqlite::params![id, tag],
                )?;
            }
        }

        tx.commit()?;
        Ok(true)
    }

    /// Resolve a recall query in priority order: exact id, exact title, then
    /// the top FTS result. Returns the full entry, or None if nothing matches.
    pub fn resolve_recall(&self, query: &str) -> Result<Option<MemoryEntry>> {
        if let Some(entry) = self.recall_by_id(query)? {
            return Ok(Some(entry));
        }
        if let Some(entry) = self.recall_by_title(query)? {
            return Ok(Some(entry));
        }
        if let Some(hit) = self.search(query, 1)?.into_iter().next() {
            return self.recall_by_id(&hit.id);
        }
        Ok(None)
    }

    pub fn count(&self) -> Result<usize> {
        let conn = self.conn.lock().map_err(|_| anyhow::anyhow!("db connection mutex poisoned"))?;
        let n: i64 = conn.query_row("SELECT COUNT(*) FROM memories", [], |r| r.get(0))?;
        Ok(n as usize)
    }

    pub fn delete(&self, id: &str) -> Result<bool> {
        let mut conn = self.conn.lock().map_err(|_| anyhow::anyhow!("db connection mutex poisoned"))?;
        let tx = conn.transaction()?;
        tx.execute("DELETE FROM tags WHERE memory_id = ?1", rusqlite::params![id])?;
        tx.execute(
            "DELETE FROM edges WHERE source_id = ?1 OR target_id = ?1",
            rusqlite::params![id],
        )?;
        // The AFTER DELETE trigger on memories keeps the FTS index in sync.
        let n = tx.execute("DELETE FROM memories WHERE id = ?1", rusqlite::params![id])?;
        tx.commit()?;
        Ok(n > 0)
    }

    #[allow(clippy::too_many_arguments)]
    fn row_to_entry(
        conn: &Connection,
        rid: String, layer_s: String, type_s: String, title: String, content: String,
        source: Option<String>, project: Option<String>, created_at: i64, updated_at: i64,
    ) -> Result<MemoryEntry> {
        let layer = layer_s.parse::<Layer>()?;
        let memory_type = type_s.parse::<MemoryType>()?;
        let tags = Self::fetch_tags(conn, &rid)?;
        Ok(MemoryEntry { id: rid, layer, memory_type, title, content, source, project, tags, created_at, updated_at })
    }

    fn fetch_tags(conn: &Connection, memory_id: &str) -> Result<Vec<String>> {
        let mut stmt = conn.prepare("SELECT tag FROM tags WHERE memory_id = ?1")?;
        let tags = stmt
            .query_map(rusqlite::params![memory_id], |row| row.get(0))?
            .collect::<rusqlite::Result<Vec<String>>>()?;
        Ok(tags)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{db, model::{Layer, MemoryType, NewMemory}};

    fn open_test_store() -> SqliteStore {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys=ON;").unwrap();
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
        let r = s.store(sample()).unwrap();
        assert!(r.id.starts_with("mem_"), "id was: {}", r.id);
    }

    #[test]
    fn store_persists_row_and_tags() {
        let s = open_test_store();
        let id = s.store(sample()).unwrap().id;
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

    #[test]
    fn recall_by_id_returns_full_entry_with_tags() {
        let s = open_test_store();
        let id = s.store(sample()).unwrap().id;
        let entry = s.recall_by_id(&id).unwrap().unwrap();
        assert_eq!(entry.title, "golang preferences");
        assert_eq!(entry.layer, Layer::Personal);
        assert_eq!(entry.tags.len(), 2);
        assert!(entry.tags.contains(&"golang".to_string()));
    }

    #[test]
    fn recall_by_id_returns_none_for_missing() {
        let s = open_test_store();
        assert!(s.recall_by_id("mem_doesnotexist").unwrap().is_none());
    }

    #[test]
    fn recall_by_title_returns_entry() {
        let s = open_test_store();
        s.store(sample()).unwrap();
        let entry = s.recall_by_title("golang preferences").unwrap().unwrap();
        assert_eq!(entry.layer, Layer::Personal);
    }

    #[test]
    fn recall_by_title_returns_none_for_missing() {
        let s = open_test_store();
        assert!(s.recall_by_title("no such title").unwrap().is_none());
    }

    #[test]
    fn store_deduplicates_tags() {
        let s = open_test_store();
        let new = NewMemory {
            title: "dedup test".to_string(),
            content: "testing tag deduplication".to_string(),
            layer: Layer::Personal,
            memory_type: MemoryType::Preference,
            tags: vec!["rust".to_string(), "rust".to_string(), "go".to_string()],
            project: None,
            source: None,
        };
        let id = s.store(new).unwrap().id;
        let conn = s.conn.lock().unwrap();
        let tag_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM tags WHERE memory_id=?1", [&id], |r| r.get(0))
            .unwrap();
        assert_eq!(tag_count, 2, "expected 2 unique tags, got {tag_count}");
    }

    #[test]
    fn store_auto_connects_memories_sharing_a_tag() {
        let s = open_test_store();
        let first = s.store(sample()).unwrap();
        assert_eq!(first.auto_connected, 0, "first memory connects to nothing");

        let second = s.store(NewMemory {
            title: "go http patterns".to_string(),
            content: "chi router, middleware chain".to_string(),
            layer: Layer::Personal,
            memory_type: MemoryType::Preference,
            tags: vec!["golang".to_string()],
            project: None,
            source: None,
        }).unwrap();
        assert_eq!(second.auto_connected, 1, "should connect to the golang memory");

        let conn = s.conn.lock().unwrap();
        let edge_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM edges WHERE source_id=?1 AND relationship='shares_tag'",
                [&second.id], |r| r.get(0),
            ).unwrap();
        assert_eq!(edge_count, 1);
    }

    #[test]
    fn search_finds_memory_by_content_keyword() {
        let s = open_test_store();
        s.store(sample()).unwrap();
        let hits = s.search("pgx", 5).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].title, "golang preferences");
        assert!(hits[0].snippet.to_lowercase().contains("pgx"));
        assert_eq!(hits[0].tags.len(), 2);
    }

    #[test]
    fn search_returns_empty_for_no_match() {
        let s = open_test_store();
        s.store(sample()).unwrap();
        assert!(s.search("kubernetes", 5).unwrap().is_empty());
    }

    #[test]
    fn search_empty_query_returns_empty() {
        let s = open_test_store();
        s.store(sample()).unwrap();
        assert!(s.search("   ", 5).unwrap().is_empty());
    }

    #[test]
    fn search_handles_punctuation_without_error() {
        let s = open_test_store();
        s.store(sample()).unwrap();
        let hits = s.search("pgx\" OR (", 5).unwrap();
        assert_eq!(hits.len(), 1);
    }

    #[test]
    fn search_respects_limit() {
        let s = open_test_store();
        for i in 0..3 {
            s.store(NewMemory {
                title: format!("note {i}"),
                content: "shared keyword apple".to_string(),
                layer: Layer::Personal,
                memory_type: MemoryType::Preference,
                tags: vec![format!("t{i}")],
                project: None,
                source: None,
            }).unwrap();
        }
        assert_eq!(s.search("apple", 2).unwrap().len(), 2);
    }

    #[test]
    fn update_replaces_title_and_content() {
        let s = open_test_store();
        let id = s.store(sample()).unwrap().id;
        let found = s.update(&id, crate::model::UpdateMemory {
            title: Some("golang prefs v2".to_string()),
            content: Some("now also: chi router".to_string()),
            tags: None,
            merge_content: false,
        }).unwrap();
        assert!(found);
        let e = s.recall_by_id(&id).unwrap().unwrap();
        assert_eq!(e.title, "golang prefs v2");
        assert_eq!(e.content, "now also: chi router");
        assert_eq!(e.tags.len(), 2, "tags untouched when None");
    }

    #[test]
    fn update_merge_content_appends() {
        let s = open_test_store();
        let id = s.store(sample()).unwrap().id;
        s.update(&id, crate::model::UpdateMemory {
            title: None,
            content: Some("addendum line".to_string()),
            tags: None,
            merge_content: true,
        }).unwrap();
        let e = s.recall_by_id(&id).unwrap().unwrap();
        assert!(e.content.contains("pgx v5 driver"), "old content kept");
        assert!(e.content.contains("addendum line"), "new content appended");
    }

    #[test]
    fn update_replaces_tags_when_provided() {
        let s = open_test_store();
        let id = s.store(sample()).unwrap().id;
        s.update(&id, crate::model::UpdateMemory {
            title: None,
            content: None,
            tags: Some(vec!["rust".to_string(), "rust".to_string(), "mcp".to_string()]),
            merge_content: false,
        }).unwrap();
        let e = s.recall_by_id(&id).unwrap().unwrap();
        assert_eq!(e.tags.len(), 2, "tags replaced and deduped");
        assert!(e.tags.contains(&"mcp".to_string()));
        assert!(!e.tags.contains(&"golang".to_string()));
    }

    #[test]
    fn update_returns_false_for_missing() {
        let s = open_test_store();
        let found = s.update("mem_nope", crate::model::UpdateMemory::default()).unwrap();
        assert!(!found);
    }

    #[test]
    fn update_reindexes_fts() {
        let s = open_test_store();
        let id = s.store(sample()).unwrap().id;
        s.update(&id, crate::model::UpdateMemory {
            title: None,
            content: Some("kubernetes operators".to_string()),
            tags: None,
            merge_content: false,
        }).unwrap();
        assert_eq!(s.search("kubernetes", 5).unwrap().len(), 1);
        assert!(s.search("pgx", 5).unwrap().is_empty());
    }

    #[test]
    fn delete_removes_memory_tags_and_fts() {
        let s = open_test_store();
        let id = s.store(sample()).unwrap().id;
        let deleted = s.delete(&id).unwrap();
        assert!(deleted);
        assert!(s.recall_by_id(&id).unwrap().is_none());
        assert!(s.search("pgx", 5).unwrap().is_empty(), "FTS entry removed");
        let conn = s.conn.lock().unwrap();
        let tag_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM tags WHERE memory_id=?1", [&id], |r| r.get(0))
            .unwrap();
        assert_eq!(tag_count, 0, "tags removed");
    }

    #[test]
    fn delete_removes_connected_edges() {
        let s = open_test_store();
        let first = s.store(sample()).unwrap().id;
        let second = s.store(NewMemory {
            title: "go again".to_string(),
            content: "more golang".to_string(),
            layer: Layer::Personal,
            memory_type: MemoryType::Preference,
            tags: vec!["golang".to_string()],
            project: None,
            source: None,
        }).unwrap().id;
        s.delete(&second).unwrap();
        let edge_count: i64 = {
            let conn = s.conn.lock().unwrap();
            conn.query_row(
                "SELECT COUNT(*) FROM edges WHERE source_id=?1 OR target_id=?1",
                [&second], |r| r.get(0),
            ).unwrap()
        };
        assert_eq!(edge_count, 0, "edges referencing deleted memory removed");
        assert!(s.recall_by_id(&first).unwrap().is_some());
    }

    #[test]
    fn delete_returns_false_for_missing() {
        let s = open_test_store();
        assert!(!s.delete("mem_nope").unwrap());
    }

    #[test]
    fn resolve_recall_matches_by_id() {
        let s = open_test_store();
        let id = s.store(sample()).unwrap().id;
        let e = s.resolve_recall(&id).unwrap().unwrap();
        assert_eq!(e.title, "golang preferences");
    }

    #[test]
    fn resolve_recall_matches_by_exact_title() {
        let s = open_test_store();
        s.store(sample()).unwrap();
        let e = s.resolve_recall("golang preferences").unwrap().unwrap();
        assert_eq!(e.tags.len(), 2);
    }

    #[test]
    fn resolve_recall_falls_back_to_fts() {
        let s = open_test_store();
        s.store(sample()).unwrap();
        let e = s.resolve_recall("pgx driver").unwrap().unwrap();
        assert_eq!(e.title, "golang preferences");
    }

    #[test]
    fn resolve_recall_returns_none_when_nothing_matches() {
        let s = open_test_store();
        s.store(sample()).unwrap();
        assert!(s.resolve_recall("kubernetes operators").unwrap().is_none());
    }

    #[test]
    fn count_returns_number_of_memories() {
        let s = open_test_store();
        assert_eq!(s.count().unwrap(), 0);
        s.store(sample()).unwrap();
        assert_eq!(s.count().unwrap(), 1);
    }

    #[test]
    fn store_does_not_connect_when_no_shared_tags() {
        let s = open_test_store();
        s.store(sample()).unwrap();
        let other = s.store(NewMemory {
            title: "unrelated".to_string(),
            content: "nothing in common".to_string(),
            layer: Layer::Personal,
            memory_type: MemoryType::Preference,
            tags: vec!["cooking".to_string()],
            project: None,
            source: None,
        }).unwrap();
        assert_eq!(other.auto_connected, 0);
    }
}
