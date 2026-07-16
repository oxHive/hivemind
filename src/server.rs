use crate::store::SqliteStore;
use rmcp::{
    handler::server::wrapper::Parameters,
    model::{CallToolResult, ErrorData, PromptMessage, Role},
    prompt, prompt_handler, prompt_router, schemars, tool, tool_handler, tool_router,
};
use serde::Deserialize;
use serde_json::json;
use std::sync::Arc;

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct MemoryStoreInput {
    /// Short descriptive title for this memory
    pub title: String,
    /// Full content to store
    pub content: String,
    /// Tags for search and auto-linking
    #[serde(default)]
    pub tags: Vec<String>,
    /// Optional token count hint
    #[serde(default)]
    pub token_count: Option<i64>,
    /// Memory layer: "personal" (follows the user) or "workspace" (project-scoped). Default: workspace.
    #[serde(default)]
    pub layer: Option<String>,
    /// Memory type: "preference" | "project" | "history". Default: project.
    #[serde(default)]
    pub memory_type: Option<String>,
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
    /// Keywords to search memory titles and content. Optional if `tags` is
    /// provided — a pure tag-boolean search with no keyword component.
    #[serde(default)]
    pub query: Option<String>,
    /// Require all of these tags (namespace:value form, e.g. "lang:rust").
    /// ANDed together, and ANDed with `query` if both are provided.
    #[serde(default)]
    pub tags: Option<Vec<String>>,
    /// Max results (default 5, capped at 10)
    #[serde(default)]
    pub limit: Option<i64>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct MemoryUpdateInput {
    /// ID of the memory to update (mem_xxx)
    pub id: String,
    /// New title (omit to keep current)
    #[serde(default)]
    pub title: Option<String>,
    /// New content (omit to keep current)
    #[serde(default)]
    pub content: Option<String>,
    /// Replace all tags with these (omit to keep current)
    #[serde(default)]
    pub tags: Option<Vec<String>>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct MemoryDeleteInput {
    /// ID of the memory to delete (mem_xxx)
    pub id: String,
    /// Must be true. Confirm with the user before deleting — deletion is permanent.
    pub confirm: bool,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct MemorySearchPromptInput {
    /// Keywords to search for in memory titles and content
    pub query: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct MemoryIdInput {
    /// Memory ID to target (e.g. mem_abc123)
    pub id: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct MemoryFlagInput {
    /// Memory ID to flag
    pub id: String,
    /// Reason for flagging: "incorrect", "outdated", "duplicate", "other"
    #[serde(default = "default_flag_reason")]
    pub reason: String,
    /// Optional note explaining the issue
    #[serde(default)]
    pub note: Option<String>,
}

fn default_flag_reason() -> String {
    "other".to_string()
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ConflictIdInput {
    /// Conflict ID to merge (conflict_xxx)
    pub id: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SessionStartInput {
    /// Absolute path to the project root where .hivemind.toml lives.
    pub project_path: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct MemoryStoreEdgeInput {
    /// Source memory ID (mem_xxx)
    pub source_id: String,
    /// Target memory ID (mem_xxx)
    pub target_id: String,
    /// Relationship type: "parent" (target is a broader principle/context this
    /// falls under) | "child" (target is a specific instance of this) |
    /// "sibling" (a peer, no hierarchy implied)
    pub relationship: String,
    /// Optional lifecycle status: "active" (default, confirmed) or "pending"
    /// (a suggestion awaiting user review in the dashboard).
    #[serde(default)]
    pub status: Option<String>,
    /// Short rationale for the connection, shown to the user during review.
    #[serde(default)]
    pub reason: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct MemoryUpdateEdgeInput {
    /// Edge ID to update (edge_xxx)
    pub id: String,
    /// New relationship: "parent" | "child" | "sibling". Omit to keep current.
    #[serde(default)]
    pub relationship: Option<String>,
    /// New rationale shown during review. Omit to keep current.
    #[serde(default)]
    pub reason: Option<String>,
    /// New mention anchor text. Omit to keep current.
    #[serde(default)]
    pub link_text: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct MemoryGetEdgesInput {
    /// Memory ID (mem_xxx) to get connections for
    pub memory_id: String,
}

#[derive(Clone)]
pub struct HiveMind {
    store: Arc<SqliteStore>,
    sync_trigger: Option<Arc<tokio::sync::Notify>>,
    events: Option<tokio::sync::broadcast::Sender<serde_json::Value>>,
}

impl HiveMind {
    #[cfg(test)]
    pub fn new(store: SqliteStore) -> Self {
        Self {
            store: Arc::new(store),
            sync_trigger: None,
            events: None,
        }
    }

    pub fn with_store(store: Arc<SqliteStore>) -> Self {
        Self {
            store,
            sync_trigger: None,
            events: None,
        }
    }

    pub fn with_sync(store: Arc<SqliteStore>, trigger: Arc<tokio::sync::Notify>) -> Self {
        Self {
            store,
            sync_trigger: Some(trigger),
            events: None,
        }
    }

    /// Broadcasts a "changed" signal to dashboard SSE subscribers whenever a
    /// memory or edge is created/updated/deleted through this MCP handle.
    pub fn with_events(mut self, tx: tokio::sync::broadcast::Sender<serde_json::Value>) -> Self {
        self.events = Some(tx);
        self
    }

    fn notify_change(&self) {
        if let Some(tx) = &self.events {
            let _ = tx.send(serde_json::json!({ "type": "changed" }));
        }
    }

    pub async fn do_memory_store(&self, p: MemoryStoreInput) -> Result<CallToolResult, ErrorData> {
        let id = format!("mem_{}", uuid::Uuid::new_v4().simple());
        let title = p.title.clone();

        let layer = p.layer.as_deref().unwrap_or("workspace");
        layer
            .parse::<crate::model::Layer>()
            .map_err(|e| ErrorData::invalid_params(e.to_string(), None))?;

        let memory_type = p.memory_type.as_deref().unwrap_or("project");
        memory_type
            .parse::<crate::model::MemoryType>()
            .map_err(|e| ErrorData::invalid_params(e.to_string(), None))?;

        self.store
            .store(&crate::store::NewMemoryRow {
                id: &id,
                title: &p.title,
                content: &p.content,
                tags: &p.tags,
                token_count: p.token_count,
                layer,
                memory_type,
            })
            .await
            .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;
        if let Some(t) = &self.sync_trigger {
            t.notify_one();
        }
        self.notify_change();
        Ok(CallToolResult::structured(json!({
            "id": id,
            "title": title,
        })))
    }

    pub async fn do_memory_recall(
        &self,
        p: MemoryRecallInput,
    ) -> Result<CallToolResult, ErrorData> {
        let entry = if let Some(ref id) = p.id {
            self.store.recall_by_id(id).await
        } else if let Some(ref title) = p.title {
            self.store.recall_by_title(title).await
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
                "tags": e.tags,
                "created_at": e.created_at,
                "updated_at": e.updated_at,
                "layer": e.layer,
                "memory_type": e.memory_type,
            }))),
        }
    }

    pub async fn do_memory_search(
        &self,
        p: MemorySearchInput,
    ) -> Result<CallToolResult, ErrorData> {
        let limit = p.limit.unwrap_or(5).clamp(1, 10);
        let query = p.query.as_deref().map(str::trim).filter(|s| !s.is_empty());
        let tags = p.tags.filter(|t| !t.is_empty());

        if query.is_none() && tags.is_none() {
            return Ok(CallToolResult::structured(json!({
                "count": 0,
                "results": [],
            })));
        }

        let hits = match (query, tags) {
            (Some(q), Some(tags)) => {
                let expr = crate::tag_query::TagExpr::and_all(&tags)
                    .expect("tags checked non-empty above");
                let candidates = self
                    .store
                    .search(q, 50)
                    .await
                    .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;
                let mut filtered: Vec<_> = candidates
                    .into_iter()
                    .filter(|e| expr.eval(&e.tags))
                    .collect();
                filtered.truncate(limit as usize);
                filtered
            }
            (Some(q), None) => self
                .store
                .search(q, limit)
                .await
                .map_err(|e| ErrorData::internal_error(e.to_string(), None))?,
            (None, Some(tags)) => {
                let expr = crate::tag_query::TagExpr::and_all(&tags)
                    .expect("tags checked non-empty above");
                let mut results = self
                    .store
                    .find_by_tag_expr(&expr)
                    .await
                    .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;
                results.truncate(limit as usize);
                results
            }
            (None, None) => unreachable!("handled by the early return above"),
        };

        let results: Vec<_> = hits
            .iter()
            .map(|h| {
                let snippet: String = h.content.chars().take(200).collect();
                json!({
                    "id": h.id,
                    "title": h.title,
                    "snippet": snippet,
                    "tags": h.tags,
                    "layer": h.layer,
                })
            })
            .collect();
        Ok(CallToolResult::structured(json!({
            "count": results.len(),
            "results": results,
        })))
    }

    pub async fn do_memory_update(
        &self,
        p: MemoryUpdateInput,
    ) -> Result<CallToolResult, ErrorData> {
        // Fetch current state to fill in unchanged fields
        let current = self
            .store
            .recall_by_id(&p.id)
            .await
            .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;
        let current = match current {
            None => {
                return Ok(CallToolResult::structured(json!({
                    "updated": false,
                    "id": p.id,
                })));
            }
            Some(c) => c,
        };
        let title = p.title.as_deref().unwrap_or(&current.title);
        let content = p.content.as_deref().unwrap_or(&current.content);
        let tags = p.tags.as_deref().unwrap_or(&current.tags);
        let updated = self
            .store
            .update(&p.id, title, content, tags)
            .await
            .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;
        if updated {
            self.notify_change();
        }
        Ok(CallToolResult::structured(json!({
            "updated": updated,
            "id": p.id,
        })))
    }

    pub async fn do_memory_delete(
        &self,
        p: MemoryDeleteInput,
    ) -> Result<CallToolResult, ErrorData> {
        if !p.confirm {
            return Err(ErrorData::invalid_params(
                "Deletion is permanent and requires confirm: true. Confirm with the user first.",
                None,
            ));
        }
        let deleted = self
            .store
            .delete(&p.id)
            .await
            .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;
        if deleted {
            self.notify_change();
        }
        Ok(CallToolResult::structured(json!({
            "deleted": deleted,
            "id": p.id,
        })))
    }

    async fn do_memory_list_prompt(&self) -> Result<Vec<PromptMessage>, ErrorData> {
        let memories = self
            .store
            .list_memories(50, 0)
            .await
            .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;
        let count = memories.len();
        let body = if memories.is_empty() {
            "No memories stored yet. Use memory_store to add some.".to_string()
        } else {
            let lines: Vec<String> = memories
                .iter()
                .map(|m| {
                    let tags = if m.tags.is_empty() {
                        String::new()
                    } else {
                        format!(" [{}]", m.tags.join(", "))
                    };
                    format!("• {} — {}{}", m.id, m.title, tags)
                })
                .collect();
            format!(
                "HiveMind Memory List ({count} memories):\n\n{}",
                lines.join("\n")
            )
        };
        Ok(vec![PromptMessage::new_text(Role::User, body)])
    }

    async fn do_memory_status_prompt(&self) -> Result<Vec<PromptMessage>, ErrorData> {
        let count = self
            .store
            .count()
            .await
            .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;
        let recent = self
            .store
            .list_memories(5, 0)
            .await
            .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;

        let recent_lines: Vec<String> = recent
            .iter()
            .map(|m| format!("  \u{2022} {} \u{2014} {}", m.id, m.title))
            .collect();

        let mut parts = vec![
            "HiveMind Status".to_string(),
            "\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}".to_string(),
            format!("Total memories: {count}"),
        ];
        if !recent_lines.is_empty() {
            parts.push("\nRecent memories:".to_string());
            parts.extend(recent_lines);
        }
        parts.push("\nTip: Use /memory-list to browse all memories, or /memory-search <query> to find specific ones.".to_string());

        Ok(vec![PromptMessage::new_text(Role::User, parts.join("\n"))])
    }

    async fn do_memory_search_prompt(
        &self,
        p: MemorySearchPromptInput,
    ) -> Result<Vec<PromptMessage>, ErrorData> {
        let trimmed = p.query.trim().to_string();
        let hits = if trimmed.is_empty() {
            vec![]
        } else {
            self.store
                .search(&trimmed, 10)
                .await
                .map_err(|e| ErrorData::internal_error(e.to_string(), None))?
        };
        let body = if hits.is_empty() {
            format!(
                "No memories found matching \"{}\".\n\nTip: Try broader keywords or use /memory-list to browse all memories.",
                p.query
            )
        } else {
            let lines: Vec<String> = hits
                .iter()
                .map(|h| {
                    let snippet: String = h.content.chars().take(200).collect();
                    format!("\u{2022} {} \u{2014} {}\n  {}", h.id, h.title, snippet)
                })
                .collect();
            format!(
                "Search results for \"{}\" ({} found):\n\n{}\n\nUse memory_recall with an ID for full content.",
                p.query,
                hits.len(),
                lines.join("\n\n")
            )
        };
        Ok(vec![PromptMessage::new_text(Role::User, body)])
    }

    async fn do_memory_edit_prompt(
        &self,
        p: MemoryIdInput,
    ) -> Result<Vec<PromptMessage>, ErrorData> {
        let mem = self
            .store
            .recall_by_id(&p.id)
            .await
            .map_err(|e| ErrorData::internal_error(e.to_string(), None))?
            .ok_or_else(|| ErrorData::invalid_params(format!("Memory {} not found", p.id), None))?;
        let tags = mem.tags.join(", ");
        let body = format!(
            "Memory to edit:\n\
             ━━━━━━━━━━━━━━\n\
             ID:      {}\n\
             Title:   {}\n\
             Tags:    {}\n\
             Content:\n{}\n\
             ━━━━━━━━━━━━━━\n\n\
             Ask the user what changes they want to make, then call memory_update with ID {} to save.\n\
             You can update content and/or tags. Omit fields you are not changing.",
            mem.id, mem.title, tags, mem.content, mem.id
        );
        Ok(vec![PromptMessage::new_text(Role::User, body)])
    }

    async fn do_memory_flag_prompt(
        &self,
        p: MemoryFlagInput,
    ) -> Result<Vec<PromptMessage>, ErrorData> {
        let mem = self
            .store
            .recall_by_id(&p.id)
            .await
            .map_err(|e| ErrorData::internal_error(e.to_string(), None))?
            .ok_or_else(|| ErrorData::invalid_params(format!("Memory {} not found", p.id), None))?;
        self.store
            .create_feedback(&p.id, &p.reason, p.note.as_deref())
            .await
            .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;
        let body = format!(
            "Flagged memory \"{}\" ({}) as \"{}\".\n\
             A feedback record has been created and will appear in the dashboard under Feedback.\n\
             The memory has not been deleted — it remains available until a human reviews the flag.\n\
             {}",
            mem.title,
            mem.id,
            p.reason,
            p.note
                .as_ref()
                .map(|n| format!("Note: {n}"))
                .unwrap_or_default()
        );
        Ok(vec![PromptMessage::new_text(Role::User, body)])
    }

    async fn do_suggest_connections_prompt(&self) -> Result<Vec<PromptMessage>, ErrorData> {
        let body = build_suggest_prompt(&self.store)
            .await
            .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;
        Ok(vec![PromptMessage::new_text(Role::User, body)])
    }

    async fn do_review_feedback_prompt(&self) -> Result<Vec<PromptMessage>, ErrorData> {
        let open_items = self
            .store
            .list_feedback(None, Some("pending"))
            .await
            .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;

        if open_items.is_empty() {
            return Ok(vec![PromptMessage::new_text(
                Role::User,
                "No open feedback items. All flagged memories have been reviewed.\n\
                 Tip: Use /memory-flag <id> to flag a memory that needs attention."
                    .to_string(),
            )]);
        }

        let mut lines = vec![
            format!("Open Feedback Items ({} total)", open_items.len()),
            "\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}".to_string(),
        ];

        for (i, item) in open_items.iter().enumerate() {
            let note = item.note.as_deref().unwrap_or("(no note)");
            lines.push(format!(
                "\n{}. [{}] Memory: {} | {}\n   Note: {}",
                i + 1,
                item.signal,
                item.memory_id,
                item.id,
                note
            ));
        }

        lines.push("\n\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}".to_string());
        lines.push("For each item, choose an action:".to_string());
        lines.push(
            "  \u{2022} Fix the memory \u{2014} call memory_update with the memory ID".to_string(),
        );
        lines.push(
            "  \u{2022} Delete if truly wrong \u{2014} call memory_delete with confirm:true"
                .to_string(),
        );
        lines.push("\nAsk the user how to handle each item before taking action.".to_string());

        Ok(vec![PromptMessage::new_text(Role::User, lines.join("\n"))])
    }

    pub async fn do_session_start(
        &self,
        p: SessionStartInput,
    ) -> Result<CallToolResult, ErrorData> {
        let canon = std::fs::canonicalize(&p.project_path).map_err(|_| {
            ErrorData::invalid_params(
                format!("project_path does not exist: {}", p.project_path),
                None,
            )
        })?;
        if !canon.is_dir() {
            return Err(ErrorData::invalid_params(
                "project_path is not a directory",
                None,
            ));
        }

        let config = crate::config::load_config(&canon)
            .map_err(|e| ErrorData::invalid_params(e.to_string(), None))?;

        let result = crate::session::execute_session_start(&config, &self.store)
            .await
            .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;

        if let Err(e) = self
            .store
            .log_session_start(&canon.to_string_lossy(), &result)
            .await
        {
            tracing::warn!("failed to write session_start_log entry: {e:#}");
        }

        Ok(CallToolResult::structured(result.to_json()))
    }
}

#[tool_router]
impl HiveMind {
    #[tool(
        description = "Store a memory, preference, or project context for future recall across sessions. Use when the user explicitly asks to remember something, or when important context should persist beyond this session."
    )]
    async fn memory_store(
        &self,
        Parameters(p): Parameters<MemoryStoreInput>,
    ) -> Result<CallToolResult, ErrorData> {
        self.do_memory_store(p).await
    }

    #[tool(
        description = "Recall a memory by exact title or ID. Returns full content. Use memory_search to find candidates first."
    )]
    async fn memory_recall(
        &self,
        Parameters(p): Parameters<MemoryRecallInput>,
    ) -> Result<CallToolResult, ErrorData> {
        self.do_memory_recall(p).await
    }

    #[tool(
        description = "Search stored memories by keyword (FTS). Returns ranked snippets to conserve context — use memory_recall with an id for full content. Default 5 results, max 10."
    )]
    async fn memory_search(
        &self,
        Parameters(p): Parameters<MemorySearchInput>,
    ) -> Result<CallToolResult, ErrorData> {
        self.do_memory_search(p).await
    }

    #[tool(
        description = "Update an existing memory's content or tags by id. Providing tags replaces all tags on the memory."
    )]
    async fn memory_update(
        &self,
        Parameters(p): Parameters<MemoryUpdateInput>,
    ) -> Result<CallToolResult, ErrorData> {
        self.do_memory_update(p).await
    }

    #[tool(
        description = "Permanently delete a memory by id. Requires confirm=true; always confirm with the user before calling."
    )]
    async fn memory_delete(
        &self,
        Parameters(p): Parameters<MemoryDeleteInput>,
    ) -> Result<CallToolResult, ErrorData> {
        self.do_memory_delete(p).await
    }

    #[tool(
        description = "Call this once at the start of every session when .hivemind.toml exists in the project root. Returns pre-configured memory context for this project."
    )]
    async fn hivemind_session_start(
        &self,
        Parameters(p): Parameters<SessionStartInput>,
    ) -> Result<CallToolResult, ErrorData> {
        self.do_session_start(p).await
    }

    #[tool(
        description = "Store a confirmed connection between two memories. Use after the user or Claude explicitly decides two memories are related. Valid relationships: parent (target is a broader principle/context source falls under), child (target is a specific instance of source), sibling (a peer, no hierarchy)."
    )]
    async fn memory_store_edge(
        &self,
        Parameters(p): Parameters<MemoryStoreEdgeInput>,
    ) -> Result<CallToolResult, ErrorData> {
        self.do_memory_store_edge(p).await
    }

    pub async fn do_memory_store_edge(
        &self,
        p: MemoryStoreEdgeInput,
    ) -> Result<CallToolResult, ErrorData> {
        use crate::model::EdgeCreate;
        let status = p.status.as_deref().unwrap_or("active");
        if !["active", "pending"].contains(&status) {
            return Err(ErrorData::invalid_params(
                "status must be \"active\" or \"pending\"",
                None,
            ));
        }
        match self
            .store
            .create_edge_with_status(
                &p.source_id,
                &p.target_id,
                &p.relationship,
                status,
                None,
                p.reason.as_deref(),
            )
            .await
        {
            Ok(EdgeCreate::Created(id)) => {
                self.notify_change();
                Ok(CallToolResult::structured(json!({
                    "created": true, "id": id,
                    "source_id": p.source_id, "target_id": p.target_id, "relationship": p.relationship,
                    "status": status,
                })))
            }
            Ok(EdgeCreate::Duplicate) => Ok(CallToolResult::structured(json!({
                "created": false, "reason": "an edge between these memories with this relationship already exists",
            }))),
            Ok(EdgeCreate::MissingEndpoint) => Err(ErrorData::invalid_params(
                "source_id and target_id must both be existing, distinct memory IDs",
                None,
            )),
            Ok(EdgeCreate::InvalidRelationship) => Err(ErrorData::invalid_params(
                format!(
                    "relationship must be one of: {}",
                    crate::store::VALID_RELATIONSHIPS.join(", ")
                ),
                None,
            )),
            Err(e) => Err(ErrorData::internal_error(e.to_string(), None)),
        }
    }

    pub async fn do_memory_update_edge(
        &self,
        p: MemoryUpdateEdgeInput,
    ) -> Result<CallToolResult, ErrorData> {
        match self
            .store
            .update_edge(
                &p.id,
                p.relationship.as_deref(),
                p.reason.as_deref(),
                p.link_text.as_deref(),
            )
            .await
        {
            Ok(true) => {
                self.notify_change();
                Ok(CallToolResult::structured(
                    json!({ "updated": true, "id": p.id }),
                ))
            }
            Ok(false) => Ok(CallToolResult::structured(
                json!({ "updated": false, "id": p.id }),
            )),
            Err(e) => Err(ErrorData::invalid_params(e.to_string(), None)),
        }
    }

    #[tool(
        description = "Update an existing connection between memories: change its relationship, reason, or link text. Use when revising a suggested connection after user feedback. Does not change status; the user approves or rejects in the dashboard."
    )]
    async fn memory_update_edge(
        &self,
        Parameters(p): Parameters<MemoryUpdateEdgeInput>,
    ) -> Result<CallToolResult, ErrorData> {
        self.do_memory_update_edge(p).await
    }

    #[tool(
        description = "Get a memory's connections to other memories, grouped by relationship: parents (broader context this falls under), children (specific instances of this), and siblings (peers, no hierarchy). Call after memory_recall to see how a memory connects to the rest of what's stored."
    )]
    async fn memory_get_edges(
        &self,
        Parameters(p): Parameters<MemoryGetEdgesInput>,
    ) -> Result<CallToolResult, ErrorData> {
        match self.store.get_edges_grouped(&p.memory_id).await {
            Ok(grouped) => Ok(CallToolResult::structured(
                serde_json::to_value(grouped).unwrap(),
            )),
            Err(e) => Err(ErrorData::internal_error(e.to_string(), None)),
        }
    }
}

