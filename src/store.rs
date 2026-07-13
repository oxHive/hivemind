use anyhow::{Result, anyhow};
use libsql::{Connection, params};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryEntry {
    pub id: String,
    pub title: String,
    pub content: String,
    pub tags: Vec<String>,
    pub created_at: i64,
    pub updated_at: i64,
    pub token_count: Option<i64>,
    pub layer: String,
    pub memory_type: String,
}

pub struct NewMemoryRow<'a> {
    pub id: &'a str,
    pub title: &'a str,
    pub content: &'a str,
    pub tags: &'a [String],
    pub token_count: Option<i64>,
    pub layer: &'a str,
    pub memory_type: &'a str,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EdgeEntry {
    pub id: String,
    pub source_id: String,
    pub target_id: String,
    pub relationship: String,
    pub status: String,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeedbackEntry {
    pub id: String,
    pub memory_id: String,
    pub signal: String,
    pub note: Option<String>,
    pub status: String,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConflictEntry {
    pub id: String,
    pub memory_id: String,
    pub remote_content: String,
    pub local_content: String,
    pub remote_updated_at: i64,
    pub local_updated_at: i64,
    pub status: String,
    pub created_at: i64,
}

pub struct JournalRow {
    pub memory_id: String,
    pub content: String,
    pub updated_at: i64,
}

pub struct SqliteStore {
    pub(crate) conn: Connection,
}

pub const VALID_RELATIONSHIPS: &[&str] = &[
    "shares_tag",
    "applies_to",
    "pairs_with",
    "used_in",
    "related_to",
    "custom",
];

/// Quote a raw user query for FTS5 MATCH: every whitespace-separated term is
/// wrapped in double quotes (FTS5 string syntax) with embedded quotes doubled,
/// so characters like / + ' - can never be parsed as FTS5 operators.
pub(crate) fn fts_quote(query: &str) -> String {
    query
        .split_whitespace()
        .map(|term| format!("\"{}\"", term.replace('"', "\"\"")))
        .collect::<Vec<_>>()
        .join(" ")
}

/// A memory may have at most one tag in the `project` namespace — this is
/// the only namespace with this restriction (see the tag-namespace-system
/// design spec); all others allow multiple values per memory.
fn validate_single_project_tag(tags: &[String]) -> Result<()> {
    let project_tag_count = tags
        .iter()
        .filter(|t| t.to_lowercase().starts_with("project:"))
        .count();
    if project_tag_count > 1 {
        return Err(anyhow!("a memory can have at most one project:* tag"));
    }
    Ok(())
}

impl SqliteStore {
    pub fn new(conn: Connection) -> Self {
        Self { conn }
    }

    pub async fn store(&self, m: &NewMemoryRow<'_>) -> Result<()> {
        validate_single_project_tag(m.tags)?;
        let now = chrono_now();
        let token_count = m
            .token_count
            .unwrap_or_else(|| crate::budget::count_entry_tokens(m.title, m.content) as i64);

        let tx = self.conn.transaction().await?;
        tx.execute(
            "INSERT INTO memories (id, title, content, created_at, updated_at, token_count, layer, memory_type)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
             ON CONFLICT(id) DO UPDATE SET
               title = excluded.title,
               content = excluded.content,
               updated_at = excluded.updated_at,
               token_count = excluded.token_count,
               layer = excluded.layer,
               memory_type = excluded.memory_type",
            params![m.id, m.title, m.content, now, now, token_count, m.layer, m.memory_type],
        )
        .await?;

        tx.execute(
            "DELETE FROM memory_tags WHERE memory_id = ?1",
            params![m.id],
        )
        .await?;
        for tag in m.tags {
            let tag_lower = tag.to_lowercase();
            tx.execute(
                "INSERT OR IGNORE INTO memory_tags (memory_id, tag) VALUES (?1, ?2)",
                params![m.id, tag_lower.as_str()],
            )
            .await?;
        }

        // Auto-connect memories sharing a tag: one statement per tag, skipping
        // pairs already linked in either direction.
        for tag in m.tags {
            let tag_lower = tag.to_lowercase();
            tx.execute(
                "INSERT INTO edges (id, source_id, target_id, relationship, status, created_at)
                 SELECT 'edge_' || lower(hex(randomblob(16))), ?1, mt.memory_id, 'shares_tag', 'active', ?2
                 FROM memory_tags mt
                 WHERE mt.tag = ?3 AND mt.memory_id != ?1
                   AND NOT EXISTS (
                       SELECT 1 FROM edges e
                       WHERE e.relationship = 'shares_tag'
                         AND ((e.source_id = ?1 AND e.target_id = mt.memory_id)
                           OR (e.source_id = mt.memory_id AND e.target_id = ?1)))",
                params![m.id, now, tag_lower.as_str()],
            )
            .await?;
        }

        // Journal the write
        tx.execute(
            "INSERT INTO sync_journal (memory_id, content, updated_at, recorded_at)
             VALUES (?1, ?2, ?3, ?3)
             ON CONFLICT(memory_id) DO UPDATE SET
               content = excluded.content, updated_at = excluded.updated_at, recorded_at = excluded.recorded_at",
            params![m.id, m.content, now],
        )
        .await?;

        tx.commit().await?;
        Ok(())
    }

    pub async fn recall_by_id(&self, id: &str) -> Result<Option<MemoryEntry>> {
        let mut rows = self
            .conn
            .query(
                "SELECT id, title, content, created_at, updated_at, token_count, layer, memory_type FROM memories WHERE id = ?1",
                params![id],
            )
            .await?;
        if let Some(row) = rows.next().await? {
            let entry = self.row_to_entry(&row)?;
            let tags = self.fetch_tags(&entry.id).await?;
            Ok(Some(MemoryEntry { tags, ..entry }))
        } else {
            Ok(None)
        }
    }

    pub async fn recall_by_title(&self, title: &str) -> Result<Option<MemoryEntry>> {
        let mut rows = self
            .conn
            .query(
                "SELECT id, title, content, created_at, updated_at, token_count, layer, memory_type FROM memories WHERE title = ?1",
                params![title],
            )
            .await?;
        if let Some(row) = rows.next().await? {
            let entry = self.row_to_entry(&row)?;
            let tags = self.fetch_tags(&entry.id).await?;
            Ok(Some(MemoryEntry { tags, ..entry }))
        } else {
            Ok(None)
        }
    }

    pub async fn search(&self, query: &str, limit: i64) -> Result<Vec<MemoryEntry>> {
        let quoted = fts_quote(query);
        if quoted.is_empty() {
            return Ok(Vec::new());
        }
        let mut rows = self
            .conn
            .query(
                "SELECT m.id, m.title, m.content, m.created_at, m.updated_at, m.token_count, m.layer, m.memory_type
                 FROM memories m
                 JOIN memories_fts f ON m.rowid = f.rowid
                 WHERE memories_fts MATCH ?1
                 ORDER BY rank
                 LIMIT ?2",
                params![quoted, limit],
            )
            .await?;
        let mut results = Vec::new();
        while let Some(row) = rows.next().await? {
            let entry = self.row_to_entry(&row)?;
            let tags = self.fetch_tags(&entry.id).await?;
            results.push(MemoryEntry { tags, ..entry });
        }
        Ok(results)
    }

    pub async fn update(
        &self,
        id: &str,
        title: &str,
        content: &str,
        tags: &[String],
    ) -> Result<bool> {
        validate_single_project_tag(tags)?;
        let now = chrono_now();
        let token_count = crate::budget::count_entry_tokens(title, content) as i64;
        let tx = self.conn.transaction().await?;
        let changed = tx
            .execute(
                "UPDATE memories SET title = ?1, content = ?2, updated_at = ?3, token_count = ?4 WHERE id = ?5",
                params![title, content, now, token_count, id],
            )
            .await?;
        if changed == 0 {
            return Ok(false);
        }
        tx.execute("DELETE FROM memory_tags WHERE memory_id = ?1", params![id])
            .await?;
        for tag in tags {
            let tag_lower = tag.to_lowercase();
            tx.execute(
                "INSERT OR IGNORE INTO memory_tags (memory_id, tag) VALUES (?1, ?2)",
                params![id, tag_lower.as_str()],
            )
            .await?;
        }

        // Journal the write
        tx.execute(
            "INSERT INTO sync_journal (memory_id, content, updated_at, recorded_at)
             VALUES (?1, ?2, ?3, ?3)
             ON CONFLICT(memory_id) DO UPDATE SET
               content = excluded.content, updated_at = excluded.updated_at, recorded_at = excluded.recorded_at",
            params![id, content, now],
        )
        .await?;

        tx.commit().await?;
        Ok(true)
    }

    pub async fn delete(&self, id: &str) -> Result<bool> {
        let changed = self
            .conn
            .execute("DELETE FROM memories WHERE id = ?1", params![id])
            .await?;
        Ok(changed > 0)
    }

    pub async fn resolve_recall(&self, query: &str) -> Result<Vec<MemoryEntry>> {
        // Try exact title match first
        if let Some(entry) = self.recall_by_title(query).await? {
            return Ok(vec![entry]);
        }
        // Fall back to FTS search
        let results = self.search(query, 5).await?;
        Ok(results)
    }

    pub async fn delete_all(&self) -> Result<i64> {
        let changed = self.conn.execute("DELETE FROM memories", ()).await?;
        Ok(changed as i64)
    }

    pub async fn count(&self) -> Result<i64> {
        let mut rows = self.conn.query("SELECT COUNT(*) FROM memories", ()).await?;
        let row = rows.next().await?.ok_or_else(|| anyhow!("no count row"))?;
        Ok(row.get(0)?)
    }

    pub async fn list_memories(&self, limit: i64, offset: i64) -> Result<Vec<MemoryEntry>> {
        let mut rows = self
            .conn
            .query(
                "SELECT id, title, content, created_at, updated_at, token_count, layer, memory_type
                 FROM memories ORDER BY updated_at DESC LIMIT ?1 OFFSET ?2",
                params![limit, offset],
            )
            .await?;
        let mut results = Vec::new();
        while let Some(row) = rows.next().await? {
            let entry = self.row_to_entry(&row)?;
            let tags = self.fetch_tags(&entry.id).await?;
            results.push(MemoryEntry { tags, ..entry });
        }
        Ok(results)
    }

    pub async fn list_edges(&self, memory_id: Option<&str>) -> Result<Vec<EdgeEntry>> {
        let mut rows = if let Some(mid) = memory_id {
            self.conn
                .query(
                    "SELECT id, source_id, target_id, relationship, status, created_at
                     FROM edges WHERE source_id = ?1 OR target_id = ?1 ORDER BY created_at DESC",
                    params![mid],
                )
                .await?
        } else {
            self.conn
                .query(
                    "SELECT id, source_id, target_id, relationship, status, created_at
                     FROM edges ORDER BY created_at DESC",
                    (),
                )
                .await?
        };
        let mut results = Vec::new();
        while let Some(row) = rows.next().await? {
            results.push(EdgeEntry {
                id: row.get(0)?,
                source_id: row.get(1)?,
                target_id: row.get(2)?,
                relationship: row.get(3)?,
                status: row.get(4)?,
                created_at: row.get(5)?,
            });
        }
        Ok(results)
    }

    pub async fn create_edge(
        &self,
        source_id: &str,
        target_id: &str,
        relationship: &str,
    ) -> Result<crate::model::EdgeCreate> {
        use crate::model::EdgeCreate;
        if !VALID_RELATIONSHIPS.contains(&relationship) || source_id == target_id {
            return Ok(EdgeCreate::InvalidRelationship);
        }
        let endpoints: i64 = {
            let mut rows = self
                .conn
                .query(
                    "SELECT COUNT(*) FROM memories WHERE id IN (?1, ?2)",
                    params![source_id, target_id],
                )
                .await?;
            rows.next().await?.unwrap().get(0)?
        };
        if endpoints != 2 {
            return Ok(EdgeCreate::MissingEndpoint);
        }
        let dup: i64 = {
            let mut rows = self
                .conn
                .query(
                    "SELECT COUNT(*) FROM edges WHERE relationship = ?3
                     AND ((source_id = ?1 AND target_id = ?2) OR (source_id = ?2 AND target_id = ?1))",
                    params![source_id, target_id, relationship],
                )
                .await?;
            rows.next().await?.unwrap().get(0)?
        };
        if dup > 0 {
            return Ok(EdgeCreate::Duplicate);
        }
        let id = format!("edge_{}", uuid::Uuid::new_v4().simple());
        self.conn
            .execute(
                "INSERT INTO edges (id, source_id, target_id, relationship, status, created_at)
                 VALUES (?1, ?2, ?3, ?4, 'active', ?5)",
                params![
                    id.as_str(),
                    source_id,
                    target_id,
                    relationship,
                    chrono_now()
                ],
            )
            .await?;
        Ok(EdgeCreate::Created(id))
    }

    pub async fn set_edge_status(&self, id: &str, status: &str) -> Result<bool> {
        let changed = self
            .conn
            .execute(
                "UPDATE edges SET status = ?1 WHERE id = ?2",
                params![status, id],
            )
            .await?;
        Ok(changed > 0)
    }

    pub async fn create_feedback(
        &self,
        memory_id: &str,
        signal: &str,
        note: Option<&str>,
    ) -> Result<FeedbackEntry> {
        let id = format!("fb_{}", uuid::Uuid::new_v4().simple());
        let now = chrono_now();
        self.conn
            .execute(
                "INSERT INTO feedback (id, memory_id, signal, note, status, created_at)
                 VALUES (?1, ?2, ?3, ?4, 'pending', ?5)",
                params![id.as_str(), memory_id, signal, note, now],
            )
            .await?;
        Ok(FeedbackEntry {
            id,
            memory_id: memory_id.to_string(),
            signal: signal.to_string(),
            note: note.map(|s| s.to_string()),
            status: "pending".to_string(),
            created_at: now,
        })
    }

    pub async fn list_feedback(
        &self,
        memory_id: Option<&str>,
        status: Option<&str>,
    ) -> Result<Vec<FeedbackEntry>> {
        let mut rows = match (memory_id, status) {
            (Some(mid), Some(status)) => {
                self.conn
                    .query(
                        "SELECT id, memory_id, signal, note, status, created_at
                         FROM feedback WHERE memory_id = ?1 AND status = ?2 ORDER BY created_at DESC",
                        params![mid, status],
                    )
                    .await?
            }
            (Some(mid), None) => {
                self.conn
                    .query(
                        "SELECT id, memory_id, signal, note, status, created_at
                         FROM feedback WHERE memory_id = ?1 ORDER BY created_at DESC",
                        params![mid],
                    )
                    .await?
            }
            (None, Some(status)) => {
                self.conn
                    .query(
                        "SELECT id, memory_id, signal, note, status, created_at
                         FROM feedback WHERE status = ?1 ORDER BY created_at DESC",
                        params![status],
                    )
                    .await?
            }
            (None, None) => {
                self.conn
                    .query(
                        "SELECT id, memory_id, signal, note, status, created_at
                         FROM feedback ORDER BY created_at DESC",
                        (),
                    )
                    .await?
            }
        };
        let mut results = Vec::new();
        while let Some(row) = rows.next().await? {
            results.push(FeedbackEntry {
                id: row.get(0)?,
                memory_id: row.get(1)?,
                signal: row.get(2)?,
                note: row.get(3)?,
                status: row.get(4)?,
                created_at: row.get(5)?,
            });
        }
        Ok(results)
    }

    pub async fn set_feedback_status(&self, id: &str, status: &str) -> Result<bool> {
        let changed = self
            .conn
            .execute(
                "UPDATE feedback SET status = ?1 WHERE id = ?2",
                params![status, id],
            )
            .await?;
        Ok(changed > 0)
    }

    pub async fn resolve_conflict(&self, id: &str, resolution: &str) -> Result<bool> {
        let Some(conflict) = self.get_conflict_by_id(id).await? else {
            return Ok(false);
        };
        if conflict.status != "pending" {
            return Ok(false);
        }
        if resolution == "keep_local"
            && let Some(mem) = self.recall_by_id(&conflict.memory_id).await?
        {
            self.update(&mem.id, &mem.title, &conflict.local_content, &mem.tags)
                .await?;
        }
        let changed = self
            .conn
            .execute(
                "UPDATE conflicts SET status = ?1 WHERE id = ?2",
                params![resolution, id],
            )
            .await?;
        Ok(changed > 0)
    }

    pub async fn take_journal(&self) -> Result<Vec<JournalRow>> {
        let mut rows = self
            .conn
            .query(
                "SELECT memory_id, content, updated_at FROM sync_journal",
                (),
            )
            .await?;
        let mut out = Vec::new();
        while let Some(row) = rows.next().await? {
            out.push(JournalRow {
                memory_id: row.get(0)?,
                content: row.get(1)?,
                updated_at: row.get(2)?,
            });
        }
        Ok(out)
    }

    pub async fn detect_conflicts(&self, journal: &[JournalRow]) -> Result<usize> {
        let mut found = 0usize;
        for j in journal {
            let current = self.recall_by_id(&j.memory_id).await?;
            if let Some(cur) = current
                && cur.content != j.content
            {
                self.write_conflict(
                    &j.memory_id,
                    &cur.content,
                    &j.content,
                    cur.updated_at,
                    j.updated_at,
                )
                .await?;
                found += 1;
            }
            self.conn
                .execute(
                    "DELETE FROM sync_journal WHERE memory_id = ?1",
                    params![j.memory_id.as_str()],
                )
                .await?;
        }
        Ok(found)
    }

    pub async fn pending_conflict_count(&self) -> Result<i64> {
        let mut rows = self
            .conn
            .query(
                "SELECT COUNT(*) FROM conflicts WHERE status = 'pending'",
                (),
            )
            .await?;
        Ok(rows.next().await?.unwrap().get(0)?)
    }

    pub async fn set_meta(&self, key: &str, value: &str) -> Result<()> {
        self.conn
            .execute(
                "INSERT INTO _meta (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                params![key, value],
            )
            .await?;
        Ok(())
    }

    pub async fn get_meta(&self, key: &str) -> Result<Option<String>> {
        let mut rows = self
            .conn
            .query("SELECT value FROM _meta WHERE key = ?1", params![key])
            .await?;
        Ok(match rows.next().await? {
            Some(row) => Some(row.get(0)?),
            None => None,
        })
    }

    pub async fn list_conflicts(&self, status: Option<&str>) -> Result<Vec<ConflictEntry>> {
        let mut rows = if let Some(status) = status {
            self.conn
                .query(
                    "SELECT id, memory_id, remote_content, local_content, remote_updated_at, local_updated_at, status, created_at
                     FROM conflicts WHERE status = ?1 ORDER BY created_at DESC",
                    params![status],
                )
                .await?
        } else {
            self.conn
                .query(
                    "SELECT id, memory_id, remote_content, local_content, remote_updated_at, local_updated_at, status, created_at
                     FROM conflicts ORDER BY created_at DESC",
                    (),
                )
                .await?
        };
        let mut results = Vec::new();
        while let Some(row) = rows.next().await? {
            results.push(ConflictEntry {
                id: row.get(0)?,
                memory_id: row.get(1)?,
                remote_content: row.get(2)?,
                local_content: row.get(3)?,
                remote_updated_at: row.get(4)?,
                local_updated_at: row.get(5)?,
                status: row.get(6)?,
                created_at: row.get(7)?,
            });
        }
        Ok(results)
    }

    pub async fn get_conflict_by_id(&self, id: &str) -> Result<Option<ConflictEntry>> {
        let mut rows = self
            .conn
            .query(
                "SELECT id, memory_id, remote_content, local_content, remote_updated_at, local_updated_at, status, created_at
                 FROM conflicts WHERE id = ?1",
                params![id],
            )
            .await?;
        if let Some(row) = rows.next().await? {
            Ok(Some(ConflictEntry {
                id: row.get(0)?,
                memory_id: row.get(1)?,
                remote_content: row.get(2)?,
                local_content: row.get(3)?,
                remote_updated_at: row.get(4)?,
                local_updated_at: row.get(5)?,
                status: row.get(6)?,
                created_at: row.get(7)?,
            }))
        } else {
            Ok(None)
        }
    }

    pub async fn write_conflict(
        &self,
        memory_id: &str,
        remote_content: &str,
        local_content: &str,
        remote_updated_at: i64,
        local_updated_at: i64,
    ) -> Result<ConflictEntry> {
        let id = format!("conflict_{}", uuid::Uuid::new_v4().simple());
        let now = chrono_now();
        self.conn
            .execute(
                "INSERT INTO conflicts (id, memory_id, remote_content, local_content, remote_updated_at, local_updated_at, status, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'pending', ?7)",
                params![id.as_str(), memory_id, remote_content, local_content, remote_updated_at, local_updated_at, now],
            )
            .await?;
        Ok(ConflictEntry {
            id,
            memory_id: memory_id.to_string(),
            remote_content: remote_content.to_string(),
            local_content: local_content.to_string(),
            remote_updated_at,
            local_updated_at,
            status: "pending".to_string(),
            created_at: now,
        })
    }

    async fn fetch_tags(&self, memory_id: &str) -> Result<Vec<String>> {
        let mut rows = self
            .conn
            .query(
                "SELECT tag FROM memory_tags WHERE memory_id = ?1 ORDER BY tag",
                params![memory_id],
            )
            .await?;
        let mut tags = Vec::new();
        while let Some(row) = rows.next().await? {
            tags.push(row.get(0)?);
        }
        Ok(tags)
    }

    fn row_to_entry(&self, row: &libsql::Row) -> Result<MemoryEntry> {
        Ok(MemoryEntry {
            id: row.get(0)?,
            title: row.get(1)?,
            content: row.get(2)?,
            tags: Vec::new(),
            created_at: row.get(3)?,
            updated_at: row.get(4)?,
            token_count: row.get(5)?,
            layer: row.get(6)?,
            memory_type: row.get(7)?,
        })
    }
}

fn chrono_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{config::SyncSettings, db};
    use tempfile::TempDir;

    async fn make_store() -> (SqliteStore, TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.db");
        let sync = SyncSettings::default();
        let database = db::open_database(&sync, path.to_str().unwrap())
            .await
            .unwrap();
        let conn = database.connect().unwrap();
        db::run_migrations(&conn).await.unwrap();
        (SqliteStore::new(conn), dir)
    }

    #[tokio::test]
    async fn store_persists_row_and_tags() {
        let (s, _dir) = make_store().await;
        s.store(&test_row(
            "mem_1",
            "My Title",
            "content here",
            &["rust".into(), "test".into()],
        ))
        .await
        .unwrap();
        let entry = s.recall_by_id("mem_1").await.unwrap().unwrap();
        assert_eq!(entry.title, "My Title");
        assert_eq!(entry.content, "content here");
        assert!(entry.tags.contains(&"rust".to_string()));
        assert!(entry.tags.contains(&"test".to_string()));
    }

    #[tokio::test]
    async fn store_deduplicates_tags() {
        let (s, _dir) = make_store().await;
        s.store(&test_row(
            "mem_2",
            "Title",
            "body",
            &["rust".into(), "rust".into()],
        ))
        .await
        .unwrap();
        let entry = s.recall_by_id("mem_2").await.unwrap().unwrap();
        assert_eq!(entry.tags.len(), 1);
    }

    #[tokio::test]
    async fn store_lowercases_tags() {
        let (s, _dir) = make_store().await;
        s.store(&test_row(
            "mem_upper",
            "Title",
            "content",
            &["Lang:Rust".into(), "PROJECT:HiveMind".into()],
        ))
        .await
        .unwrap();
        let entry = s.recall_by_id("mem_upper").await.unwrap().unwrap();
        assert!(entry.tags.contains(&"lang:rust".to_string()));
        assert!(entry.tags.contains(&"project:hivemind".to_string()));
    }

    #[tokio::test]
    async fn store_rejects_more_than_one_project_tag() {
        let (s, _dir) = make_store().await;
        let result = s
            .store(&test_row(
                "mem_multi_project",
                "Title",
                "content",
                &["project:hivemind".into(), "project:oxhive".into()],
            ))
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn update_rejects_more_than_one_project_tag() {
        let (s, _dir) = make_store().await;
        let tags: Vec<String> = vec![];
        s.store(&test_row("mem_up", "Title", "content", &tags))
            .await
            .unwrap();
        let result = s
            .update(
                "mem_up",
                "Title",
                "content",
                &["project:a".into(), "project:b".into()],
            )
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn store_auto_connects_memories_sharing_a_tag() {
        let (s, _dir) = make_store().await;
        s.store(&test_row("mem_a", "A", "body a", &["shared".into()]))
            .await
            .unwrap();
        s.store(&test_row("mem_b", "B", "body b", &["shared".into()]))
            .await
            .unwrap();
        let edges = s.list_edges(None).await.unwrap();
        assert!(!edges.is_empty(), "expected auto-edge from shared tag");
    }

    #[tokio::test]
    async fn delete_removes_memory_tags_and_fts() {
        let (s, _dir) = make_store().await;
        s.store(&test_row(
            "mem_del",
            "Delete Me",
            "some content",
            &["tag1".into()],
        ))
        .await
        .unwrap();
        s.delete("mem_del").await.unwrap();
        assert!(s.recall_by_id("mem_del").await.unwrap().is_none());
        let results = s.search("some content", 10).await.unwrap();
        assert!(results.is_empty(), "FTS should not return deleted memory");
    }

    #[tokio::test]
    async fn delete_removes_connected_edges() {
        let (s, _dir) = make_store().await;
        s.store(&test_row("mem_e1", "E1", "body", &["tag_e".into()]))
            .await
            .unwrap();
        s.store(&test_row("mem_e2", "E2", "body", &["tag_e".into()]))
            .await
            .unwrap();
        s.delete("mem_e1").await.unwrap();
        let edges = s.list_edges(None).await.unwrap();
        assert!(
            edges.is_empty(),
            "edges involving deleted memory should be gone"
        );
    }

    #[tokio::test]
    async fn search_returns_results_for_matching_content() {
        let (s, _dir) = make_store().await;
        s.store(&test_row(
            "mem_s1",
            "Rust Tips",
            "use iterators not loops",
            &[],
        ))
        .await
        .unwrap();
        let results = s.search("iterators", 10).await.unwrap();
        assert!(!results.is_empty());
        assert_eq!(results[0].id, "mem_s1");
    }

    #[tokio::test]
    async fn conflict_round_trip() {
        let (s, _dir) = make_store().await;
        s.store(&test_row("mem_c1", "C1", "local content", &[]))
            .await
            .unwrap();
        let entry = s.recall_by_id("mem_c1").await.unwrap().unwrap();
        let conflict = s
            .write_conflict(
                "mem_c1",
                "remote content",
                "local content",
                entry.updated_at + 1,
                entry.updated_at,
            )
            .await
            .unwrap();
        let fetched = s.get_conflict_by_id(&conflict.id).await.unwrap().unwrap();
        assert_eq!(fetched.remote_content, "remote content");
        let resolved = s
            .resolve_conflict(&conflict.id, "keep_local")
            .await
            .unwrap();
        assert!(resolved);
        let after = s.get_conflict_by_id(&conflict.id).await.unwrap().unwrap();
        assert_eq!(after.status, "keep_local");
    }

    #[tokio::test]
    async fn update_returns_false_for_missing_id() {
        let (s, _dir) = make_store().await;
        let updated = s
            .update("mem_nonexistent", "title", "new content", &[])
            .await
            .unwrap();
        assert!(!updated);
    }

    #[tokio::test]
    async fn list_memories_returns_all_stored() {
        let (s, _dir) = make_store().await;
        s.store(&test_row("mem_a", "Alpha", "first", &["a".into()]))
            .await
            .unwrap();
        s.store(&test_row("mem_b", "Beta", "second", &["b".into()]))
            .await
            .unwrap();
        let list = s.list_memories(10, 0).await.unwrap();
        assert_eq!(list.len(), 2);
    }

    #[tokio::test]
    async fn list_edges_filtered_by_memory_id() {
        let (s, _dir) = make_store().await;
        s.store(&test_row("mem_x", "X", "body", &["shared_tag".into()]))
            .await
            .unwrap();
        s.store(&test_row("mem_y", "Y", "body", &["shared_tag".into()]))
            .await
            .unwrap();
        s.store(&test_row("mem_z", "Z", "body", &["other_tag".into()]))
            .await
            .unwrap();
        s.create_edge("mem_x", "mem_z", "related_to").await.unwrap();

        let all = s.list_edges(None).await.unwrap();
        let filtered = s.list_edges(Some("mem_x")).await.unwrap();

        assert!(all.len() >= filtered.len());
        assert!(
            filtered
                .iter()
                .all(|e| e.source_id == "mem_x" || e.target_id == "mem_x")
        );
    }

    #[tokio::test]
    async fn set_edge_status_updates() {
        let (s, _dir) = make_store().await;
        s.store(&test_row("mem_p", "P", "body", &[])).await.unwrap();
        s.store(&test_row("mem_q", "Q", "body", &[])).await.unwrap();
        let edge = s.create_edge("mem_p", "mem_q", "pairs_with").await.unwrap();
        let crate::model::EdgeCreate::Created(edge_id) = edge else {
            panic!("expected EdgeCreate::Created");
        };
        let ok = s.set_edge_status(&edge_id, "inactive").await.unwrap();
        assert!(ok);
        let edges = s.list_edges(None).await.unwrap();
        let updated = edges.iter().find(|e| e.id == edge_id).unwrap();
        assert_eq!(updated.status, "inactive");
    }

    #[tokio::test]
    async fn set_edge_status_returns_false_for_missing() {
        let (s, _dir) = make_store().await;
        let ok = s
            .set_edge_status("edge_nonexistent", "inactive")
            .await
            .unwrap();
        assert!(!ok);
    }

    #[tokio::test]
    async fn create_edge_reports_duplicate_and_missing_endpoint() {
        use crate::model::EdgeCreate;
        let (s, _dir) = make_store().await;
        let tags: Vec<String> = vec![];
        s.store(&test_row("mem_1", "A", "a", &tags)).await.unwrap();
        s.store(&test_row("mem_2", "B", "b", &tags)).await.unwrap();

        let first = s.create_edge("mem_1", "mem_2", "related_to").await.unwrap();
        assert!(matches!(first, EdgeCreate::Created(_)));
        // duplicate, even reversed
        assert_eq!(
            s.create_edge("mem_2", "mem_1", "related_to").await.unwrap(),
            EdgeCreate::Duplicate
        );
        assert_eq!(
            s.create_edge("mem_1", "mem_ghost", "related_to")
                .await
                .unwrap(),
            EdgeCreate::MissingEndpoint
        );
        assert_eq!(
            s.create_edge("mem_1", "mem_2", "banana").await.unwrap(),
            EdgeCreate::InvalidRelationship
        );
    }

    #[tokio::test]
    async fn list_feedback_filtered_by_memory_id() {
        let (s, _dir) = make_store().await;
        s.store(&test_row("mem_f1", "F1", "body", &[]))
            .await
            .unwrap();
        s.store(&test_row("mem_f2", "F2", "body", &[]))
            .await
            .unwrap();
        s.create_feedback("mem_f1", "positive", None).await.unwrap();
        s.create_feedback("mem_f2", "negative", Some("outdated"))
            .await
            .unwrap();

        let all = s.list_feedback(None, None).await.unwrap();
        let filtered = s.list_feedback(Some("mem_f1"), None).await.unwrap();

        assert_eq!(all.len(), 2);
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].memory_id, "mem_f1");
    }

    #[tokio::test]
    async fn set_feedback_status_updates() {
        let (s, _dir) = make_store().await;
        s.store(&test_row("mem_g", "G", "body", &[])).await.unwrap();
        let fb = s.create_feedback("mem_g", "negative", None).await.unwrap();
        let ok = s.set_feedback_status(&fb.id, "resolved").await.unwrap();
        assert!(ok);
        let items = s.list_feedback(Some("mem_g"), None).await.unwrap();
        assert_eq!(items[0].status, "resolved");
    }

    #[tokio::test]
    async fn set_feedback_status_returns_false_for_missing() {
        let (s, _dir) = make_store().await;
        let ok = s
            .set_feedback_status("fb_nonexistent", "resolved")
            .await
            .unwrap();
        assert!(!ok);
    }

    #[tokio::test]
    async fn list_conflicts_returns_entries() {
        let (s, _dir) = make_store().await;
        s.store(&test_row("mem_h", "H", "local", &[]))
            .await
            .unwrap();
        let entry = s.recall_by_id("mem_h").await.unwrap().unwrap();
        s.write_conflict(
            "mem_h",
            "remote",
            "local",
            entry.updated_at + 1,
            entry.updated_at,
        )
        .await
        .unwrap();
        let conflicts = s.list_conflicts(None).await.unwrap();
        assert_eq!(conflicts.len(), 1);
        assert_eq!(conflicts[0].memory_id, "mem_h");
    }

    #[tokio::test]
    async fn get_conflict_by_id_returns_none_for_missing() {
        let (s, _dir) = make_store().await;
        let result = s.get_conflict_by_id("conflict_nonexistent").await.unwrap();
        assert!(result.is_none());
    }

    fn test_row<'a>(
        id: &'a str,
        title: &'a str,
        content: &'a str,
        tags: &'a [String],
    ) -> NewMemoryRow<'a> {
        NewMemoryRow {
            id,
            title,
            content,
            tags,
            token_count: None,
            layer: "workspace",
            memory_type: "project",
        }
    }

    #[tokio::test]
    async fn store_persists_layer_and_memory_type() {
        let (s, _dir) = make_store().await;
        s.store(&NewMemoryRow {
            id: "mem_l1",
            title: "pref",
            content: "body",
            tags: &[],
            token_count: None,
            layer: "personal",
            memory_type: "preference",
        })
        .await
        .unwrap();
        let e = s.recall_by_id("mem_l1").await.unwrap().unwrap();
        assert_eq!(e.layer, "personal");
        assert_eq!(e.memory_type, "preference");
    }

    #[tokio::test]
    async fn store_computes_token_count_when_missing() {
        let (s, _dir) = make_store().await;
        let tags: Vec<String> = vec![];
        s.store(&test_row(
            "mem_tc",
            "title here",
            "some content words",
            &tags,
        ))
        .await
        .unwrap();
        let e = s.recall_by_id("mem_tc").await.unwrap().unwrap();
        assert!(e.token_count.unwrap() > 0);
    }

    #[tokio::test]
    async fn auto_edges_are_not_duplicated_in_reverse() {
        let (s, _dir) = make_store().await;
        let tags = vec!["shared".to_string()];
        s.store(&test_row("mem_r1", "A", "a", &tags)).await.unwrap();
        s.store(&test_row("mem_r2", "B", "b", &tags)).await.unwrap();
        // re-storing the first must not create a reverse duplicate edge
        s.store(&test_row("mem_r1", "A", "a2", &tags))
            .await
            .unwrap();
        let edges = s.list_edges(None).await.unwrap();
        assert_eq!(
            edges.len(),
            1,
            "one shares_tag edge between the pair, either direction"
        );
    }

    #[tokio::test]
    async fn update_changes_title_and_recounts_tokens() {
        let (s, _dir) = make_store().await;
        let tags: Vec<String> = vec![];
        s.store(&test_row("mem_t", "old title", "short", &tags))
            .await
            .unwrap();
        let before = s.recall_by_id("mem_t").await.unwrap().unwrap();
        let long = "much longer content ".repeat(50);
        let ok = s.update("mem_t", "new title", &long, &tags).await.unwrap();
        assert!(ok);
        let after = s.recall_by_id("mem_t").await.unwrap().unwrap();
        assert_eq!(after.title, "new title");
        assert!(after.token_count.unwrap() > before.token_count.unwrap());
    }

    #[test]
    fn fts_quote_wraps_terms_and_escapes_quotes() {
        assert_eq!(fts_quote("project/myapp"), "\"project/myapp\"");
        assert_eq!(fts_quote("c++ tips"), "\"c++\" \"tips\"");
        assert_eq!(fts_quote("say \"hi\""), "\"say\" \"\"\"hi\"\"\"");
        assert_eq!(fts_quote("   "), "");
    }

    #[tokio::test]
    async fn search_tolerates_fts_special_characters() {
        let (s, _dir) = make_store().await;
        s.store(&test_row("mem_sp", "project/myapp", "slash content", &[]))
            .await
            .unwrap();
        // must not error, and exact-ish term still matches via quoted FTS
        let results = s.search("project/myapp", 10).await.unwrap();
        assert_eq!(results.len(), 1);
        // pure syntax garbage must not error either
        assert!(s.search("c++ ((", 10).await.unwrap().len() <= 1);
    }

    #[tokio::test]
    async fn resolve_recall_returns_empty_for_unmatched_special_title() {
        let (s, _dir) = make_store().await;
        let r = s.resolve_recall("does/not/exist").await.unwrap();
        assert!(r.is_empty());
    }

    #[tokio::test]
    async fn detect_conflicts_records_overwritten_local_write() {
        let (s, _dir) = make_store().await;
        let tags: Vec<String> = vec![];
        s.store(&test_row("mem_c", "C", "local version", &tags))
            .await
            .unwrap();
        let journal = s.take_journal().await.unwrap();
        assert_eq!(journal.len(), 1);
        // simulate remote frames landing during sync: content replaced out of band
        s.conn.execute(
            "UPDATE memories SET content = 'remote version', updated_at = updated_at + 10 WHERE id = 'mem_c'", (),
        ).await.unwrap();
        let found = s.detect_conflicts(&journal).await.unwrap();
        assert_eq!(found, 1);
        let conflicts = s.list_conflicts(Some("pending")).await.unwrap();
        assert_eq!(conflicts[0].remote_content, "remote version");
        assert_eq!(conflicts[0].local_content, "local version");
        assert!(
            s.take_journal().await.unwrap().is_empty(),
            "journal cleared after detection"
        );
    }

    #[tokio::test]
    async fn detect_conflicts_is_silent_when_local_write_survived() {
        let (s, _dir) = make_store().await;
        let tags: Vec<String> = vec![];
        s.store(&test_row("mem_ok", "OK", "content", &tags))
            .await
            .unwrap();
        let journal = s.take_journal().await.unwrap();
        let found = s.detect_conflicts(&journal).await.unwrap();
        assert_eq!(found, 0);
        assert!(s.list_conflicts(Some("pending")).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn resolve_keep_local_restores_content() {
        let (s, _dir) = make_store().await;
        let tags: Vec<String> = vec![];
        s.store(&test_row("mem_r", "R", "remote won", &tags))
            .await
            .unwrap();
        let c = s
            .write_conflict("mem_r", "remote won", "my local text", 20, 10)
            .await
            .unwrap();
        assert!(s.resolve_conflict(&c.id, "keep_local").await.unwrap());
        let mem = s.recall_by_id("mem_r").await.unwrap().unwrap();
        assert_eq!(mem.content, "my local text");
        let after = s.get_conflict_by_id(&c.id).await.unwrap().unwrap();
        assert_eq!(after.status, "keep_local");
    }

    #[tokio::test]
    async fn meta_roundtrip() {
        let (s, _dir) = make_store().await;
        assert!(s.get_meta("last_synced_at").await.unwrap().is_none());
        s.set_meta("last_synced_at", "1234").await.unwrap();
        assert_eq!(s.get_meta("last_synced_at").await.unwrap().unwrap(), "1234");
    }
}
