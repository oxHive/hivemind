use std::sync::Mutex;
use anyhow::Result;
use rusqlite::{Connection, OptionalExtension};
use crate::model::{NewMemory, MemoryEntry, Layer, MemoryType, StoreResult, Edge, EdgeCreate, FeedbackItem, ConflictItem};

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

    /// Newest-first listing. `rowid DESC` breaks same-second timestamp ties.
    pub fn list_memories(&self, layer: Option<Layer>, limit: usize) -> Result<Vec<MemoryEntry>> {
        let conn = self.conn.lock().map_err(|_| anyhow::anyhow!("db connection mutex poisoned"))?;
        let base = "SELECT id, layer, type, title, content, source, project, created_at, updated_at
                    FROM memories";
        let rows = match &layer {
            Some(l) => {
                let mut stmt = conn.prepare(
                    &format!("{base} WHERE layer = ?1 ORDER BY updated_at DESC, rowid DESC LIMIT ?2"))?;
                stmt.query_map(rusqlite::params![l.to_string(), limit as i64], Self::row_tuple)?
                    .collect::<rusqlite::Result<Vec<_>>>()?
            }
            None => {
                let mut stmt = conn.prepare(
                    &format!("{base} ORDER BY updated_at DESC, rowid DESC LIMIT ?1"))?;
                stmt.query_map(rusqlite::params![limit as i64], Self::row_tuple)?
                    .collect::<rusqlite::Result<Vec<_>>>()?
            }
        };
        let mut entries = Vec::with_capacity(rows.len());
        for (rid, layer_s, type_s, title, content, source, project, created_at, updated_at) in rows {
            entries.push(Self::row_to_entry(
                &conn, rid, layer_s, type_s, title, content, source, project, created_at, updated_at)?);
        }
        Ok(entries)
    }

    pub fn list_edges(&self, status: Option<&str>) -> Result<Vec<Edge>> {
        let conn = self.conn.lock().map_err(|_| anyhow::anyhow!("db connection mutex poisoned"))?;
        let base = "SELECT id, source_id, target_id, relationship, weight, inferred_by, status, reason, created_at, updated_at
                    FROM edges";
        let map = |row: &rusqlite::Row| -> rusqlite::Result<Edge> {
            Ok(Edge {
                id: row.get(0)?,
                source_id: row.get(1)?,
                target_id: row.get(2)?,
                relationship: row.get(3)?,
                weight: row.get(4)?,
                inferred_by: row.get(5)?,
                status: row.get(6)?,
                reason: row.get(7)?,
                created_at: row.get(8)?,
                updated_at: row.get(9)?,
            })
        };
        let edges = match status {
            Some(st) => {
                let mut stmt = conn.prepare(&format!("{base} WHERE status = ?1 ORDER BY created_at DESC, rowid DESC"))?;
                stmt.query_map(rusqlite::params![st], map)?.collect::<rusqlite::Result<Vec<_>>>()?
            }
            None => {
                let mut stmt = conn.prepare(&format!("{base} ORDER BY created_at DESC, rowid DESC"))?;
                stmt.query_map([], map)?.collect::<rusqlite::Result<Vec<_>>>()?
            }
        };
        Ok(edges)
    }

    /// Create a user-confirmed (manual, accepted) edge between two memories.
    pub fn create_edge(&self, source_id: &str, target_id: &str, relationship: &str) -> Result<EdgeCreate> {
        let now = now_secs();
        let conn = self.conn.lock().map_err(|_| anyhow::anyhow!("db connection mutex poisoned"))?;
        for id in [source_id, target_id] {
            let exists: i64 = conn.query_row(
                "SELECT EXISTS(SELECT 1 FROM memories WHERE id = ?1)",
                rusqlite::params![id], |r| r.get(0))?;
            if exists == 0 {
                return Ok(EdgeCreate::MissingEndpoint);
            }
        }
        let edge_id = format!("edge_{}", uuid::Uuid::new_v4().simple());
        let n = conn.execute(
            "INSERT OR IGNORE INTO edges
             (id, source_id, target_id, relationship, weight, inferred_by, status, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, 1.0, 'manual', 'accepted', ?5, ?5)",
            rusqlite::params![edge_id, source_id, target_id, relationship, now],
        )?;
        if n == 0 {
            Ok(EdgeCreate::Duplicate)
        } else {
            Ok(EdgeCreate::Created(edge_id))
        }
    }

    pub fn set_edge_status(&self, id: &str, status: &str) -> Result<bool> {
        let conn = self.conn.lock().map_err(|_| anyhow::anyhow!("db connection mutex poisoned"))?;
        let n = conn.execute(
            "UPDATE edges SET status = ?1, updated_at = ?2 WHERE id = ?3",
            rusqlite::params![status, now_secs(), id],
        )?;
        Ok(n > 0)
    }

    /// Returns Ok(None) when a referenced memory/edge does not exist.
    pub fn create_feedback(
        &self,
        memory_id: Option<&str>,
        edge_id: Option<&str>,
        kind: &str,
        note: Option<&str>,
    ) -> Result<Option<String>> {
        let conn = self.conn.lock().map_err(|_| anyhow::anyhow!("db connection mutex poisoned"))?;
        if let Some(mid) = memory_id {
            let exists: i64 = conn.query_row(
                "SELECT EXISTS(SELECT 1 FROM memories WHERE id = ?1)",
                rusqlite::params![mid], |r| r.get(0))?;
            if exists == 0 { return Ok(None); }
        }
        if let Some(eid) = edge_id {
            let exists: i64 = conn.query_row(
                "SELECT EXISTS(SELECT 1 FROM edges WHERE id = ?1)",
                rusqlite::params![eid], |r| r.get(0))?;
            if exists == 0 { return Ok(None); }
        }
        let id = format!("fb_{}", uuid::Uuid::new_v4().simple());
        conn.execute(
            "INSERT INTO feedback (id, memory_id, edge_id, type, note, status, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, 'open', ?6)",
            rusqlite::params![id, memory_id, edge_id, kind, note, now_secs()],
        )?;
        Ok(Some(id))
    }

    pub fn list_feedback(&self, status: Option<&str>) -> Result<Vec<FeedbackItem>> {
        let conn = self.conn.lock().map_err(|_| anyhow::anyhow!("db connection mutex poisoned"))?;
        let base = "SELECT id, memory_id, edge_id, type, note, status, created_at FROM feedback";
        let map = |row: &rusqlite::Row| -> rusqlite::Result<FeedbackItem> {
            Ok(FeedbackItem {
                id: row.get(0)?,
                memory_id: row.get(1)?,
                edge_id: row.get(2)?,
                kind: row.get(3)?,
                note: row.get(4)?,
                status: row.get(5)?,
                created_at: row.get(6)?,
            })
        };
        let items = match status {
            Some(st) => {
                let mut stmt = conn.prepare(&format!("{base} WHERE status = ?1 ORDER BY created_at DESC, rowid DESC"))?;
                stmt.query_map(rusqlite::params![st], map)?.collect::<rusqlite::Result<Vec<_>>>()?
            }
            None => {
                let mut stmt = conn.prepare(&format!("{base} ORDER BY created_at DESC, rowid DESC"))?;
                stmt.query_map([], map)?.collect::<rusqlite::Result<Vec<_>>>()?
            }
        };
        Ok(items)
    }

    pub fn set_feedback_status(&self, id: &str, status: &str) -> Result<bool> {
        let conn = self.conn.lock().map_err(|_| anyhow::anyhow!("db connection mutex poisoned"))?;
        let n = conn.execute(
            "UPDATE feedback SET status = ?1 WHERE id = ?2",
            rusqlite::params![status, id],
        )?;
        Ok(n > 0)
    }

    pub fn get_kv(&self, key: &str) -> Result<Option<String>> {
        let conn = self.conn.lock().map_err(|_| anyhow::anyhow!("db mutex poisoned"))?;
        let mut stmt = conn.prepare("SELECT value FROM kv WHERE key = ?1")?;
        match stmt.query_row(rusqlite::params![key], |r| r.get(0)) {
            Ok(v) => Ok(Some(v)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    pub fn set_kv(&self, key: &str, value: &str) -> Result<()> {
        let conn = self.conn.lock().map_err(|_| anyhow::anyhow!("db mutex poisoned"))?;
        conn.execute(
            "INSERT INTO kv (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            rusqlite::params![key, value],
        )?;
        Ok(())
    }

    #[allow(dead_code)]
    pub fn write_conflict(
        &self,
        memory_id: Option<&str>,
        winner: &str,
        loser: &str,
        winner_src: &str,
        loser_src: &str,
    ) -> Result<String> {
        let id = format!("cfl_{}", uuid::Uuid::new_v4().simple());
        let now = now_secs();
        let conn = self.conn.lock().map_err(|_| anyhow::anyhow!("db mutex poisoned"))?;
        conn.execute(
            "INSERT INTO conflicts (id, memory_id, winner, loser, winner_src, loser_src, detected_at, status)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'open')",
            rusqlite::params![id, memory_id, winner, loser, winner_src, loser_src, now],
        )?;
        Ok(id)
    }

    pub fn resolve_conflict(&self, id: &str, action: &str) -> Result<bool> {
        let conn = self.conn.lock().map_err(|_| anyhow::anyhow!("db mutex poisoned"))?;
        if action == "restore" {
            let row: Option<(Option<String>, String)> = {
                let mut stmt = conn.prepare(
                    "SELECT memory_id, loser FROM conflicts WHERE id = ?1 AND status = 'open'"
                )?;
                stmt.query_row(rusqlite::params![id], |r| Ok((r.get(0)?, r.get(1)?)))
                    .optional()?
            };
            if let Some((Some(memory_id), loser_content)) = row {
                conn.execute(
                    "UPDATE memories SET content = ?1, updated_at = ?2 WHERE id = ?3",
                    rusqlite::params![loser_content, now_secs(), memory_id],
                )?;
            }
        }
        let new_status = if action == "restore" { "restored" } else { "resolved" };
        let n = conn.execute(
            "UPDATE conflicts SET status = ?1 WHERE id = ?2 AND status = 'open'",
            rusqlite::params![new_status, id],
        )?;
        Ok(n > 0)
    }

    /// Conflicts are written by Phase 5 sync; the dashboard already reads them.
    pub fn list_conflicts(&self, status: Option<&str>) -> Result<Vec<ConflictItem>> {
        let conn = self.conn.lock().map_err(|_| anyhow::anyhow!("db connection mutex poisoned"))?;
        let map = |row: &rusqlite::Row| -> rusqlite::Result<ConflictItem> {
            Ok(ConflictItem {
                id: row.get(0)?,
                memory_id: row.get(1)?,
                winner: row.get(2)?,
                loser: row.get(3)?,
                winner_src: row.get(4)?,
                loser_src: row.get(5)?,
                detected_at: row.get(6)?,
                status: row.get(7)?,
                title: row.get(8)?,
            })
        };
        let base = "SELECT c.id, c.memory_id, c.winner, c.loser, c.winner_src, c.loser_src,
                           c.detected_at, c.status, m.title
                    FROM conflicts c LEFT JOIN memories m ON c.memory_id = m.id";
        let items = match status {
            Some(st) => {
                let sql = format!("{base} WHERE c.status = ?1 ORDER BY c.detected_at DESC, c.rowid DESC");
                let mut stmt = conn.prepare(&sql)?;
                stmt.query_map(rusqlite::params![st], map)?.collect::<rusqlite::Result<Vec<_>>>()?
            }
            None => {
                let sql = format!("{base} ORDER BY c.detected_at DESC, c.rowid DESC");
                let mut stmt = conn.prepare(&sql)?;
                stmt.query_map([], map)?.collect::<rusqlite::Result<Vec<_>>>()?
            }
        };
        Ok(items)
    }

    pub fn memories_since(&self, since_ts: i64) -> Result<Vec<MemoryEntry>> {
        let conn = self.conn.lock().map_err(|_| anyhow::anyhow!("db mutex poisoned"))?;
        let mut stmt = conn.prepare(
            "SELECT id, layer, type, title, content, source, project, created_at, updated_at
             FROM memories WHERE updated_at > ?1 ORDER BY updated_at ASC"
        )?;
        let rows = stmt.query_map(rusqlite::params![since_ts], Self::row_tuple)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let mut entries = Vec::with_capacity(rows.len());
        for (rid, layer_s, type_s, title, content, source, project, created_at, updated_at) in rows {
            entries.push(Self::row_to_entry(&conn, rid, layer_s, type_s, title, content, source, project, created_at, updated_at)?);
        }
        Ok(entries)
    }

    pub fn upsert_memory(&self, entry: &crate::model::MemoryEntry) -> Result<Option<crate::model::ConflictItem>> {
        let now = now_secs();
        let conn = self.conn.lock().map_err(|_| anyhow::anyhow!("db mutex poisoned"))?;
        let tx = conn.unchecked_transaction()?;

        let existing: Option<(i64, String, String)> = tx.query_row(
            "SELECT updated_at, content, title FROM memories WHERE id = ?1",
            rusqlite::params![entry.id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        ).optional()?;

        let conflict = if let Some((local_ts, local_content, local_title)) = existing {
            if entry.updated_at == local_ts {
                tx.commit()?;
                return Ok(None);
            }
            let (winner, loser, winner_src, loser_src) = if entry.updated_at > local_ts {
                (entry.content.as_str(), local_content.as_str(), "remote", "local")
            } else {
                (local_content.as_str(), entry.content.as_str(), "local", "remote")
            };
            if entry.updated_at > local_ts {
                tx.execute(
                    "UPDATE memories SET layer=?1, type=?2, title=?3, content=?4, source=?5, project=?6, updated_at=?7 WHERE id=?8",
                    rusqlite::params![
                        entry.layer.to_string(), entry.memory_type.to_string(),
                        entry.title, entry.content, entry.source, entry.project,
                        entry.updated_at, entry.id,
                    ],
                )?;
                tx.execute("DELETE FROM tags WHERE memory_id = ?1", rusqlite::params![entry.id])?;
                for t in &entry.tags {
                    tx.execute("INSERT INTO tags (memory_id, tag) VALUES (?1, ?2)", rusqlite::params![entry.id, t])?;
                }
            }
            let cfl_id = format!("cfl_{}", uuid::Uuid::new_v4().simple());
            tx.execute(
                "INSERT INTO conflicts (id, memory_id, winner, loser, winner_src, loser_src, detected_at, status)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'open')",
                rusqlite::params![cfl_id, entry.id, winner, loser, winner_src, loser_src, now],
            )?;
            Some(crate::model::ConflictItem {
                id: cfl_id,
                memory_id: Some(entry.id.clone()),
                winner: winner.to_string(),
                loser: loser.to_string(),
                winner_src: winner_src.to_string(),
                loser_src: loser_src.to_string(),
                detected_at: now,
                status: "open".to_string(),
                title: Some(local_title),
            })
        } else {
            tx.execute(
                "INSERT INTO memories (id, layer, type, title, content, source, project, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                rusqlite::params![
                    entry.id, entry.layer.to_string(), entry.memory_type.to_string(),
                    entry.title, entry.content, entry.source, entry.project,
                    entry.created_at, entry.updated_at,
                ],
            )?;
            for t in &entry.tags {
                tx.execute("INSERT INTO tags (memory_id, tag) VALUES (?1, ?2)", rusqlite::params![entry.id, t])?;
            }
            None
        };

        tx.commit()?;
        Ok(conflict)
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

    #[allow(clippy::type_complexity)]
    fn row_tuple(row: &rusqlite::Row) -> rusqlite::Result<(
        String, String, String, String, String,
        Option<String>, Option<String>, i64, i64,
    )> {
        Ok((
            row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?,
            row.get(5)?, row.get(6)?, row.get(7)?, row.get(8)?,
        ))
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

    #[test]
    fn list_memories_returns_newest_first_with_tags() {
        let s = open_test_store();
        s.store(sample()).unwrap();
        s.store(NewMemory {
            title: "second".to_string(),
            content: "newer entry".to_string(),
            layer: Layer::Workspace,
            memory_type: MemoryType::Project,
            tags: vec!["t".to_string()],
            project: Some("proj".to_string()),
            source: None,
        }).unwrap();
        let list = s.list_memories(None, 50).unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].title, "second", "newest first");
        assert_eq!(list[1].tags.len(), 2, "tags hydrated");
    }

    #[test]
    fn list_memories_filters_by_layer_and_respects_limit() {
        let s = open_test_store();
        s.store(sample()).unwrap(); // personal
        for i in 0..2 {
            s.store(NewMemory {
                title: format!("ws {i}"),
                content: "workspace entry".to_string(),
                layer: Layer::Workspace,
                memory_type: MemoryType::Project,
                tags: vec![],
                project: Some("p".to_string()),
                source: None,
            }).unwrap();
        }
        let ws = s.list_memories(Some(Layer::Workspace), 50).unwrap();
        assert_eq!(ws.len(), 2);
        assert!(ws.iter().all(|e| e.layer == Layer::Workspace));
        assert_eq!(s.list_memories(None, 1).unwrap().len(), 1);
    }

    #[test]
    fn create_edge_creates_accepted_manual_edge() {
        let s = open_test_store();
        let a = s.store(sample()).unwrap().id;
        let b = s.store(NewMemory {
            title: "other".to_string(), content: "x".to_string(),
            layer: Layer::Personal, memory_type: MemoryType::Preference,
            tags: vec![], project: None, source: None,
        }).unwrap().id;
        let created = s.create_edge(&a, &b, "pairs_with").unwrap();
        let id = match created {
            crate::model::EdgeCreate::Created(id) => id,
            other => panic!("expected Created, got {other:?}"),
        };
        let edges = s.list_edges(None).unwrap();
        let e = edges.iter().find(|e| e.id == id).unwrap();
        assert_eq!(e.relationship, "pairs_with");
        assert_eq!(e.inferred_by, "manual");
        assert_eq!(e.status, "accepted");
    }

    #[test]
    fn create_edge_detects_duplicate_and_missing_endpoint() {
        let s = open_test_store();
        let a = s.store(sample()).unwrap().id;
        let b = s.store(NewMemory {
            title: "other".to_string(), content: "x".to_string(),
            layer: Layer::Personal, memory_type: MemoryType::Preference,
            tags: vec![], project: None, source: None,
        }).unwrap().id;
        s.create_edge(&a, &b, "pairs_with").unwrap();
        assert_eq!(s.create_edge(&a, &b, "pairs_with").unwrap(), crate::model::EdgeCreate::Duplicate);
        assert_eq!(s.create_edge(&a, "mem_nope", "pairs_with").unwrap(), crate::model::EdgeCreate::MissingEndpoint);
    }

    #[test]
    fn list_edges_filters_by_status_and_set_edge_status_flips() {
        let s = open_test_store();
        s.store(sample()).unwrap();
        // Sharing the "golang" tag auto-creates an accepted edge.
        s.store(NewMemory {
            title: "go http".to_string(), content: "chi router".to_string(),
            layer: Layer::Personal, memory_type: MemoryType::Preference,
            tags: vec!["golang".to_string()], project: None, source: None,
        }).unwrap();
        let accepted = s.list_edges(Some("accepted")).unwrap();
        assert_eq!(accepted.len(), 1);
        assert!(s.list_edges(Some("pending")).unwrap().is_empty());

        let id = accepted[0].id.clone();
        assert!(s.set_edge_status(&id, "rejected").unwrap());
        assert!(s.list_edges(Some("accepted")).unwrap().is_empty());
        assert_eq!(s.list_edges(Some("rejected")).unwrap().len(), 1);
        assert!(!s.set_edge_status("edge_nope", "accepted").unwrap());
    }

    #[test]
    fn feedback_roundtrip_and_status_filter() {
        let s = open_test_store();
        let mem = s.store(sample()).unwrap().id;
        let id = s.create_feedback(Some(&mem), None, "outdated", Some("pgx is now v6"))
            .unwrap().expect("memory exists");
        assert!(id.starts_with("fb_"), "id was {id}");

        let open = s.list_feedback(Some("open")).unwrap();
        assert_eq!(open.len(), 1);
        assert_eq!(open[0].kind, "outdated");
        assert_eq!(open[0].memory_id.as_deref(), Some(mem.as_str()));

        assert!(s.set_feedback_status(&id, "resolved").unwrap());
        assert!(s.list_feedback(Some("open")).unwrap().is_empty());
        assert_eq!(s.list_feedback(None).unwrap().len(), 1);
        assert!(!s.set_feedback_status("fb_nope", "resolved").unwrap());
    }

    #[test]
    fn create_feedback_rejects_missing_memory() {
        let s = open_test_store();
        assert!(s.create_feedback(Some("mem_nope"), None, "incorrect", None).unwrap().is_none());
    }

    #[test]
    fn list_conflicts_is_empty_pre_sync() {
        let s = open_test_store();
        assert!(s.list_conflicts(None).unwrap().is_empty());
    }

    #[test]
    fn get_kv_returns_none_when_key_missing() {
        let s = open_test_store();
        assert!(s.get_kv("not_set").unwrap().is_none());
    }

    #[test]
    fn set_and_get_kv_roundtrip() {
        let s = open_test_store();
        s.set_kv("last_synced_at", "1000000000").unwrap();
        assert_eq!(s.get_kv("last_synced_at").unwrap().as_deref(), Some("1000000000"));
        // upsert overwrites
        s.set_kv("last_synced_at", "2000000000").unwrap();
        assert_eq!(s.get_kv("last_synced_at").unwrap().as_deref(), Some("2000000000"));
    }

    #[test]
    fn write_conflict_creates_open_record() {
        let s = open_test_store();
        let id = s.write_conflict(None, "remote content", "local content", "remote", "local").unwrap();
        assert!(id.starts_with("cfl_"));
        let conflicts = s.list_conflicts(Some("open")).unwrap();
        assert_eq!(conflicts.len(), 1);
        assert_eq!(conflicts[0].winner, "remote content");
        assert_eq!(conflicts[0].loser, "local content");
        assert_eq!(conflicts[0].status, "open");
    }

    #[test]
    fn resolve_conflict_keep_marks_resolved() {
        let s = open_test_store();
        let id = s.write_conflict(None, "w", "l", "remote", "local").unwrap();
        assert!(s.resolve_conflict(&id, "keep").unwrap());
        let open = s.list_conflicts(Some("open")).unwrap();
        assert!(open.is_empty());
        let resolved = s.list_conflicts(Some("resolved")).unwrap();
        assert_eq!(resolved.len(), 1);
    }

    #[test]
    fn resolve_conflict_restore_updates_memory_content() {
        let s = open_test_store();
        let mem_id = s.store(crate::model::NewMemory {
            title: "t".into(), content: "new content".into(),
            layer: crate::model::Layer::Personal,
            memory_type: crate::model::MemoryType::Preference,
            tags: vec![], project: None, source: None,
        }).unwrap().id;
        let cfl_id = s.write_conflict(Some(&mem_id), "new content", "old content", "remote", "local").unwrap();
        assert!(s.resolve_conflict(&cfl_id, "restore").unwrap());
        let mem = s.recall_by_id(&mem_id).unwrap().unwrap();
        assert_eq!(mem.content, "old content", "restore should write the loser content back");
        let restored = s.list_conflicts(Some("restored")).unwrap();
        assert_eq!(restored.len(), 1);
    }

    #[test]
    fn resolve_conflict_returns_false_for_missing_id() {
        let s = open_test_store();
        assert!(!s.resolve_conflict("cfl_nonexistent", "keep").unwrap());
    }

    #[test]
    fn list_conflicts_filters_by_status() {
        let s = open_test_store();
        s.write_conflict(None, "w", "l", "remote", "local").unwrap();
        let id2 = s.write_conflict(None, "w2", "l2", "remote", "local").unwrap();
        s.resolve_conflict(&id2, "keep").unwrap();
        assert_eq!(s.list_conflicts(Some("open")).unwrap().len(), 1);
        assert_eq!(s.list_conflicts(Some("resolved")).unwrap().len(), 1);
        assert_eq!(s.list_conflicts(None).unwrap().len(), 2);
    }

    fn remote_entry(id: &str, title: &str, content: &str, updated_at: i64) -> crate::model::MemoryEntry {
        crate::model::MemoryEntry {
            id: id.to_string(),
            layer: crate::model::Layer::Personal,
            memory_type: crate::model::MemoryType::Preference,
            title: title.to_string(),
            content: content.to_string(),
            source: Some("remote".to_string()),
            project: None,
            tags: vec!["test".to_string()],
            created_at: updated_at - 10,
            updated_at,
        }
    }

    #[test]
    fn memories_since_returns_empty_when_no_new_records() {
        let s = open_test_store();
        let _id = s.store(sample()).unwrap().id;
        let later = now_secs() + 10;
        let result = s.memories_since(later).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn memories_since_returns_records_after_timestamp() {
        let s = open_test_store();
        let _id = s.store(sample()).unwrap().id;
        let ts = now_secs() - 1;
        let result = s.memories_since(ts).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].title, "golang preferences");
    }

    #[test]
    fn upsert_memory_inserts_new_memory_without_conflict() {
        let s = open_test_store();
        let entry = remote_entry("mem_abc", "Remote Title", "Remote content", now_secs());
        let conflict = s.upsert_memory(&entry).unwrap();
        assert!(conflict.is_none(), "new memory should not produce a conflict");
        let recalled = s.recall_by_id("mem_abc").unwrap().unwrap();
        assert_eq!(recalled.title, "Remote Title");
        assert_eq!(recalled.tags, vec!["test"]);
    }

    #[test]
    fn upsert_memory_remote_wins_when_newer() {
        let s = open_test_store();
        let local_id = s.store(crate::model::NewMemory {
            title: "Local title".into(), content: "Local content".into(),
            layer: crate::model::Layer::Personal,
            memory_type: crate::model::MemoryType::Preference,
            tags: vec![], project: None, source: Some("local".to_string()),
        }).unwrap().id;

        let remote_ts = now_secs() + 100;
        let entry = remote_entry(&local_id, "Remote title", "Remote content", remote_ts);
        let conflict = s.upsert_memory(&entry).unwrap();

        let c = conflict.expect("conflict expected when remote wins");
        assert_eq!(c.winner, "Remote content");
        assert_eq!(c.loser, "Local content");
        assert_eq!(c.winner_src, "remote");
        assert_eq!(c.loser_src, "local");

        let recalled = s.recall_by_id(&local_id).unwrap().unwrap();
        assert_eq!(recalled.content, "Remote content", "memory should be updated to winner");
    }

    #[test]
    fn upsert_memory_local_wins_when_newer() {
        let s = open_test_store();
        let local_id = s.store(crate::model::NewMemory {
            title: "Local title".into(), content: "Local content".into(),
            layer: crate::model::Layer::Personal,
            memory_type: crate::model::MemoryType::Preference,
            tags: vec![], project: None, source: Some("local".to_string()),
        }).unwrap().id;

        let entry = remote_entry(&local_id, "Remote title", "Remote content", 1);
        let conflict = s.upsert_memory(&entry).unwrap();

        let c = conflict.expect("conflict expected even when local wins");
        assert_eq!(c.winner_src, "local");
        assert_eq!(c.loser_src, "remote");

        let recalled = s.recall_by_id(&local_id).unwrap().unwrap();
        assert_eq!(recalled.content, "Local content", "local content should be unchanged");
    }

    #[test]
    fn upsert_memory_no_conflict_when_timestamps_equal() {
        let s = open_test_store();
        let ts = now_secs();
        let local_id = {
            let conn = s.conn.lock().unwrap();
            let id = format!("mem_{}", uuid::Uuid::new_v4().simple());
            conn.execute(
                "INSERT INTO memories (id, layer, type, title, content, created_at, updated_at) VALUES (?1,'personal','preference','T','C',?2,?2)",
                rusqlite::params![id, ts],
            ).unwrap();
            id
        };
        let entry = remote_entry(&local_id, "T", "C", ts);
        let conflict = s.upsert_memory(&entry).unwrap();
        assert!(conflict.is_none(), "same timestamp = no conflict");
    }
}