#[prompt_router]
impl HiveMind {
    /// List all memories with titles and tags
    #[prompt(
        name = "memory-list",
        description = "List all stored memories with titles and tags. Use to browse what HiveMind knows before searching or editing."
    )]
    async fn memory_list_prompt(&self) -> Result<Vec<PromptMessage>, ErrorData> {
        self.do_memory_list_prompt().await
    }

    /// Show the current memory count and recent activity
    #[prompt(
        name = "memory-status",
        description = "Show total memory count and recent memories. Use at the start of a session to understand what context is available."
    )]
    async fn memory_status_prompt(&self) -> Result<Vec<PromptMessage>, ErrorData> {
        self.do_memory_status_prompt().await
    }

    /// Search memories by keyword and present results
    #[prompt(
        name = "memory-search",
        description = "Search HiveMind memories by keyword. Returns matching memories with content snippets. Follow up with memory_recall for full content."
    )]
    async fn memory_search_prompt(
        &self,
        Parameters(p): Parameters<MemorySearchPromptInput>,
    ) -> Result<Vec<PromptMessage>, ErrorData> {
        self.do_memory_search_prompt(p).await
    }

    /// Fetch a memory and return a prompt for editing its content or tags
    #[prompt(
        name = "memory-edit",
        description = "Fetch a specific memory by ID and present its current content for editing. After reviewing, call memory_update to save changes."
    )]
    async fn memory_edit_prompt(
        &self,
        Parameters(p): Parameters<MemoryIdInput>,
    ) -> Result<Vec<PromptMessage>, ErrorData> {
        self.do_memory_edit_prompt(p).await
    }

    /// Flag a memory as incorrect, outdated, or duplicate
    #[prompt(
        name = "memory-flag",
        description = "Flag a memory for review. Creates a feedback record with the specified reason."
    )]
    async fn memory_flag_prompt(
        &self,
        Parameters(p): Parameters<MemoryFlagInput>,
    ) -> Result<Vec<PromptMessage>, ErrorData> {
        self.do_memory_flag_prompt(p).await
    }

    /// Analyze the memory graph and suggest new connections between related memories
    #[prompt(
        name = "suggest-connections",
        description = "Fetch all memories and existing connections, then analyze them to suggest new edges."
    )]
    async fn suggest_connections_prompt(&self) -> Result<Vec<PromptMessage>, ErrorData> {
        self.do_suggest_connections_prompt().await
    }

    /// Surface all open feedback items for interactive review and resolution
    #[prompt(
        name = "review-feedback",
        description = "Fetch open feedback items (flagged memories) and present them for interactive resolution."
    )]
    async fn review_feedback_prompt(&self) -> Result<Vec<PromptMessage>, ErrorData> {
        self.do_review_feedback_prompt().await
    }
}

