use anyhow::{Result, anyhow};
use libsql::{Connection, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

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
    pub link_text: Option<String>,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelatedMemory {
    pub id: String,
    pub title: String,
    pub link_text: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupedEdges {
    pub parents: Vec<RelatedMemory>,
    pub children: Vec<RelatedMemory>,
    pub siblings: Vec<RelatedMemory>,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionStartLogRow {
    pub id: String,
    pub project_name: String,
    pub project_path: String,
    pub created_at: i64,
    pub max_tokens: i64,
    pub used_tokens: i64,
    pub memories_recalled: i64,
    pub truncated: bool,
    pub loaded: Value,
    pub skipped: Value,
}

pub struct JournalRow {
    pub memory_id: String,
    pub content: String,
    pub updated_at: i64,
}

pub struct SqliteStore {
    pub(crate) conn: Connection,
}

pub const VALID_RELATIONSHIPS: &[&str] = &["parent", "child", "sibling"];

static RELATIONSHIP_LINK_RE: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
    regex::Regex::new(r"\[([^\]]+)\]\((?:(parent|child|sibling):)?(mem_[0-9a-f]{32})\)").unwrap()
});

/// Extract `[phrase](kind:mem_xxx)` relationship links from memory content
/// (bare `[phrase](mem_xxx)` defaults to "sibling"). Deduped by (kind, target)
/// pair — the same target under two different kinds both survive, first
/// phrase wins for an exact repeat. Links to `source_id` are dropped.
pub(crate) fn parse_relationship_links(
    source_id: &str,
    content: &str,
) -> Vec<(String, String, String)> {
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for cap in RELATIONSHIP_LINK_RE.captures_iter(content) {
        let target = cap[3].to_string();
        if target == source_id {
            continue;
        }
        let kind = cap
            .get(2)
            .map(|m| m.as_str().to_string())
            .unwrap_or_else(|| "sibling".to_string());
        if !seen.insert((kind.clone(), target.clone())) {
            continue;
        }
        out.push((cap[1].to_string(), kind, target));
    }
    out
}

/// Maintain parent/child/sibling edges for `source_id` from its content's
/// `[phrase](kind:mem_xxx)` links (bare `[phrase](mem_xxx)` defaults to
/// "sibling"): upserts an active edge per surviving (kind, target) pair
/// (refreshing `link_text` and resetting `status` to 'active' on rephrase),
/// and deletes any (kind, target) pair no longer present in the content.
async fn sync_relationship_edges(
    tx: &libsql::Transaction,
    source_id: &str,
    content: &str,
    now: i64,
) -> Result<()> {
    let links = parse_relationship_links(source_id, content);

    for (phrase, kind, target) in &links {
        tx.execute(
            "INSERT INTO edges (id, source_id, target_id, relationship, status, created_at, link_text)
             SELECT 'edge_' || lower(hex(randomblob(16))), ?1, ?2, ?3, 'active', ?4, ?5
             WHERE EXISTS (SELECT 1 FROM memories WHERE id = ?2)
             ON CONFLICT(source_id, target_id, relationship)
             DO UPDATE SET link_text = excluded.link_text, status = 'active'",
            params![source_id, target.as_str(), kind.as_str(), now, phrase.as_str()],
        )
        .await?;
    }

    if links.is_empty() {
        tx.execute(
            "DELETE FROM edges WHERE source_id = ?1 AND relationship IN ('parent', 'child', 'sibling')",
            params![source_id],
        )
        .await?;
    } else {
        let conditions: Vec<String> = (0..links.len())
            .map(|i| {
                format!(
                    "(relationship = ?{} AND target_id = ?{})",
                    i * 2 + 2,
                    i * 2 + 3
                )
            })
            .collect();
        let sql = format!(
            "DELETE FROM edges WHERE source_id = ?1 AND relationship IN ('parent', 'child', 'sibling') \
             AND NOT ({})",
            conditions.join(" OR ")
        );
        let mut p: Vec<libsql::Value> = vec![libsql::Value::Text(source_id.to_string())];
        for (_, kind, target) in &links {
            p.push(libsql::Value::Text(kind.clone()));
            p.push(libsql::Value::Text(target.clone()));
        }
        tx.execute(&sql, p).await?;
    }
    Ok(())
}

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

        sync_relationship_edges(&tx, m.id, m.content, now).await?;

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

        sync_relationship_edges(&tx, id, content, now).await?;

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

    pub async fn log_session_start(
        &self,
        project_path: &str,
        result: &crate::session::SessionStartResult,
    ) -> Result<()> {
        let id = format!("ssl_{}", uuid::Uuid::new_v4().simple());
        let now = chrono_now();
        let loaded_json = json!(
            result
                .loaded
                .iter()
                .map(|l| json!({
                    "id": l.entry.id,
                    "title": l.entry.title,
                    "tags": l.entry.tags,
                    "layer": l.entry.layer,
                    "tokens": l.tokens,
                }))
                .collect::<Vec<_>>()
        )
        .to_string();
        let skipped_json = json!(
            result
                .skipped
                .iter()
                .map(|s| json!({ "query": s.query, "reason": s.reason.as_str() }))
                .collect::<Vec<_>>()
        )
        .to_string();
        // Match `SessionStartResult::truncated()` semantics so the analytics
        // page reports the same truncation status the MCP tool already
        // returned for this identical session-start run.
        let truncated = result.truncated();

        self.conn
            .execute(
                "INSERT INTO session_start_log
                    (id, project_name, project_path, created_at, max_tokens, used_tokens,
                     memories_recalled, truncated, loaded_json, skipped_json)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                params![
                    id,
                    result.project.clone(),
                    project_path,
                    now,
                    result.max_tokens as i64,
                    result.used_tokens as i64,
                    result.memories_recalled as i64,
                    truncated as i64,
                    loaded_json,
                    skipped_json,
                ],
            )
            .await?;
        Ok(())
    }

    pub async fn list_session_logs(&self, limit: i64) -> Result<Vec<SessionStartLogRow>> {
        let mut rows = self
            .conn
            .query(
                "SELECT id, project_name, project_path, created_at, max_tokens, used_tokens,
                        memories_recalled, truncated, loaded_json, skipped_json
                 FROM session_start_log ORDER BY created_at DESC LIMIT ?1",
                params![limit],
            )
            .await?;
        let mut results = Vec::new();
        while let Some(row) = rows.next().await? {
            let loaded_json: String = row.get(8)?;
            let skipped_json: String = row.get(9)?;
            results.push(SessionStartLogRow {
                id: row.get(0)?,
                project_name: row.get(1)?,
                project_path: row.get(2)?,
                created_at: row.get(3)?,
                max_tokens: row.get(4)?,
                used_tokens: row.get(5)?,
                memories_recalled: row.get(6)?,
                truncated: row.get::<i64>(7)? != 0,
                loaded: serde_json::from_str(&loaded_json)?,
                skipped: serde_json::from_str(&skipped_json)?,
            });
        }
        Ok(results)
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

    /// Evaluates a tag boolean expression against every stored memory. Reuses
    /// `list_memories`'s per-row tag fetch (same N+1 pattern already used by
    /// `list_memories`/`search`) rather than a bulk-query optimization — fine
    /// at this tool's realistic memory counts (see `export()`'s identical
    /// `list_memories(100_000, 0)` full-table convention).
    pub async fn find_by_tag_expr(
        &self,
        expr: &crate::tag_query::TagExpr,
    ) -> Result<Vec<MemoryEntry>> {
        let all = self.list_memories(100_000, 0).await?;
        Ok(all.into_iter().filter(|e| expr.eval(&e.tags)).collect())
    }

    pub async fn list_edges(&self, memory_id: Option<&str>) -> Result<Vec<EdgeEntry>> {
        let mut rows = if let Some(mid) = memory_id {
            self.conn
                .query(
                    "SELECT id, source_id, target_id, relationship, status, created_at, link_text, reason
                     FROM edges WHERE source_id = ?1 OR target_id = ?1 ORDER BY created_at DESC",
                    params![mid],
                )
                .await?
        } else {
            self.conn
                .query(
                    "SELECT id, source_id, target_id, relationship, status, created_at, link_text, reason
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
                link_text: row.get(6)?,
                reason: row.get(7)?,
            });
        }
        Ok(results)
    }

    pub async fn get_edges_grouped(&self, memory_id: &str) -> Result<GroupedEdges> {
        async fn fetch(
            conn: &Connection,
            sql: &str,
            memory_id: &str,
        ) -> Result<Vec<RelatedMemory>> {
            let mut rows = conn.query(sql, params![memory_id]).await?;
            let mut out = Vec::new();
            while let Some(row) = rows.next().await? {
                out.push(RelatedMemory {
                    id: row.get(0)?,
                    title: row.get(1)?,
                    link_text: row.get(2)?,
                });
            }
            Ok(out)
        }

        let parents = fetch(
            &self.conn,
            "SELECT id, title, MIN(link_text) AS link_text FROM ( \
                SELECT m.id, m.title, e.link_text FROM edges e JOIN memories m ON m.id = e.target_id \
                 WHERE e.source_id = ?1 AND e.relationship = 'parent' \
                 UNION ALL \
                SELECT m.id, m.title, e.link_text FROM edges e JOIN memories m ON m.id = e.source_id \
                 WHERE e.target_id = ?1 AND e.relationship = 'child' \
             ) GROUP BY id, title",
            memory_id,
        )
        .await?;

        let children = fetch(
            &self.conn,
            "SELECT id, title, MIN(link_text) AS link_text FROM ( \
                SELECT m.id, m.title, e.link_text FROM edges e JOIN memories m ON m.id = e.target_id \
                 WHERE e.source_id = ?1 AND e.relationship = 'child' \
                 UNION ALL \
                SELECT m.id, m.title, e.link_text FROM edges e JOIN memories m ON m.id = e.source_id \
                 WHERE e.target_id = ?1 AND e.relationship = 'parent' \
             ) GROUP BY id, title",
            memory_id,
        )
        .await?;

        let siblings = fetch(
            &self.conn,
            "SELECT id, title, MIN(link_text) AS link_text FROM ( \
                SELECT m.id, m.title, e.link_text FROM edges e JOIN memories m ON m.id = e.target_id \
                 WHERE e.source_id = ?1 AND e.relationship = 'sibling' \
                 UNION ALL \
                SELECT m.id, m.title, e.link_text FROM edges e JOIN memories m ON m.id = e.source_id \
                 WHERE e.target_id = ?1 AND e.relationship = 'sibling' \
             ) GROUP BY id, title",
            memory_id,
        )
        .await?;

        Ok(GroupedEdges {
            parents,
            children,
            siblings,
        })
    }

    pub async fn create_edge(
        &self,
        source_id: &str,
        target_id: &str,
        relationship: &str,
    ) -> Result<crate::model::EdgeCreate> {
        self.create_edge_with_status(source_id, target_id, relationship, "active", None, None)
            .await
    }

    /// Like `create_edge`, but lets the caller set `status`/`link_text`
    /// directly instead of always defaulting to a freshly-accepted edge —
    /// used by import so a restored backup preserves pending/rejected edges
    /// and mention anchor text instead of silently promoting everything to
    /// 'active' and dropping link_text.
    pub async fn create_edge_with_status(
        &self,
        source_id: &str,
        target_id: &str,
        relationship: &str,
        status: &str,
        link_text: Option<&str>,
        reason: Option<&str>,
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
                "INSERT INTO edges (id, source_id, target_id, relationship, status, created_at, link_text, reason)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    id.as_str(),
                    source_id,
                    target_id,
                    relationship,
                    status,
                    chrono_now(),
                    link_text,
                    reason
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

    pub async fn get_edge(&self, id: &str) -> Result<Option<EdgeEntry>> {
        let mut rows = self
            .conn
            .query(
                "SELECT id, source_id, target_id, relationship, status, created_at, link_text, reason
                 FROM edges WHERE id = ?1",
                params![id],
            )
            .await?;
        match rows.next().await? {
            None => Ok(None),
            Some(row) => Ok(Some(EdgeEntry {
                id: row.get(0)?,
                source_id: row.get(1)?,
                target_id: row.get(2)?,
                relationship: row.get(3)?,
                status: row.get(4)?,
                created_at: row.get(5)?,
                link_text: row.get(6)?,
                reason: row.get(7)?,
            })),
        }
    }

    /// Patch a subset of edge fields. `None` leaves a field unchanged.
    /// Returns false when no edge has this id.
    pub async fn update_edge(
        &self,
        id: &str,
        relationship: Option<&str>,
        reason: Option<&str>,
        link_text: Option<&str>,
    ) -> Result<bool> {
        if let Some(r) = relationship
            && !VALID_RELATIONSHIPS.contains(&r)
        {
            anyhow::bail!("invalid relationship; valid: {}", VALID_RELATIONSHIPS.join(", "));
        }
        let changed = self
            .conn
            .execute(
                "UPDATE edges SET
                     relationship = COALESCE(?1, relationship),
                     reason = COALESCE(?2, reason),
                     link_text = COALESCE(?3, link_text)
                 WHERE id = ?4",
                params![relationship, reason, link_text, id],
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
    async fn find_by_tag_expr_returns_matching_memories() {
        let (s, _dir) = make_store().await;
        s.store(&test_row(
            "mem_rust",
            "Rust notes",
            "content",
            &["lang:rust".into(), "project:hivemind".into()],
        ))
        .await
        .unwrap();
        s.store(&test_row(
            "mem_vue",
            "Vue notes",
            "content",
            &["lang:vue".into(), "project:hivemind".into()],
        ))
        .await
        .unwrap();
        s.store(&test_row(
            "mem_other",
            "Unrelated",
            "content",
            &["project:oxhive".into()],
        ))
        .await
        .unwrap();

        let expr = crate::tag_query::parse("tag:project:hivemind & tag:lang:rust").unwrap();
        let results = s.find_by_tag_expr(&expr).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].title, "Rust notes");

        let expr_or = crate::tag_query::parse("tag:lang:rust | tag:lang:vue").unwrap();
        let mut results_or = s.find_by_tag_expr(&expr_or).await.unwrap();
        results_or.sort_by(|a, b| a.title.cmp(&b.title));
        assert_eq!(results_or.len(), 2);
        assert_eq!(results_or[0].title, "Rust notes");
        assert_eq!(results_or[1].title, "Vue notes");
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
        s.create_edge("mem_x", "mem_z", "sibling").await.unwrap();

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
        let edge = s.create_edge("mem_p", "mem_q", "sibling").await.unwrap();
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
    async fn create_edge_with_reason_roundtrips() {
        let (s, _dir) = make_store().await;
        s.store(&test_row("mem_a", "A", "a", &[])).await.unwrap();
        s.store(&test_row("mem_b", "B", "b", &[])).await.unwrap();
        let created = s
            .create_edge_with_status("mem_a", "mem_b", "sibling", "pending", None, Some("both cover auth"))
            .await
            .unwrap();
        assert!(matches!(created, crate::model::EdgeCreate::Created(_)));
        let edges = s.list_edges(None).await.unwrap();
        assert_eq!(edges[0].reason.as_deref(), Some("both cover auth"));
        assert_eq!(edges[0].status, "pending");
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
    async fn update_edge_patches_fields_in_place() {
        let (s, _dir) = make_store().await;
        s.store(&test_row("mem_a", "A", "a", &[])).await.unwrap();
        s.store(&test_row("mem_b", "B", "b", &[])).await.unwrap();
        let created = s
            .create_edge_with_status("mem_a", "mem_b", "sibling", "pending", None, Some("first take"))
            .await
            .unwrap();
        let crate::model::EdgeCreate::Created(id) = created else {
            panic!("expected EdgeCreate::Created");
        };
        let ok = s
            .update_edge(&id, Some("parent"), Some("refined reason"), None)
            .await
            .unwrap();
        assert!(ok);
        let e = s.get_edge(&id).await.unwrap().unwrap();
        assert_eq!(e.relationship, "parent");
        assert_eq!(e.reason.as_deref(), Some("refined reason"));
        assert_eq!(e.status, "pending", "status untouched");
    }

    #[tokio::test]
    async fn update_edge_rejects_invalid_relationship_and_missing_id() {
        let (s, _dir) = make_store().await;
        assert!(!s.update_edge("edge_missing", None, Some("x"), None).await.unwrap());

        s.store(&test_row("mem_a", "A", "a", &[])).await.unwrap();
        s.store(&test_row("mem_b", "B", "b", &[])).await.unwrap();
        let crate::model::EdgeCreate::Created(id) =
            s.create_edge("mem_a", "mem_b", "sibling").await.unwrap()
        else {
            panic!("expected EdgeCreate::Created");
        };
        assert!(s.update_edge(&id, Some("related_to"), None, None).await.is_err());
    }

    #[tokio::test]
    async fn create_edge_reports_duplicate_and_missing_endpoint() {
        use crate::model::EdgeCreate;
        let (s, _dir) = make_store().await;
        let tags: Vec<String> = vec![];
        s.store(&test_row("mem_1", "A", "a", &tags)).await.unwrap();
        s.store(&test_row("mem_2", "B", "b", &tags)).await.unwrap();

        let first = s.create_edge("mem_1", "mem_2", "sibling").await.unwrap();
        assert!(matches!(first, EdgeCreate::Created(_)));
        // duplicate, even reversed
        assert_eq!(
            s.create_edge("mem_2", "mem_1", "sibling").await.unwrap(),
            EdgeCreate::Duplicate
        );
        assert_eq!(
            s.create_edge("mem_1", "mem_ghost", "sibling")
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

    #[tokio::test]
    async fn get_edges_grouped_reads_parent_and_child_from_both_directions() {
        const PARENT: &str = "mem_11111111111111111111111111111111";
        const CHILD: &str = "mem_22222222222222222222222222222222";
        let (s, _dir) = make_store().await;
        s.store(&test_row(CHILD, "Child", "child body", &[]))
            .await
            .unwrap();
        s.store(&test_row(PARENT, "Parent", "parent body", &[]))
            .await
            .unwrap();
        // CHILD asserts PARENT is its parent.
        s.update(CHILD, "Child", &format!("[the rule](parent:{PARENT})"), &[])
            .await
            .unwrap();

        let from_child = s.get_edges_grouped(CHILD).await.unwrap();
        assert_eq!(from_child.parents.len(), 1);
        assert_eq!(from_child.parents[0].id, PARENT);
        assert_eq!(from_child.parents[0].link_text.as_deref(), Some("the rule"));
        assert!(from_child.children.is_empty());

        // From the parent's side, the same edge should surface CHILD as a child,
        // even though PARENT never authored a `child:` link itself.
        let from_parent = s.get_edges_grouped(PARENT).await.unwrap();
        assert_eq!(from_parent.children.len(), 1);
        assert_eq!(from_parent.children[0].id, CHILD);
        assert!(from_parent.parents.is_empty());
    }

    #[tokio::test]
    async fn get_edges_grouped_siblings_symmetric() {
        const A: &str = "mem_33333333333333333333333333333333";
        const B: &str = "mem_44444444444444444444444444444444";
        let (s, _dir) = make_store().await;
        s.store(&test_row(A, "A", "a", &[])).await.unwrap();
        s.store(&test_row(B, "B", &format!("[peer](sibling:{A})"), &[]))
            .await
            .unwrap();

        let from_a = s.get_edges_grouped(A).await.unwrap();
        assert_eq!(from_a.siblings.len(), 1);
        assert_eq!(from_a.siblings[0].id, B);

        let from_b = s.get_edges_grouped(B).await.unwrap();
        assert_eq!(from_b.siblings.len(), 1);
        assert_eq!(from_b.siblings[0].id, A);
    }

    #[tokio::test]
    async fn get_edges_grouped_dedupes_mutual_reciprocal_links() {
        const A: &str = "mem_55555555555555555555555555555555";
        const B: &str = "mem_66666666666666666666666666666666";
        let (s, _dir) = make_store().await;
        s.store(&test_row(A, "A", "a", &[])).await.unwrap();
        s.store(&test_row(B, "B", "b", &[])).await.unwrap();
        // Each side independently authors a reciprocal sibling link, producing
        // two distinct edge rows: (A -> B, sibling) and (B -> A, sibling).
        s.update(A, "A", &format!("[my sibling](sibling:{B})"), &[])
            .await
            .unwrap();
        s.update(B, "B", &format!("[my sibling too](sibling:{A})"), &[])
            .await
            .unwrap();

        let from_a = s.get_edges_grouped(A).await.unwrap();
        assert_eq!(
            from_a.siblings.len(),
            1,
            "expected exactly one sibling entry, got {:?}",
            from_a.siblings
        );
        assert_eq!(from_a.siblings[0].id, B);

        let from_b = s.get_edges_grouped(B).await.unwrap();
        assert_eq!(
            from_b.siblings.len(),
            1,
            "expected exactly one sibling entry, got {:?}",
            from_b.siblings
        );
        assert_eq!(from_b.siblings[0].id, A);
    }

    #[tokio::test]
    async fn get_edges_grouped_dedupes_mutual_parent_child_links() {
        const A: &str = "mem_77777777777777777777777777777777";
        const B: &str = "mem_88888888888888888888888888888888";
        let (s, _dir) = make_store().await;
        s.store(&test_row(A, "A", "a", &[])).await.unwrap();
        s.store(&test_row(B, "B", "b", &[])).await.unwrap();
        // A declares B as its parent, and B independently declares A as its child,
        // producing two distinct edge rows: (A -> B, parent) and (B -> A, child).
        s.update(A, "A", &format!("[my parent](parent:{B})"), &[])
            .await
            .unwrap();
        s.update(B, "B", &format!("[my child](child:{A})"), &[])
            .await
            .unwrap();

        let from_a = s.get_edges_grouped(A).await.unwrap();
        assert_eq!(
            from_a.parents.len(),
            1,
            "expected exactly one parent entry, got {:?}",
            from_a.parents
        );
        assert_eq!(from_a.parents[0].id, B);

        let from_b = s.get_edges_grouped(B).await.unwrap();
        assert_eq!(
            from_b.children.len(),
            1,
            "expected exactly one child entry, got {:?}",
            from_b.children
        );
        assert_eq!(from_b.children[0].id, A);
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

    #[test]
    fn parse_relationship_links_defaults_bare_link_to_sibling() {
        let id = "mem_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        let content = "see [plain link](mem_bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb)";
        let got = parse_relationship_links(id, content);
        assert_eq!(
            got,
            vec![(
                "plain link".to_string(),
                "sibling".to_string(),
                "mem_bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".to_string()
            )]
        );
    }

    #[test]
    fn parse_relationship_links_reads_explicit_kind_prefix() {
        let id = "mem_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        let content = "\
            [the rule](parent:mem_bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb) \
            [an instance](child:mem_cccccccccccccccccccccccccccccccc) \
            [a peer](sibling:mem_dddddddddddddddddddddddddddddddd)";
        let got = parse_relationship_links(id, content);
        assert_eq!(
            got,
            vec![
                (
                    "the rule".to_string(),
                    "parent".to_string(),
                    "mem_bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".to_string()
                ),
                (
                    "an instance".to_string(),
                    "child".to_string(),
                    "mem_cccccccccccccccccccccccccccccccc".to_string()
                ),
                (
                    "a peer".to_string(),
                    "sibling".to_string(),
                    "mem_dddddddddddddddddddddddddddddddd".to_string()
                ),
            ]
        );
    }

    #[test]
    fn parse_relationship_links_same_target_different_kinds_both_survive() {
        let id = "mem_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        let content = "\
            [as parent](parent:mem_bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb) \
            [as sibling too](sibling:mem_bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb)";
        let got = parse_relationship_links(id, content);
        assert_eq!(got.len(), 2);
        assert!(got.iter().any(|(_, k, _)| k == "parent"));
        assert!(got.iter().any(|(_, k, _)| k == "sibling"));
    }

    #[test]
    fn parse_relationship_links_drops_self_link_and_unknown_kind_word() {
        let id = "mem_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        let content = "\
            [self](parent:mem_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa) \
            [bogus kind](notakind:mem_bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb) \
            [real one](sibling:mem_ccccccccccccccccccccccccccccccc)";
        let got = parse_relationship_links(id, content);
        // "bogus kind" fails the fixed parent|child|sibling alternation entirely,
        // so it doesn't match at all (not even as a bare/default link) — this is
        // the same "malformed mention falls through as inert text" behavior the
        // prior feature already had for e.g. a too-short mem_ id.
        assert_eq!(got.len(), 0);
    }

    const T_B: &str = "mem_bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
    const T_C: &str = "mem_cccccccccccccccccccccccccccccccc";

    #[tokio::test]
    async fn store_creates_edges_with_parsed_kind_and_link_text() {
        let (s, _dir) = make_store().await;
        s.store(&test_row(T_B, "Target", "target body", &[]))
            .await
            .unwrap();
        let content = format!("[the rule]({T_B})");
        s.store(&test_row("mem_src", "Src", &content, &[]))
            .await
            .unwrap();

        let edges = s.list_edges(Some("mem_src")).await.unwrap();
        assert_eq!(edges.len(), 1);
        assert_eq!(edges[0].relationship, "sibling");
        assert_eq!(edges[0].status, "active");
        assert_eq!(edges[0].link_text.as_deref(), Some("the rule"));
    }

    #[tokio::test]
    async fn store_creates_explicit_kind_edge() {
        let (s, _dir) = make_store().await;
        s.store(&test_row(T_B, "Target", "target body", &[]))
            .await
            .unwrap();
        let content = format!("[the rule](parent:{T_B})");
        s.store(&test_row("mem_src", "Src", &content, &[]))
            .await
            .unwrap();

        let edges = s.list_edges(Some("mem_src")).await.unwrap();
        assert_eq!(edges.len(), 1);
        assert_eq!(edges[0].relationship, "parent");
    }

    #[tokio::test]
    async fn update_switching_kind_deletes_old_kind_edge_and_creates_new() {
        let (s, _dir) = make_store().await;
        s.store(&test_row(T_B, "B", "b", &[])).await.unwrap();
        s.store(&test_row(
            "mem_src",
            "Src",
            &format!("[x](parent:{T_B})"),
            &[],
        ))
        .await
        .unwrap();

        let first = s.list_edges(Some("mem_src")).await.unwrap();
        assert_eq!(first.len(), 1);
        assert_eq!(first[0].relationship, "parent");

        s.update("mem_src", "Src", &format!("[x](child:{T_B})"), &[])
            .await
            .unwrap();
        let second = s.list_edges(Some("mem_src")).await.unwrap();
        assert_eq!(
            second.len(),
            1,
            "old parent-kind edge should be gone, replaced by a child-kind one"
        );
        assert_eq!(second[0].relationship, "child");
    }

    #[tokio::test]
    async fn update_same_target_two_kinds_keeps_both_and_only_removes_dropped_one() {
        let (s, _dir) = make_store().await;
        s.store(&test_row(T_B, "B", "b", &[])).await.unwrap();
        s.store(&test_row(
            "mem_src",
            "Src",
            &format!("[as parent](parent:{T_B}) [as sibling](sibling:{T_B})"),
            &[],
        ))
        .await
        .unwrap();
        let first = s.list_edges(Some("mem_src")).await.unwrap();
        assert_eq!(first.len(), 2);

        // Drop the sibling-kind link, keep the parent-kind one.
        s.update("mem_src", "Src", &format!("[as parent](parent:{T_B})"), &[])
            .await
            .unwrap();
        let second = s.list_edges(Some("mem_src")).await.unwrap();
        assert_eq!(second.len(), 1);
        assert_eq!(second[0].relationship, "parent");
    }

    #[tokio::test]
    async fn update_dropping_one_of_two_targets_deletes_only_that_edge() {
        let (s, _dir) = make_store().await;
        s.store(&test_row(T_B, "B", "b", &[])).await.unwrap();
        s.store(&test_row(T_C, "C", "c", &[])).await.unwrap();
        s.store(&test_row(
            "mem_src",
            "Src",
            &format!("[a]({T_B}) [b]({T_C})"),
            &[],
        ))
        .await
        .unwrap();
        let first = s.list_edges(Some("mem_src")).await.unwrap();
        assert_eq!(first.len(), 2);

        // Rephrase to drop the link to T_C, keeping only the link to T_B.
        s.update("mem_src", "Src", &format!("[a]({T_B})"), &[])
            .await
            .unwrap();
        let second = s.list_edges(Some("mem_src")).await.unwrap();
        assert_eq!(
            second.len(),
            1,
            "the T_C edge should have been stale-deleted"
        );
        assert_eq!(second[0].target_id, T_B);
    }

    #[tokio::test]
    async fn relationship_link_to_nonexistent_target_creates_no_edge() {
        let (s, _dir) = make_store().await;
        s.store(&test_row("mem_src", "Src", &format!("[ghost]({T_B})"), &[]))
            .await
            .unwrap();
        let edges = s.list_edges(Some("mem_src")).await.unwrap();
        assert!(edges.is_empty());
    }

    #[tokio::test]
    async fn stale_pending_mention_edge_self_heals_to_active_on_save() {
        let (s, _dir) = make_store().await;
        s.store(&test_row(T_B, "Target", "target body", &[]))
            .await
            .unwrap();
        s.store(&test_row("mem_src", "Src", "no links yet", &[]))
            .await
            .unwrap();

        // Simulate an import-created `sibling` edge stuck in 'pending' (e.g. from a
        // path that inserts relationship edges without activating them).
        s.conn
            .execute(
                "INSERT INTO edges (id, source_id, target_id, relationship, status, created_at, link_text)
                 VALUES ('edge_pending_test', 'mem_src', ?1, 'sibling', 'pending', ?2, 'old text')",
                params![T_B, chrono_now()],
            )
            .await
            .unwrap();

        // Saving the source memory with a matching `[phrase](target_id)` link should
        // upsert the existing edge and flip it back to 'active'.
        let content = format!("relates to [the target]({T_B})");
        s.update("mem_src", "Src", &content, &[]).await.unwrap();

        let edges = s.list_edges(Some("mem_src")).await.unwrap();
        let m: Vec<_> = edges
            .iter()
            .filter(|e| e.relationship == "sibling")
            .collect();
        assert_eq!(m.len(), 1);
        assert_eq!(m[0].id, "edge_pending_test");
        assert_eq!(m[0].status, "active");
        assert_eq!(m[0].link_text.as_deref(), Some("the target"));
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

    #[tokio::test]
    async fn log_and_list_session_start_round_trip() {
        let (store, _dir) = make_store().await;
        let result = crate::session::SessionStartResult {
            project: "demo".to_string(),
            loaded: vec![],
            skipped: vec![crate::session::SkippedEntry {
                query: "missing pref".to_string(),
                reason: crate::session::SkipReason::NotFound,
            }],
            used_tokens: 120,
            max_tokens: 2000,
            memories_recalled: 0,
        };
        store
            .log_session_start("/tmp/demo-project", &result)
            .await
            .unwrap();

        let logs = store.list_session_logs(10).await.unwrap();
        assert_eq!(logs.len(), 1);
        assert_eq!(logs[0].project_name, "demo");
        assert_eq!(logs[0].project_path, "/tmp/demo-project");
        assert_eq!(logs[0].used_tokens, 120);
        assert_eq!(logs[0].max_tokens, 2000);
        assert_eq!(logs[0].memories_recalled, 0);
        assert!(logs[0].truncated);
        assert_eq!(logs[0].skipped[0]["query"], "missing pref");
        assert_eq!(logs[0].skipped[0]["reason"], "not_found");
    }

    #[tokio::test]
    async fn list_session_logs_orders_newest_first() {
        let (store, _dir) = make_store().await;
        for (project, used) in [("first", 10usize), ("second", 20usize)] {
            let result = crate::session::SessionStartResult {
                project: project.to_string(),
                loaded: vec![],
                skipped: vec![],
                used_tokens: used,
                max_tokens: 2000,
                memories_recalled: 0,
            };
            store.log_session_start("/tmp/p", &result).await.unwrap();
            tokio::time::sleep(std::time::Duration::from_millis(1100)).await;
        }
        let logs = store.list_session_logs(10).await.unwrap();
        assert_eq!(logs.len(), 2);
        assert_eq!(logs[0].project_name, "second", "newest first");
        assert_eq!(logs[1].project_name, "first");
    }
}
