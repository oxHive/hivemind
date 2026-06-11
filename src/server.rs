use std::sync::Arc;
use rmcp::{
    handler::server::wrapper::Parameters,
    model::{CallToolResult, ErrorData},
    schemars, tool, tool_handler, tool_router,
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

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct MemorySearchInput {
    /// Keywords to search memory titles and content
    pub query: String,
    /// Max results (default 5, capped at 10)
    #[serde(default)]
    pub limit: Option<u32>,
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
        let result = self.store.store(new_memory)
            .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::structured(json!({
            "id": result.id,
            "title": title,
            "auto_connected": result.auto_connected
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

    pub async fn do_memory_search(&self, p: MemorySearchInput) -> Result<CallToolResult, ErrorData> {
        let limit = p.limit.unwrap_or(5).clamp(1, 10) as usize;
        let hits = self.store.search(&p.query, limit)
            .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;
        let results: Vec<_> = hits.iter().map(|h| json!({
            "id": h.id,
            "title": h.title,
            "snippet": h.snippet,
            "layer": h.layer.to_string(),
            "tags": h.tags,
        })).collect();
        Ok(CallToolResult::structured(json!({
            "count": results.len(),
            "results": results,
        })))
    }
}

#[tool_router]
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

    #[tool(description = "Search stored memories by keyword (FTS). Returns ranked snippets (not full content) to conserve context — use memory_recall with an id for full content. Default 5 results, max 10.")]
    async fn memory_search(
        &self,
        Parameters(p): Parameters<MemorySearchInput>,
    ) -> Result<CallToolResult, ErrorData> {
        self.do_memory_search(p).await
    }
}

#[tool_handler]
impl rmcp::ServerHandler for HiveMind {
    fn get_info(&self) -> rmcp::model::ServerInfo {
        rmcp::model::ServerInfo::new(
            rmcp::model::ServerCapabilities::builder().enable_tools().build(),
        )
        .with_server_info(rmcp::model::Implementation::new("hivemind", env!("CARGO_PKG_VERSION")))
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

    #[test]
    fn get_info_advertises_name_and_tools_capability() {
        use rmcp::ServerHandler;
        let info = test_hivemind().get_info();
        assert_eq!(info.server_info.name, "hivemind");
        assert!(info.capabilities.tools.is_some(), "tools capability must be advertised");
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

    #[tokio::test]
    async fn memory_search_returns_snippets() {
        let hm = test_hivemind();
        hm.do_memory_store(MemoryStoreInput {
            title: "db driver choice".to_string(),
            content: "we standardized on pgx v5 for postgres".to_string(),
            layer: "personal".to_string(),
            tags: vec!["golang".to_string(), "database".to_string()],
            project: None,
        }).await.unwrap();

        let result = hm.do_memory_search(MemorySearchInput {
            query: "pgx".to_string(),
            limit: None,
        }).await.unwrap();
        let val = result.structured_content.unwrap();
        assert_eq!(val["count"], 1);
        assert_eq!(val["results"][0]["title"], "db driver choice");
        assert!(val["results"][0]["snippet"].as_str().unwrap().to_lowercase().contains("pgx"));
        assert!(val["results"][0].get("content").is_none(), "search returns snippets, not full content");
    }

    #[tokio::test]
    async fn memory_search_empty_query_returns_zero() {
        let hm = test_hivemind();
        let result = hm.do_memory_search(MemorySearchInput { query: "  ".to_string(), limit: None }).await.unwrap();
        assert_eq!(result.structured_content.unwrap()["count"], 0);
    }
}