pub(crate) async fn build_suggest_prompt(store: &SqliteStore) -> anyhow::Result<String> {
    let memories = store.list_memories(100, 0).await?;
    let edges = store.list_edges(None).await?;

    if memories.is_empty() {
        return Ok(
            "No memories stored yet. Add some memories first with memory_store.".to_string(),
        );
    }

    let mem_lines: Vec<String> = memories
        .iter()
        .map(|m| {
            let tags = if m.tags.is_empty() {
                String::new()
            } else {
                format!(" [{}]", m.tags.join(", "))
            };
            let snippet: String = m.content.chars().take(80).collect();
            let ellipsis = if m.content.len() > 80 { "…" } else { "" };
            format!("{} | {} | {}{}", m.id, m.title, snippet, ellipsis) + &tags
        })
        .collect();

    let edge_lines: Vec<String> = edges
        .iter()
        .map(|e| {
            format!(
                "{} --[{}]--> {} ({})",
                e.source_id, e.relationship, e.target_id, e.status
            )
        })
        .collect();

    let edge_section = if edge_lines.is_empty() {
        "  (none yet)".to_string()
    } else {
        edge_lines.join("\n")
    };

    Ok(format!(
        "HiveMind — Suggest Connections\n\
         ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n\
         You have {} memories and {} existing connections.\n\n\
         MEMORIES:\n\
         {}\n\n\
         EXISTING CONNECTIONS:\n\
         {}\n\n\
         ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n\
         Analyze the memories above and identify meaningful connections not yet captured.\n\
         For each suggested connection, call the memory_store_edge tool with:\n\
         \x20\x20source_id, target_id, relationship, status: \"pending\",\n\
         \x20\x20and a one-sentence reason explaining the connection.\n\
         Relationship types:\n\
         \x20\x20parent  - target is a broader principle/context source falls under\n\
         \x20\x20child   - target is a specific instance of source\n\
         \x20\x20sibling - a peer, no hierarchy\n\
         Suggest 3-7 connections. Focus on cross-domain insights.",
        memories.len(),
        edges.len(),
        mem_lines.join("\n"),
        edge_section,
    ))
}

