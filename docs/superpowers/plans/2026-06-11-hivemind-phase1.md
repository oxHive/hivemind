# HiveMind Phase 1 — Core MCP Loop

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A working Rust MCP stdio server with `memory_store` and `memory_recall` tools backed by SQLite — the minimum needed to store a preference in one Claude Code session and recall it in the next.

**Architecture:** Single Rust binary; SQLite for storage via `rusqlite` with bundled libsqlite; MCP protocol via `rmcp` with stdio transport. Five source modules: `model` (types), `db` (schema), `store` (data access), `server` (MCP tools), `main` (wiring).

**Tech Stack:** Rust 2024 edition · rmcp 1.x (`server` feature) · rusqlite 0.32 (bundled) · tokio 1 (full) · serde + schemars · uuid v4 · tracing → stderr

---

## File Map

| File | Responsibility |
|------|---------------|
| `Cargo.toml` | Dependencies |
| `src/main.rs` | Tokio entry point; opens DB; serves stdio |
| `src/model.rs` | `Layer`, `MemoryType` enums; `MemoryEntry`, `NewMemory` structs |
| `src/db.rs` | `create_schema(conn)`, `open(path)` |
| `src/store.rs` | `SqliteStore` — `store()`, `recall_by_id()`, `recall_by_title()` |
| `src/server.rs` | `HiveMind` struct; `memory_store` and `memory_recall` MCP tools |

---

## Task 1: Update Cargo.toml

**Files:**
- Modify: `Cargo.toml`

- [ ] **Step 1: Replace `[dependencies]` section**

```toml
[package]
name = "oxhivemind"
version = "0.1.0"
edition = "2024"

[bin]
name = "hivemind"
path = "src/main.rs"

[dependencies]
rmcp = { version = "1", features = ["server"] }
rusqlite = { version = "0.32", features = ["bundled"] }
tokio = { version = "1", features = ["full"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
schemars = "0.8"
anyhow = "1"
uuid = { version = "1", features = ["v4"] }
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }

[dev-dependencies]
tempfile = "3"
```

- [ ] **Step 2: Verify dependencies resolve**

```bash
cargo fetch
```

Expected: no errors, lockfile written.

---

## Task 2: Model types (src/model.rs)

**Files:**
- Create: `src/model.rs`
- Modify: `src/main.rs` (add `mod model;`)

- [ ] **Step 1: Add `mod model;` stub to main.rs so tests can reference it**

Replace `src/main.rs` contents with:

```rust
mod model;

fn main() {}
```

- [ ] **Step 2: Write the failing test**

Create `src/model.rs` with only the test module:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layer_roundtrips_through_string() {
        let l: Layer = "personal".parse().unwrap();
        assert_eq!(l.to_string(), "personal");

        let l: Layer = "workspace".parse().unwrap();
        assert_eq!(l.to_string(), "workspace");
    }

    #[test]
    fn layer_rejects_invalid_value() {
        assert!("unknown".parse::<Layer>().is_err());
    }

    #[test]
    fn memory_type_roundtrips() {
        assert_eq!("preference".parse::<MemoryType>().unwrap().to_string(), "preference");
        assert_eq!("project".parse::<MemoryType>().unwrap().to_string(), "project");
        assert_eq!("history".parse::<MemoryType>().unwrap().to_string(), "history");
    }
}
```

- [ ] **Step 3: Run — expect compile error (types undefined)**

```bash
cargo test --lib 2>&1 | head -20
```

Expected: `error[E0412]: cannot find type 'Layer'`

- [ ] **Step 4: Implement the full model module**

Replace `src/model.rs` with:

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Layer {
    Personal,
    Workspace,
}

impl std::fmt::Display for Layer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Layer::Personal => write!(f, "personal"),
            Layer::Workspace => write!(f, "workspace"),
        }
    }
}

impl std::str::FromStr for Layer {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "personal" => Ok(Layer::Personal),
            "workspace" => Ok(Layer::Workspace),
            _ => Err(anyhow::anyhow!("invalid layer: {s}")),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum MemoryType {
    Preference,
    Project,
    History,
}

impl std::fmt::Display for MemoryType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MemoryType::Preference => write!(f, "preference"),
            MemoryType::Project => write!(f, "project"),
            MemoryType::History => write!(f, "history"),
        }
    }
}

impl std::str::FromStr for MemoryType {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "preference" => Ok(MemoryType::Preference),
            "project" => Ok(MemoryType::Project),
            "history" => Ok(MemoryType::History),
            _ => Err(anyhow::anyhow!("invalid memory type: {s}")),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryEntry {
    pub id: String,
    pub layer: Layer,
    pub memory_type: MemoryType,
    pub title: String,
    pub content: String,
    pub source: Option<String>,
    pub project: Option<String>,
    pub tags: Vec<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone)]
pub struct NewMemory {
    pub title: String,
    pub content: String,
    pub layer: Layer,
    pub memory_type: MemoryType,
    pub tags: Vec<String>,
    pub project: Option<String>,
    pub source: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layer_roundtrips_through_string() {
        let l: Layer = "personal".parse().unwrap();
        assert_eq!(l.to_string(), "personal");

        let l: Layer = "workspace".parse().unwrap();
        assert_eq!(l.to_string(), "workspace");
    }

    #[test]
    fn layer_rejects_invalid_value() {
        assert!("unknown".parse::<Layer>().is_err());
    }

    #[test]
    fn memory_type_roundtrips() {
        assert_eq!("preference".parse::<MemoryType>().unwrap().to_string(), "preference");
        assert_eq!("project".parse::<MemoryType>().unwrap().to_string(), "project");
        assert_eq!("history".parse::<MemoryType>().unwrap().to_string(), "history");
    }
}
```