#[tool_handler]
#[prompt_handler]
impl rmcp::ServerHandler for HiveMind {
    fn get_info(&self) -> rmcp::model::ServerInfo {
        rmcp::model::ServerInfo::new(
            rmcp::model::ServerCapabilities::builder()
                .enable_tools()
                .enable_prompts()
                .build(),
        )
        .with_server_info(rmcp::model::Implementation::new(
            "hivemind",
            env!("CARGO_PKG_VERSION"),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{config::SyncSettings, db, store::SqliteStore};
    use rmcp::model::ContentBlock;
    use tempfile::TempDir;

    async fn test_hivemind() -> (HiveMind, TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.db");
        let sync = SyncSettings::default();
        let database = db::open_database(&sync, path.to_str().unwrap())
            .await
            .unwrap();
        let conn = database.connect().unwrap();
        db::run_migrations(&conn).await.unwrap();
        (HiveMind::new(SqliteStore::new(conn)), dir)
    }

    async fn seed_two(hm: &HiveMind) -> (String, String) {
        for (t, c) in [("alpha", "a"), ("beta", "b")] {
            hm.do_memory_store(MemoryStoreInput {
                title: t.to_string(),
                content: c.to_string(),
                tags: vec![],
                token_count: None,
                layer: None,
                memory_type: None,
            })
            .await
            .unwrap();
        }
        let mems = hm.store.list_memories(10, 0).await.unwrap();
        let a = mems.iter().find(|m| m.title == "alpha").unwrap().id.clone();
        let b = mems.iter().find(|m| m.title == "beta").unwrap().id.clone();
        (a, b)
    }

    #[tokio::test]
    async fn memory_store_edge_accepts_pending_status_and_reason() {
        let (hm, _dir) = test_hivemind().await;
        let (a, b) = seed_two(&hm).await;
        hm.do_memory_store_edge(MemoryStoreEdgeInput {
            source_id: a,
            target_id: b,
            relationship: "sibling".into(),
            status: Some("pending".into()),
            reason: Some("both about testing".into()),
        })
        .await
        .unwrap();
        let edges = hm.store.list_edges(None).await.unwrap();
        assert_eq!(edges[0].status, "pending");
        assert_eq!(edges[0].reason.as_deref(), Some("both about testing"));
    }

    #[tokio::test]
    async fn memory_store_edge_rejects_bogus_status() {
        let (hm, _dir) = test_hivemind().await;
        let (a, b) = seed_two(&hm).await;
        let err = hm
            .do_memory_store_edge(MemoryStoreEdgeInput {
                source_id: a,
                target_id: b,
                relationship: "sibling".into(),
                status: Some("rejected".into()),
                reason: None,
            })
            .await;
        assert!(err.is_err(), "storing directly as rejected makes no sense");
    }

    #[tokio::test]
    async fn suggest_prompt_instructs_pending_status() {
        let (hm, _dir) = test_hivemind().await;
        let (_a, _b) = seed_two(&hm).await;
        let msgs = hm.do_suggest_connections_prompt().await.unwrap();
        let text = prompt_text(&msgs[0]);
        assert!(text.contains("status: \"pending\""));
        assert!(text.contains("reason"));
    }

    #[tokio::test]
    async fn get_info_advertises_name_and_tools_capability() {
        use rmcp::ServerHandler;
        let (hm, _dir) = test_hivemind().await;
        let info = hm.get_info();
        assert_eq!(info.server_info.name, "hivemind");
        assert!(
            info.capabilities.tools.is_some(),
            "tools capability must be advertised"
        );
    }

    #[tokio::test]
    async fn get_info_advertises_prompts_capability() {
        use rmcp::ServerHandler;
        let (hm, _dir) = test_hivemind().await;
        let info = hm.get_info();
        assert!(
            info.capabilities.prompts.is_some(),
            "prompts capability must be advertised"
        );
    }

    #[test]
    fn list_prompts_returns_memory_list() {
        let prompts = HiveMind::prompt_router().list_all();
        let names: Vec<&str> = prompts.iter().map(|p| p.name.as_str()).collect();
        assert!(
            names.contains(&"memory-list"),
            "memory-list prompt must be listed"
        );
    }

    #[tokio::test]
    async fn memory_store_tool_returns_mem_id() {
        let (hm, _dir) = test_hivemind().await;
        let result = hm
            .do_memory_store(MemoryStoreInput {
                title: "my preference".to_string(),
                content: "prefer tabs over spaces".to_string(),
                tags: vec!["style".to_string()],
                token_count: None,
                layer: None,
                memory_type: None,
            })
            .await
            .unwrap();
        let val = result.structured_content.unwrap();
        assert!(val["id"].as_str().unwrap().starts_with("mem_"));
    }

    #[tokio::test]
    async fn memory_recall_by_id_returns_content() {
        let (hm, _dir) = test_hivemind().await;
        let stored = hm
            .do_memory_store(MemoryStoreInput {
                title: "rust style".to_string(),
                content: "use clippy, rustfmt, and deny warnings".to_string(),
                tags: vec!["rust".to_string()],
                token_count: None,
                layer: None,
                memory_type: None,
            })
            .await
            .unwrap();
        let id = stored.structured_content.unwrap()["id"]
            .as_str()
            .unwrap()
            .to_string();

        let result = hm
            .do_memory_recall(MemoryRecallInput {
                id: Some(id),
                title: None,
            })
            .await
            .unwrap();
        let val = result.structured_content.unwrap();
        assert_eq!(val["found"], true);
        assert_eq!(val["title"], "rust style");
        assert!(val["content"].as_str().unwrap().contains("clippy"));
    }

    #[tokio::test]
    async fn memory_recall_by_title_returns_content() {
        let (hm, _dir) = test_hivemind().await;
        hm.do_memory_store(MemoryStoreInput {
            title: "clean arch".to_string(),
            content: "domain at center, infra at edge".to_string(),
            tags: vec!["architecture".to_string()],
            token_count: None,
            layer: None,
            memory_type: None,
        })
        .await
        .unwrap();

        let result = hm
            .do_memory_recall(MemoryRecallInput {
                id: None,
                title: Some("clean arch".to_string()),
            })
            .await
            .unwrap();
        let val = result.structured_content.unwrap();
        assert_eq!(val["found"], true);
        assert_eq!(val["content"], "domain at center, infra at edge");
    }

    #[tokio::test]
    async fn memory_recall_returns_not_found_for_missing_id() {
        let (hm, _dir) = test_hivemind().await;
        let result = hm
            .do_memory_recall(MemoryRecallInput {
                id: Some("mem_doesnotexist".to_string()),
                title: None,
            })
            .await
            .unwrap();
        assert_eq!(result.structured_content.unwrap()["found"], false);
    }

    #[tokio::test]
    async fn memory_recall_errors_without_id_or_title() {
        let (hm, _dir) = test_hivemind().await;
        let err = hm
            .do_memory_recall(MemoryRecallInput {
                id: None,
                title: None,
            })
            .await;
        assert!(err.is_err());
    }

    #[tokio::test]
    async fn memory_search_returns_snippets() {
        let (hm, _dir) = test_hivemind().await;
        hm.do_memory_store(MemoryStoreInput {
            title: "db driver choice".to_string(),
            content: "we standardized on pgx v5 for postgres".to_string(),
            tags: vec!["golang".to_string(), "database".to_string()],
            token_count: None,
            layer: None,
            memory_type: None,
        })
        .await
        .unwrap();

        let result = hm
            .do_memory_search(MemorySearchInput {
                query: Some("pgx".to_string()),
                tags: None,
                limit: None,
            })
            .await
            .unwrap();
        let val = result.structured_content.unwrap();
        assert_eq!(val["count"], 1);
        assert_eq!(val["results"][0]["title"], "db driver choice");
        assert!(
            val["results"][0]["snippet"]
                .as_str()
                .unwrap()
                .to_lowercase()
                .contains("pgx")
        );
        assert!(
            val["results"][0].get("content").is_none(),
            "search returns snippets, not full content"
        );
    }

    #[tokio::test]
    async fn memory_search_empty_query_returns_zero() {
        let (hm, _dir) = test_hivemind().await;
        let result = hm
            .do_memory_search(MemorySearchInput {
                query: Some("  ".to_string()),
                tags: None,
                limit: None,
            })
            .await
            .unwrap();
        assert_eq!(result.structured_content.unwrap()["count"], 0);
    }

    #[tokio::test]
    async fn memory_search_by_tags_only() {
        let (hm, _dir) = test_hivemind().await;
        hm.do_memory_store(MemoryStoreInput {
            title: "rust preferences".to_string(),
            content: "use anyhow for errors".to_string(),
            tags: vec!["lang:rust".to_string(), "project:hivemind".to_string()],
            token_count: None,
            layer: None,
            memory_type: None,
        })
        .await
        .unwrap();
        hm.do_memory_store(MemoryStoreInput {
            title: "vue preferences".to_string(),
            content: "use pinia for state".to_string(),
            tags: vec!["lang:vue".to_string(), "project:hivemind".to_string()],
            token_count: None,
            layer: None,
            memory_type: None,
        })
        .await
        .unwrap();

        let result = hm
            .do_memory_search(MemorySearchInput {
                query: None,
                tags: Some(vec!["lang:rust".to_string()]),
                limit: None,
            })
            .await
            .unwrap();
        let val = result.structured_content.unwrap();
        assert_eq!(val["count"], 1);
        assert_eq!(val["results"][0]["title"], "rust preferences");
    }

    #[tokio::test]
    async fn memory_search_query_and_tags_combined() {
        let (hm, _dir) = test_hivemind().await;
        hm.do_memory_store(MemoryStoreInput {
            title: "rust error handling".to_string(),
            content: "use anyhow for errors".to_string(),
            tags: vec!["lang:rust".to_string()],
            token_count: None,
            layer: None,
            memory_type: None,
        })
        .await
        .unwrap();
        hm.do_memory_store(MemoryStoreInput {
            title: "vue error handling".to_string(),
            content: "use error boundaries".to_string(),
            tags: vec!["lang:vue".to_string()],
            token_count: None,
            layer: None,
            memory_type: None,
        })
        .await
        .unwrap();

        // "error handling" FTS-matches both, but only the rust one carries the tag.
        let result = hm
            .do_memory_search(MemorySearchInput {
                query: Some("error handling".to_string()),
                tags: Some(vec!["lang:rust".to_string()]),
                limit: None,
            })
            .await
            .unwrap();
        let val = result.structured_content.unwrap();
        assert_eq!(val["count"], 1);
        assert_eq!(val["results"][0]["title"], "rust error handling");
    }

    #[tokio::test]
    async fn memory_update_changes_content() {
        let (hm, _dir) = test_hivemind().await;
        let stored = hm
            .do_memory_store(MemoryStoreInput {
                title: "deploy notes".to_string(),
                content: "uses docker swarm".to_string(),
                tags: vec!["devops".to_string()],
                token_count: None,
                layer: None,
                memory_type: None,
            })
            .await
            .unwrap();
        let id = stored.structured_content.unwrap()["id"]
            .as_str()
            .unwrap()
            .to_string();

        let result = hm
            .do_memory_update(MemoryUpdateInput {
                id: id.clone(),
                title: None,
                content: Some("migrated to kubernetes".to_string()),
                tags: None,
            })
            .await
            .unwrap();
        assert_eq!(result.structured_content.unwrap()["updated"], true);

        let recalled = hm
            .do_memory_recall(MemoryRecallInput {
                id: Some(id),
                title: None,
            })
            .await
            .unwrap();
        assert_eq!(
            recalled.structured_content.unwrap()["content"],
            "migrated to kubernetes"
        );
    }

    #[tokio::test]
    async fn memory_update_returns_updated_false_for_missing() {
        let (hm, _dir) = test_hivemind().await;
        let result = hm
            .do_memory_update(MemoryUpdateInput {
                id: "mem_nope".to_string(),
                title: None,
                content: None,
                tags: None,
            })
            .await
            .unwrap();
        assert_eq!(result.structured_content.unwrap()["updated"], false);
    }

    #[tokio::test]
    async fn session_start_loads_configured_recalls() {
        let (hm, _dir) = test_hivemind().await;
        hm.do_memory_store(MemoryStoreInput {
            title: "golang preferences".to_string(),
            content: "use uber/zap, sqlc, pgx v5".to_string(),
            tags: vec!["golang".to_string()],
            token_count: None,
            layer: None,
            memory_type: None,
        })
        .await
        .unwrap();

        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(
            tmp.path().join(".hivemind.toml"),
            "[project]\nname=\"demo\"\n[hooks.on_session_start]\nmax_tokens=2000\nrecalls=[\"golang preferences\"]\n",
        ).unwrap();

        let result = hm
            .do_session_start(SessionStartInput {
                project_path: tmp.path().to_string_lossy().into_owned(),
            })
            .await
            .unwrap();
        let val = result.structured_content.unwrap();
        assert_eq!(val["project"], "demo");
        assert_eq!(val["context_loaded"].as_array().unwrap().len(), 1);
        assert_eq!(val["context_loaded"][0]["title"], "golang preferences");
        assert_eq!(val["budget"]["truncated"], false);
        assert!(val["budget"]["used_tokens"].as_u64().unwrap() > 0);
    }

    #[tokio::test]
    async fn session_start_writes_a_log_entry() {
        let (hm, _dir) = test_hivemind().await;
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(
            tmp.path().join(".hivemind.toml"),
            "[project]\nname=\"demo\"\n[hooks.on_session_start]\nmax_tokens=2000\nrecalls=[]\n",
        )
        .unwrap();

        hm.do_session_start(SessionStartInput {
            project_path: tmp.path().to_string_lossy().into_owned(),
        })
        .await
        .unwrap();

        let logs = hm.store.list_session_logs(10).await.unwrap();
        assert_eq!(logs.len(), 1);
        assert_eq!(logs[0].project_name, "demo");
        assert_eq!(logs[0].project_path, tmp.path().to_string_lossy());
    }

    #[tokio::test]
    async fn session_start_rejects_nonexistent_path() {
        let (hm, _dir) = test_hivemind().await;
        let err = hm
            .do_session_start(SessionStartInput {
                project_path: "/no/such/dir/anywhere".to_string(),
            })
            .await;
        assert!(err.is_err());
    }

    #[tokio::test]
    async fn session_start_errors_without_config() {
        let (hm, _dir) = test_hivemind().await;
        let tmp = tempfile::tempdir().unwrap();
        let err = hm
            .do_session_start(SessionStartInput {
                project_path: tmp.path().to_string_lossy().into_owned(),
            })
            .await;
        assert!(err.is_err());
    }

    fn prompt_text(msg: &PromptMessage) -> &str {
        match &msg.content {
            ContentBlock::Text(t) => t.text.as_str(),
            _ => panic!("expected text content"),
        }
    }

    #[tokio::test]
    async fn memory_list_prompt_returns_no_memories_message() {
        let (hm, _dir) = test_hivemind().await;
        let result = hm.do_memory_list_prompt().await.unwrap();
        assert_eq!(result.len(), 1);
        assert!(prompt_text(&result[0]).contains("No memories"));
    }

    #[tokio::test]
    async fn memory_status_prompt_includes_count() {
        let (hm, _dir) = test_hivemind().await;
        hm.do_memory_store(MemoryStoreInput {
            title: "test".to_string(),
            content: "c".to_string(),
            tags: vec![],
            token_count: None,
            layer: None,
            memory_type: None,
        })
        .await
        .unwrap();
        let result = hm.do_memory_status_prompt().await.unwrap();
        let text = prompt_text(&result[0]);
        assert!(text.contains("1"), "status should show count of 1");
    }

    #[tokio::test]
    async fn memory_search_prompt_returns_results() {
        let (hm, _dir) = test_hivemind().await;
        hm.do_memory_store(MemoryStoreInput {
            title: "golang preferences".to_string(),
            content: "use uber/zap and chi router".to_string(),
            tags: vec!["golang".to_string()],
            token_count: None,
            layer: None,
            memory_type: None,
        })
        .await
        .unwrap();
        let result = hm
            .do_memory_search_prompt(MemorySearchPromptInput {
                query: "uber".to_string(),
            })
            .await
            .unwrap();
        let text = prompt_text(&result[0]);
        assert!(text.contains("uber") || text.contains("golang"));
    }

    #[tokio::test]
    async fn memory_edit_prompt_returns_formatted_content() {
        let (hm, _dir) = test_hivemind().await;
        let stored = hm
            .do_memory_store(MemoryStoreInput {
                title: "rust style".to_string(),
                content: "use clippy and rustfmt".to_string(),
                tags: vec!["rust".to_string()],
                token_count: None,
                layer: None,
                memory_type: None,
            })
            .await
            .unwrap();
        let id = stored.structured_content.unwrap()["id"]
            .as_str()
            .unwrap()
            .to_string();

        let result = hm
            .do_memory_edit_prompt(MemoryIdInput { id: id.clone() })
            .await
            .unwrap();
        let text = prompt_text(&result[0]);
        assert!(text.contains("rust style"), "should include memory title");
        assert!(text.contains("clippy"), "should include memory content");
        assert!(text.contains(&id), "should include the ID");
    }

    #[tokio::test]
    async fn memory_edit_prompt_returns_error_for_missing_id() {
        let (hm, _dir) = test_hivemind().await;
        let result = hm
            .do_memory_edit_prompt(MemoryIdInput {
                id: "mem_nonexistent".to_string(),
            })
            .await;
        assert!(result.is_err(), "should error when memory not found");
    }

    #[tokio::test]
    async fn memory_flag_prompt_creates_feedback_record() {
        let (hm, _dir) = test_hivemind().await;
        let stored = hm
            .do_memory_store(MemoryStoreInput {
                title: "test".to_string(),
                content: "c".to_string(),
                tags: vec![],
                token_count: None,
                layer: None,
                memory_type: None,
            })
            .await
            .unwrap();
        let id = stored.structured_content.unwrap()["id"]
            .as_str()
            .unwrap()
            .to_string();

        let result = hm
            .do_memory_flag_prompt(MemoryFlagInput {
                id: id.clone(),
                reason: "outdated".to_string(),
                note: None,
            })
            .await
            .unwrap();
        let text = prompt_text(&result[0]);
        assert!(
            text.to_lowercase().contains("flagged"),
            "should confirm the flag"
        );

        let feedback = hm.store.list_feedback(None, None).await.unwrap();
        assert_eq!(feedback.len(), 1, "feedback record should be created");
    }

    #[tokio::test]
    async fn suggest_connections_prompt_lists_memories_and_edges() {
        let (hm, _dir) = test_hivemind().await;
        hm.do_memory_store(MemoryStoreInput {
            title: "golang preferences".to_string(),
            content: "use uber/zap and chi router".to_string(),
            tags: vec!["golang".to_string()],
            token_count: None,
            layer: None,
            memory_type: None,
        })
        .await
        .unwrap();
        hm.do_memory_store(MemoryStoreInput {
            title: "observability stack".to_string(),
            content: "prometheus, grafana, loki".to_string(),
            tags: vec!["observability".to_string()],
            token_count: None,
            layer: None,
            memory_type: None,
        })
        .await
        .unwrap();
        let result = hm.do_suggest_connections_prompt().await.unwrap();
        let text = prompt_text(&result[0]);
        assert!(
            text.contains("golang preferences"),
            "should include memory titles"
        );
        assert!(
            text.contains("memory_store_edge"),
            "should instruct Claude to use the memory_store_edge tool to create edges"
        );
    }

    #[tokio::test]
    async fn memory_store_accepts_layer_and_rejects_invalid() {
        let (hm, _dir) = test_hivemind().await;
        let ok = hm
            .do_memory_store(MemoryStoreInput {
                title: "t".into(),
                content: "c".into(),
                tags: vec![],
                token_count: None,
                layer: Some("personal".into()),
                memory_type: Some("preference".into()),
            })
            .await
            .unwrap();
        let id = ok.structured_content.unwrap()["id"]
            .as_str()
            .unwrap()
            .to_string();
        let recalled = hm
            .do_memory_recall(MemoryRecallInput {
                id: Some(id),
                title: None,
            })
            .await
            .unwrap();
        let val = recalled.structured_content.unwrap();
        assert_eq!(val["layer"], "personal");
        assert_eq!(val["memory_type"], "preference");

        let bad = hm
            .do_memory_store(MemoryStoreInput {
                title: "t".into(),
                content: "c".into(),
                tags: vec![],
                token_count: None,
                layer: Some("cosmic".into()),
                memory_type: None,
            })
            .await;
        assert!(bad.is_err());
    }

    #[tokio::test]
    async fn review_feedback_prompt_shows_open_items() {
        let (hm, _dir) = test_hivemind().await;
        let stored = hm
            .do_memory_store(MemoryStoreInput {
                title: "old pref".to_string(),
                content: "stale content".to_string(),
                tags: vec![],
                token_count: None,
                layer: None,
                memory_type: None,
            })
            .await
            .unwrap();
        let mem_id = stored.structured_content.unwrap()["id"]
            .as_str()
            .unwrap()
            .to_string();
        hm.store
            .create_feedback(&mem_id, "outdated", Some("This is outdated"))
            .await
            .unwrap();

        let result = hm.do_review_feedback_prompt().await.unwrap();
        let text = prompt_text(&result[0]);
        assert!(
            text.contains("outdated") || text.contains("old pref"),
            "should show feedback items"
        );
    }

    #[tokio::test]
    async fn review_feedback_prompt_empty_when_no_open_items() {
        let (hm, _dir) = test_hivemind().await;
        let result = hm.do_review_feedback_prompt().await.unwrap();
        let text = prompt_text(&result[0]);
        assert!(
            text.to_lowercase().contains("no open"),
            "should indicate no items"
        );
    }

    #[tokio::test]
    async fn with_store_constructor() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.db");
        let sync = SyncSettings::default();
        let database = db::open_database(&sync, path.to_str().unwrap())
            .await
            .unwrap();
        let conn = database.connect().unwrap();
        db::run_migrations(&conn).await.unwrap();
        let store = Arc::new(SqliteStore::new(conn));
        let hm = HiveMind::with_store(Arc::clone(&store));
        assert!(hm.sync_trigger.is_none());
    }

    #[tokio::test]
    async fn with_sync_constructor_stores_trigger() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.db");
        let sync = SyncSettings::default();
        let database = db::open_database(&sync, path.to_str().unwrap())
            .await
            .unwrap();
        let conn = database.connect().unwrap();
        db::run_migrations(&conn).await.unwrap();
        let store = Arc::new(SqliteStore::new(conn));
        let trigger = Arc::new(tokio::sync::Notify::new());
        let hm = HiveMind::with_sync(store, trigger);
        assert!(hm.sync_trigger.is_some());
    }

    #[tokio::test]
    async fn memory_store_notifies_sync_trigger() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.db");
        let sync = SyncSettings::default();
        let database = db::open_database(&sync, path.to_str().unwrap())
            .await
            .unwrap();
        let conn = database.connect().unwrap();
        db::run_migrations(&conn).await.unwrap();
        let store = Arc::new(SqliteStore::new(conn));
        let trigger = Arc::new(tokio::sync::Notify::new());
        let hm = HiveMind::with_sync(store, Arc::clone(&trigger));

        let notified = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let notified2 = Arc::clone(&notified);
        let trigger2 = Arc::clone(&trigger);
        tokio::spawn(async move {
            trigger2.notified().await;
            notified2.store(true, std::sync::atomic::Ordering::Relaxed);
        });

        hm.do_memory_store(MemoryStoreInput {
            title: "t".to_string(),
            content: "c".to_string(),
            tags: vec![],
            token_count: None,
            layer: None,
            memory_type: None,
        })
        .await
        .unwrap();

        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        assert!(notified.load(std::sync::atomic::Ordering::Relaxed));
    }

    #[tokio::test]
    async fn suggest_connections_empty_store_returns_message() {
        let (hm, _dir) = test_hivemind().await;
        let result = hm.do_suggest_connections_prompt().await.unwrap();
        assert_eq!(result.len(), 1);
        let text = prompt_text(&result[0]);
        assert!(
            text.contains("No memories"),
            "empty store should say no memories"
        );
    }

    #[tokio::test]
    async fn session_start_rejects_file_path() {
        let (hm, _dir) = test_hivemind().await;
        let tmp = tempfile::tempdir().unwrap();
        let file_path = tmp.path().join("somefile.txt");
        std::fs::write(&file_path, "hello").unwrap();
        let err = hm
            .do_session_start(SessionStartInput {
                project_path: file_path.to_string_lossy().into_owned(),
            })
            .await;
        assert!(err.is_err());
    }

    #[tokio::test]
    async fn memory_list_prompt_with_memories_shows_titles() {
        let (hm, _dir) = test_hivemind().await;
        hm.do_memory_store(MemoryStoreInput {
            title: "my preference".to_string(),
            content: "use tabs".to_string(),
            tags: vec!["style".to_string()],
            token_count: None,
            layer: None,
            memory_type: None,
        })
        .await
        .unwrap();
        let result = hm.do_memory_list_prompt().await.unwrap();
        let text = prompt_text(&result[0]);
        assert!(text.contains("my preference"));
        assert!(text.contains("style"));
    }