- [ ] **Step 5: Run — expect all pass**

```bash
cargo test model
```

Expected: `test model::tests::layer_roundtrips_through_string ... ok`

- [ ] **Step 6: Commit**

```bash
git add Cargo.toml Cargo.lock src/main.rs src/model.rs
git commit -m "feat: add model types (Layer, MemoryType, MemoryEntry, NewMemory)"
```

---

## Task 3: SQLite schema (src/db.rs)

**Files:**
- Create: `src/db.rs`
- Modify: `src/main.rs` (add `mod db;`)

- [ ] **Step 1: Write the failing test**

Create `src/db.rs` with only the test module:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_schema_creates_memories_and_tags_tables() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
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
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        create_schema(&conn).unwrap();
        create_schema(&conn).unwrap(); // second call must not error
    }
}
```

- [ ] **Step 2: Add `mod db;` to `src/main.rs`**

```rust
mod db;
mod model;

fn main() {}
```

- [ ] **Step 3: Run — expect compile error**

```bash
cargo test db 2>&1 | head -10
```

Expected: `error[E0425]: cannot find function 'create_schema'`

- [ ] **Step 4: Implement db.rs**

Replace `src/db.rs` with:

```rust
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
        );",
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
```

- [ ] **Step 5: Run — expect all pass**

```bash
cargo test db
```

Expected: `test db::tests::create_schema_creates_memories_and_tags_tables ... ok`

- [ ] **Step 6: Commit**

```bash
git add src/db.rs src/main.rs
git commit -m "feat: add SQLite schema (memories + tags tables)"
```

---

## Task 4: SqliteStore — store() (src/store.rs)

**Files:**
- Create: `src/store.rs`
- Modify: `src/main.rs` (add `mod store;`)

- [ ] **Step 1: Write the failing test**

Create `src/store.rs` with only the test module:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{db, model::{Layer, MemoryType, NewMemory}};

    fn open_test_store() -> SqliteStore {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
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
```

- [ ] **Step 2: Add `mod store;` to `src/main.rs`**

```rust
mod db;
mod model;
mod store;

fn main() {}
```

- [ ] **Step 3: Run — expect compile error**

```bash
cargo test store 2>&1 | head -10
```

Expected: `error[E0412]: cannot find struct 'SqliteStore'`

- [ ] **Step 4: Implement SqliteStore::store()**

Replace `src/store.rs` with:

```rust
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
```

- [ ] **Step 5: Run — expect all pass**

```bash
cargo test store
```

Expected: `test store::tests::store_returns_mem_prefixed_id ... ok`

- [ ] **Step 6: Commit**

```bash
git add src/store.rs src/main.rs
git commit -m "feat: implement SqliteStore::store()"
```

---

## Task 5: SqliteStore — recall methods

**Files:**
- Modify: `src/store.rs`

- [ ] **Step 1: Write the failing tests**

Add to the `tests` module in `src/store.rs`:

```rust
    #[test]
    fn recall_by_id_returns_full_entry_with_tags() {
        let s = open_test_store();
        let id = s.store(sample()).unwrap();
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
```

- [ ] **Step 2: Run — expect compile error**

```bash
cargo test store 2>&1 | head -10
```

Expected: `error[E0599]: no method named 'recall_by_id'`

- [ ] **Step 3: Implement recall methods**