    #[tokio::test]
    async fn memory_update_preserves_tags_when_not_specified() {
        let (hm, _dir) = test_hivemind().await;
        let stored = hm
            .do_memory_store(MemoryStoreInput {
                title: "tagged".to_string(),
                content: "original".to_string(),
                tags: vec!["keep".to_string()],
                token_count: None,
                layer: None,
                memory_type: None,
            })
            .await
            .unwrap();
        let id = stored.structured_content.unwrap()["id"]
            .as_str()
            .unwrap()
            .to_string();

        hm.do_memory_update(MemoryUpdateInput {
            id: id.clone(),
            title: None,
            content: Some("updated".to_string()),
            tags: None,
        })
        .await
        .unwrap();

        let recalled = hm
            .do_memory_recall(MemoryRecallInput {
                id: Some(id),
                title: None,
            })
            .await
            .unwrap();
        let val = recalled.structured_content.unwrap();
        assert_eq!(val["content"], "updated");
        let tags: Vec<_> = val["tags"].as_array().unwrap().iter().collect();
        assert!(tags.iter().any(|t| t.as_str() == Some("keep")));
    }

    #[test]
    fn all_seven_prompts_are_registered() {
        let prompts = HiveMind::prompt_router().list_all();
        let names: Vec<&str> = prompts.iter().map(|p| p.name.as_str()).collect();
        let expected = [
            "memory-list",
            "memory-status",
            "memory-search",
            "memory-edit",
            "memory-flag",
            "suggest-connections",
            "review-feedback",
        ];
        for name in &expected {
            assert!(
                names.contains(name),
                "prompt {name} must be registered; got: {names:?}"
            );
        }
        assert_eq!(prompts.len(), 7, "exactly 7 prompts expected");
    }