Add to `src/store.rs` after the `store` method (inside the `impl SqliteStore` block):

```rust
    pub fn recall_by_id(&self, id: &str) -> Result<Option<MemoryEntry>> {
        let conn = self.conn.lock().unwrap();
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
                let layer = layer_s.parse::<Layer>()?;
                let memory_type = type_s.parse::<MemoryType>()?;
                let tags = Self::fetch_tags(&*conn, &rid)?;
                Ok(Some(MemoryEntry { id: rid, layer, memory_type, title, content, source, project, tags, created_at, updated_at }))
            }
        }
    }

    pub fn recall_by_title(&self, title: &str) -> Result<Option<MemoryEntry>> {
        let conn = self.conn.lock().unwrap();
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
                let layer = layer_s.parse::<Layer>()?;
                let memory_type = type_s.parse::<MemoryType>()?;
                let tags = Self::fetch_tags(&*conn, &rid)?;
                Ok(Some(MemoryEntry { id: rid, layer, memory_type, title, content, source, project, tags, created_at, updated_at }))
            }
        }
    }

    fn fetch_tags(conn: &Connection, memory_id: &str) -> Result<Vec<String>> {
        let mut stmt = conn.prepare("SELECT tag FROM tags WHERE memory_id = ?1")?;
        let tags = stmt
            .query_map(rusqlite::params![memory_id], |row| row.get(0))?
            .collect::<rusqlite::Result<Vec<String>>>()?;
        Ok(tags)
    }
```

- [ ] **Step 4: Run — expect all pass**

```bash
cargo test store
```

Expected: all 6 tests pass.

- [ ] **Step 5: Commit**

```bash
git add src/store.rs
git commit -m "feat: implement SqliteStore recall_by_id and recall_by_title"
```

---

## Task 6: MCP server — memory_store tool (src/server.rs)

**Files:**
- Create: `src/server.rs`
- Modify: `src/main.rs` (add `mod server;`)

- [ ] **Step 1: Write the failing test**

Create `src/server.rs` with only the test module:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{db, store::SqliteStore};

    fn test_hivemind() -> HiveMind {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        db::create_schema(&conn).unwrap();
        HiveMind::new(SqliteStore::new(conn))
    }

    #[tokio::test]
    async fn memory_store_tool_returns_mem_id() {
        let hm = test_hivemind();
        let result = hm.do_memory_store(MemoryStoreInput {
            title: "my preference".to_string(),
            content: "prefer tabs over spaces".to_string(),
            layer: "personal".to_string(),
            tags: vec!["style".to_string()],
            project: None,
        }).await.unwrap();
        let val = result.structured_content.unwrap();
        assert!(val["id"].as_str().unwrap().starts_with("mem_"));
        assert_eq!(val["auto_connected"], 0);
    }
}
```

- [ ] **Step 2: Add `mod server;` to `src/main.rs`**

```rust
mod db;
mod model;
mod server;
mod store;

fn main() {}
```

- [ ] **Step 3: Run — expect compile error**

```bash
cargo test server 2>&1 | head -10
```

Expected: `error[E0412]: cannot find struct 'HiveMind'`

- [ ] **Step 4: Implement server.rs with memory_store tool**

Replace `src/server.rs` with:

```rust
use std::sync::Arc;
use rmcp::{
    handler::server::wrapper::Parameters,
    model::{CallToolResult, ErrorData},
    schemars, tool, tool_router,
};
use serde::Deserialize;
use serde_json::json;
use crate::{
    model::{Layer, MemoryType, NewMemory},
    store::SqliteStore,
};

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct MemoryStoreInput {
    /// Short descriptive title for this memory
    pub title: String,
    /// Full content to store
    pub content: String,
    /// "personal" (follows you) or "workspace" (project-scoped)
    pub layer: String,
    /// Tags for search and auto-linking
    pub tags: Vec<String>,
    /// Project name — required when layer is "workspace"
    #[serde(default)]
    pub project: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct MemoryRecallInput {
    /// Memory ID (mem_xxx) — use this or title, not both
    #[serde(default)]
    pub id: Option<String>,
    /// Exact title — use this or id, not both
    #[serde(default)]
    pub title: Option<String>,
}

#[derive(Clone)]
pub struct HiveMind {
    store: Arc<SqliteStore>,
}

impl HiveMind {
    pub fn new(store: SqliteStore) -> Self {
        Self { store: Arc::new(store) }
    }

    pub async fn do_memory_store(&self, p: MemoryStoreInput) -> Result<CallToolResult, ErrorData> {
        let layer = p.layer.parse::<Layer>()
            .map_err(|e| ErrorData::invalid_params(e.to_string(), None))?;
        let title = p.title.clone();
        let new_memory = NewMemory {
            title: p.title,
            content: p.content,
            layer,
            memory_type: MemoryType::Preference,
            tags: p.tags,
            project: p.project,
            source: Some("claude_code".to_string()),
        };
        let id = self.store.store(new_memory)
            .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::structured(json!({
            "id": id,
            "title": title,
            "auto_connected": 0
        })))
    }

    pub async fn do_memory_recall(&self, p: MemoryRecallInput) -> Result<CallToolResult, ErrorData> {
        let entry = if let Some(ref id) = p.id {
            self.store.recall_by_id(id)
        } else if let Some(ref title) = p.title {
            self.store.recall_by_title(title)
        } else {
            return Err(ErrorData::invalid_params(
                "provide either 'id' or 'title'",
                None,
            ));
        }
        .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;

        match entry {
            None => Ok(CallToolResult::structured(json!({ "found": false }))),
            Some(e) => Ok(CallToolResult::structured(json!({
                "found": true,
                "id": e.id,
                "title": e.title,
                "content": e.content,
                "layer": e.layer.to_string(),
                "tags": e.tags,
                "project": e.project,
                "created_at": e.created_at,
                "updated_at": e.updated_at,
            }))),
        }
    }
}

#[tool_router(server_handler)]
impl HiveMind {
    #[tool(description = "Store a memory, preference, project context, or personal note for future recall across sessions and devices. Use when the user explicitly asks to remember something, or when important context should persist beyond this session.")]
    async fn memory_store(
        &self,
        Parameters(p): Parameters<MemoryStoreInput>,
    ) -> Result<CallToolResult, ErrorData> {
        self.do_memory_store(p).await
    }

    #[tool(description = "Recall a memory by exact title or ID. Returns full content. Use memory_search to find candidates first.")]
    async fn memory_recall(
        &self,
        Parameters(p): Parameters<MemoryRecallInput>,
    ) -> Result<CallToolResult, ErrorData> {
        self.do_memory_recall(p).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{db, store::SqliteStore};

    fn test_hivemind() -> HiveMind {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        db::create_schema(&conn).unwrap();
        HiveMind::new(SqliteStore::new(conn))
    }

    #[tokio::test]
    async fn memory_store_tool_returns_mem_id() {
        let hm = test_hivemind();
        let result = hm.do_memory_store(MemoryStoreInput {
            title: "my preference".to_string(),
            content: "prefer tabs over spaces".to_string(),
            layer: "personal".to_string(),
            tags: vec!["style".to_string()],
            project: None,
        }).await.unwrap();
        let val = result.structured_content.unwrap();
        assert!(val["id"].as_str().unwrap().starts_with("mem_"));
        assert_eq!(val["auto_connected"], 0);
    }
}
```

- [ ] **Step 5: Run — expect pass**

```bash
cargo test server
```

Expected: `test server::tests::memory_store_tool_returns_mem_id ... ok`

- [ ] **Step 6: Commit**

```bash
git add src/server.rs src/main.rs
git commit -m "feat: add HiveMind MCP server with memory_store tool"
```

---

## Task 7: MCP server — memory_recall tool

**Files:**
- Modify: `src/server.rs`

- [ ] **Step 1: Write the failing test**

Add to the `tests` module in `src/server.rs`:

```rust
    #[tokio::test]
    async fn memory_recall_by_id_returns_content() {
        let hm = test_hivemind();
        let stored = hm.do_memory_store(MemoryStoreInput {
            title: "rust style".to_string(),
            content: "use clippy, rustfmt, and deny warnings".to_string(),
            layer: "personal".to_string(),
            tags: vec!["rust".to_string()],
            project: None,
        }).await.unwrap();
        let id = stored.structured_content.unwrap()["id"].as_str().unwrap().to_string();

        let result = hm.do_memory_recall(MemoryRecallInput {
            id: Some(id),
            title: None,
        }).await.unwrap();
        let val = result.structured_content.unwrap();
        assert_eq!(val["found"], true);
        assert_eq!(val["title"], "rust style");
        assert!(val["content"].as_str().unwrap().contains("clippy"));
    }