    #[tokio::test]
    async fn memory_get_edges_returns_grouped_connections() {
        let (hm, _dir) = test_hivemind().await;

        let parent = hm
            .do_memory_store(MemoryStoreInput {
                title: "Parent".to_string(),
                content: "parent body".to_string(),
                tags: vec![],
                token_count: None,
                layer: None,
                memory_type: None,
            })
            .await
            .unwrap();
        let parent_id = parent.structured_content.unwrap()["id"]
            .as_str()
            .unwrap()
            .to_string();

        let child = hm
            .do_memory_store(MemoryStoreInput {
                title: "Child".to_string(),
                content: "child body".to_string(),
                tags: vec![],
                token_count: None,
                layer: None,
                memory_type: None,
            })
            .await
            .unwrap();
        let child_id = child.structured_content.unwrap()["id"]
            .as_str()
            .unwrap()
            .to_string();

        // Child asserts Parent is its parent.
        hm.do_memory_update(MemoryUpdateInput {
            id: child_id.clone(),
            title: None,
            content: Some(format!("[the rule](parent:{parent_id})")),
            tags: None,
        })
        .await
        .unwrap();

        let from_child = hm
            .memory_get_edges(Parameters(MemoryGetEdgesInput {
                memory_id: child_id.clone(),
            }))
            .await
            .unwrap();
        let from_child = from_child.structured_content.unwrap();
        assert_eq!(from_child["parents"][0]["id"], parent_id);
        assert_eq!(from_child["parents"][0]["link_text"], "the rule");
        assert!(from_child["children"].as_array().unwrap().is_empty());

        let from_parent = hm
            .memory_get_edges(Parameters(MemoryGetEdgesInput {
                memory_id: parent_id.clone(),
            }))
            .await
            .unwrap();
        let from_parent = from_parent.structured_content.unwrap();
        assert_eq!(from_parent["children"][0]["id"], child_id);
        assert!(from_parent["parents"].as_array().unwrap().is_empty());
    }