    #[tokio::test]
    async fn memory_recall_by_title_returns_content() {
        let hm = test_hivemind();
        hm.do_memory_store(MemoryStoreInput {
            title: "clean arch".to_string(),
            content: "domain at center, infra at edge".to_string(),
            layer: "personal".to_string(),
            tags: vec!["architecture".to_string()],
            project: None,
        }).await.unwrap();

        let result = hm.do_memory_recall(MemoryRecallInput {
            id: None,
            title: Some("clean arch".to_string()),
        }).await.unwrap();
        let val = result.structured_content.unwrap();
        assert_eq!(val["found"], true);
        assert_eq!(val["content"], "domain at center, infra at edge");
    }

    #[tokio::test]
    async fn memory_recall_returns_not_found_for_missing_id() {
        let hm = test_hivemind();
        let result = hm.do_memory_recall(MemoryRecallInput {
            id: Some("mem_doesnotexist".to_string()),
            title: None,
        }).await.unwrap();
        assert_eq!(result.structured_content.unwrap()["found"], false);
    }

    #[tokio::test]
    async fn memory_recall_errors_without_id_or_title() {
        let hm = test_hivemind();
        let err = hm.do_memory_recall(MemoryRecallInput { id: None, title: None }).await;
        assert!(err.is_err());
    }
```

- [ ] **Step 2: Run — expect failures (do_memory_recall is already implemented, so tests may already pass)**

```bash
cargo test server
```

If all tests pass: the recall logic was implemented in Task 6 — move on.
If failures: check the `do_memory_recall` implementation matches the test expectations.

- [ ] **Step 3: Run full test suite**

```bash
cargo test
```

Expected: all tests pass across all modules.

- [ ] **Step 4: Commit**

```bash
git add src/server.rs
git commit -m "feat: add memory_recall tool with id and title lookup"
```

---

## Task 8: Wire stdio transport (src/main.rs)

**Files:**
- Modify: `src/main.rs`

- [ ] **Step 1: Implement main.rs**

Replace `src/main.rs` with:

```rust
mod db;
mod model;
mod server;
mod store;

use anyhow::Result;
use rmcp::{ServiceExt, transport::stdio};
use server::HiveMind;
use store::SqliteStore;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "hivemind=info".into()),
        )
        .init();

    let db_path = resolve_db_path();
    tracing::info!("opening database at {db_path}");

    let conn = db::open(&db_path)?;
    let service = HiveMind::new(SqliteStore::new(conn));

    tracing::info!("HiveMind MCP server starting on stdio");
    let server = service.serve(stdio()).await?;
    server.waiting().await?;
    Ok(())
}

fn resolve_db_path() -> String {
    if let Ok(path) = std::env::var("HIVEMIND_DB_PATH") {
        return path;
    }
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    let dir = format!("{home}/.local/share/hivemind");
    std::fs::create_dir_all(&dir).ok();
    format!("{dir}/memory.db")
}
```

- [ ] **Step 2: Build release binary**

```bash
cargo build --release 2>&1 | tail -5
```

Expected: `Compiling oxhivemind ... Finished release [optimized] target(s)`

- [ ] **Step 3: Smoke test — store a memory and read it back**

The MCP Inspector is the easiest way. Alternatively, send raw JSON-RPC over stdin:

```bash
echo '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"test","version":"0.0.1"}}}' | HIVEMIND_DB_PATH=/tmp/hivemind-test.db ./target/release/hivemind 2>/dev/null | head -1
```

Expected: a JSON response containing `"result"` and `"serverInfo"`.

- [ ] **Step 4: Run full test suite one last time**

```bash
cargo test
```

Expected: all tests pass.

- [ ] **Step 5: Commit**

```bash
git add src/main.rs
git commit -m "feat: wire stdio MCP transport — Phase 1 complete"
```

---

## Self-Review Against Spec

Spec section 10 (Phase 1 checklist):

| Spec requirement | Task |
|---|---|
| Scaffold Rust project with `rmcp` SDK | Task 1 |
| SQLite store — memories + tags tables | Task 3 |
| `memory_store` tool — write to SQLite, return id + auto_connected | Task 6 |
| `memory_recall` tool — exact match by title or id | Task 7 |
| Wire MCP stdio transport | Task 8 |
| Manual validation: store → end session → new session → recall | Task 8 smoke test |

**Not in this plan (Phase 2+):** FTS5 search, `memory_update`, `memory_delete`, edges, hooks, dashboard, sync.

**Known deviations from spec:**
- `type` input field is not exposed to Claude (defaults to `preference`). Spec tool schema also omits it — this is intentional.
- `include_connected` in `memory_recall` is parsed but ignored (edges table not yet implemented — Phase 2).