    #[tokio::test]
    async fn memory_delete_requires_confirm() {
        let (hm, _dir) = test_hivemind().await;
        let stored = hm
            .do_memory_store(MemoryStoreInput {
                title: "temp".to_string(),
                content: "delete me".to_string(),
                tags: vec!["tmp".to_string()],
                token_count: None,
                layer: None,
                memory_type: None,
            })
            .await
            .unwrap();
        let id = stored.structured_content.unwrap()["id"]
            .as_str()
            .unwrap()
            .to_string();

        let err = hm
            .do_memory_delete(MemoryDeleteInput {
                id: id.clone(),
                confirm: false,
            })
            .await;
        assert!(err.is_err());
        assert!(
            hm.do_memory_recall(MemoryRecallInput {
                id: Some(id.clone()),
                title: None
            })
            .await
            .unwrap()
            .structured_content
            .unwrap()["found"]
                == true
        );

        let ok = hm
            .do_memory_delete(MemoryDeleteInput {
                id: id.clone(),
                confirm: true,
            })
            .await
            .unwrap();
        assert_eq!(ok.structured_content.unwrap()["deleted"], true);
        assert_eq!(
            hm.do_memory_recall(MemoryRecallInput {
                id: Some(id),
                title: None
            })
            .await
            .unwrap()
            .structured_content
            .unwrap()["found"],
            false
        );
    }

    #[tokio::test]
    async fn memory_update_edge_patches_pending_edge() {
        let (hm, _dir) = test_hivemind().await;
        let (a, b) = seed_two(&hm).await;
        hm.do_memory_store_edge(MemoryStoreEdgeInput {
            source_id: a,
            target_id: b,
            relationship: "sibling".into(),
            status: Some("pending".into()),
            reason: Some("first take".into()),
        })
        .await
        .unwrap();
        let edge_id = hm.store.list_edges(None).await.unwrap()[0].id.clone();

        hm.do_memory_update_edge(MemoryUpdateEdgeInput {
            id: edge_id.clone(),
            relationship: Some("parent".into()),
            reason: Some("a is the general rule".into()),
            link_text: None,
        })
        .await
        .unwrap();

        let e = hm.store.get_edge(&edge_id).await.unwrap().unwrap();
        assert_eq!(e.relationship, "parent");
        assert_eq!(e.reason.as_deref(), Some("a is the general rule"));
        assert_eq!(e.status, "pending");
    }

    #[tokio::test]
    async fn memory_update_edge_missing_id_reports_updated_false() {
        let (hm, _dir) = test_hivemind().await;
        let res = hm
            .do_memory_update_edge(MemoryUpdateEdgeInput {
                id: "edge_missing".into(),
                relationship: None,
                reason: Some("x".into()),
                link_text: None,
            })
            .await
            .unwrap();
        let v = res.structured_content.unwrap();
        assert_eq!(v["updated"], false);
    }

    #[tokio::test]
    async fn memory_update_edge_invalid_relationship_errors() {
        let (hm, _dir) = test_hivemind().await;
        let (a, b) = seed_two(&hm).await;
        hm.do_memory_store_edge(MemoryStoreEdgeInput {
            source_id: a,
            target_id: b,
            relationship: "sibling".into(),
            status: Some("pending".into()),
            reason: None,
        })
        .await
        .unwrap();
        let edge_id = hm.store.list_edges(None).await.unwrap()[0].id.clone();
        let res = hm
            .do_memory_update_edge(MemoryUpdateEdgeInput {
                id: edge_id,
                relationship: Some("related_to".into()),
                reason: None,
                link_text: None,
            })
            .await;
        assert!(res.is_err());
    }
}
