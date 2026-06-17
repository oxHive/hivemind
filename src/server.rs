use crate::{
    model::{Layer, MemoryType, NewMemory},
    store::SqliteStore,
};
use rmcp::{
    RoleServer,
    handler::server::wrapper::Parameters,
    model::{
        CallToolResult, ErrorData, GetPromptRequestParams, GetPromptResult, ListPromptsResult,
        PaginatedRequestParams, PromptMessage, PromptMessageRole,
    },
    prompt, prompt_handler, prompt_router, schemars,
    service::RequestContext,
    tool, tool_handler, tool_router,
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
    /// If true, append `content` to existing content instead of replacing
    #[serde(default)]
    pub merge_content: Option<bool>,
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
    /// Conflict ID to merge (cfl_xxx)
    pub id: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SessionStartInput {
    /// Absolute path to the project root where .hivemind.toml lives.
    pub project_path: String,
}

#[derive(Clone)]
pub struct HiveMind {
    store: Arc<SqliteStore>,
    sync_trigger: Option<Arc<tokio::sync::Notify>>,
}

impl HiveMind {
    #[allow(dead_code)]
    pub fn new(store: SqliteStore) -> Self {
        Self {
            store: Arc::new(store),
            sync_trigger: None,
        }
    }

    pub fn with_store(store: Arc<SqliteStore>) -> Self {
        Self {
            store,
            sync_trigger: None,
        }
    }

    pub fn with_sync(store: Arc<SqliteStore>, trigger: Arc<tokio::sync::Notify>) -> Self {
        Self {
            store,
            sync_trigger: Some(trigger),
        }
    }

    pub async fn do_memory_store(&self, p: MemoryStoreInput) -> Result<CallToolResult, ErrorData> {
        let layer = p
            .layer
            .parse::<Layer>()
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
        let result = self
            .store
            .store(new_memory)
            .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;
        if let Some(t) = &self.sync_trigger {
            t.notify_one();
        }
        Ok(CallToolResult::structured(json!({
            "id": result.id,
            "title": title,
            "auto_connected": result.auto_connected
        })))
    }

    pub async fn do_memory_recall(
        &self,
        p: MemoryRecallInput,
    ) -> Result<CallToolResult, ErrorData> {
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

    pub async fn do_memory_search(
        &self,
        p: MemorySearchInput,
    ) -> Result<CallToolResult, ErrorData> {
        let limit = p.limit.unwrap_or(5).clamp(1, 10) as usize;
        let hits = self
            .store
            .search(&p.query, limit)
            .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;
        let results: Vec<_> = hits
            .iter()
            .map(|h| {
                json!({
                    "id": h.id,
                    "title": h.title,
                    "snippet": h.snippet,
                    "layer": h.layer.to_string(),
                    "tags": h.tags,
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
        let updated = self
            .store
            .update(
                &p.id,
                crate::model::UpdateMemory {
                    title: p.title,
                    content: p.content,
                    tags: p.tags,
                    merge_content: p.merge_content.unwrap_or(false),
                },
            )
            .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;
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
            .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::structured(json!({
            "deleted": deleted,
            "id": p.id,
        })))
    }

    async fn do_memory_list_prompt(&self) -> Result<Vec<PromptMessage>, ErrorData> {
        let memories = self
            .store
            .list_memories(None, 50)
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
                    format!("• {} — {}{} ({})", m.id, m.title, tags, m.layer)
                })
                .collect();
            format!(
                "HiveMind Memory List ({count} memories):\n\n{}",
                lines.join("\n")
            )
        };
        Ok(vec![PromptMessage::new_text(PromptMessageRole::User, body)])
    }

    async fn do_memory_status_prompt(&self) -> Result<Vec<PromptMessage>, ErrorData> {
        let count = self
            .store
            .count()
            .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;
        let recent = self
            .store
            .list_memories(None, 5)
            .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;
        let open_conflicts = self
            .store
            .list_conflicts(Some("open"))
            .map_err(|e| ErrorData::internal_error(e.to_string(), None))?
            .len();
        let open_feedback = self
            .store
            .list_feedback(Some("open"))
            .map_err(|e| ErrorData::internal_error(e.to_string(), None))?
            .len();

        let recent_lines: Vec<String> = recent
            .iter()
            .map(|m| format!("  \u{2022} {} \u{2014} {} [{}]", m.id, m.title, m.layer))
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
        if open_conflicts > 0 {
            parts.push(format!("\n\u{26a0}  {open_conflicts} sync conflict(s) need review \u{2014} check the dashboard or use /memory-merge"));
        }
        if open_feedback > 0 {
            parts.push(format!(
                "\u{26a0}  {open_feedback} open feedback item(s) \u{2014} use /review-feedback"
            ));
        }
        parts.push("\nTip: Use /memory-list to browse all memories, or /memory-search <query> to find specific ones.".to_string());

        Ok(vec![PromptMessage::new_text(
            PromptMessageRole::User,
            parts.join("\n"),
        )])
    }

    async fn do_memory_search_prompt(
        &self,
        p: MemorySearchPromptInput,
    ) -> Result<Vec<PromptMessage>, ErrorData> {
        let hits = self
            .store
            .search(&p.query, 10)
            .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;
        let body = if hits.is_empty() {
            format!(
                "No memories found matching \"{}\".\n\nTip: Try broader keywords or use /memory-list to browse all memories.",
                p.query
            )
        } else {
            let lines: Vec<String> = hits
                .iter()
                .map(|h| format!("\u{2022} {} \u{2014} {}\n  {}", h.id, h.title, h.snippet))
                .collect();
            format!(
                "Search results for \"{}\" ({} found):\n\n{}\n\nUse memory_recall with an ID for full content.",
                p.query,
                hits.len(),
                lines.join("\n\n")
            )
        };
        Ok(vec![PromptMessage::new_text(PromptMessageRole::User, body)])
    }

    async fn do_memory_edit_prompt(
        &self,
        p: MemoryIdInput,
    ) -> Result<Vec<PromptMessage>, ErrorData> {
        let mem = self
            .store
            .recall_by_id(&p.id)
            .map_err(|e| ErrorData::internal_error(e.to_string(), None))?
            .ok_or_else(|| ErrorData::invalid_params(format!("Memory {} not found", p.id), None))?;
        let tags = mem.tags.join(", ");
        let body = format!(
            "Memory to edit:\n\
             ━━━━━━━━━━━━━━\n\
             ID:      {}\n\
             Title:   {}\n\
             Layer:   {}\n\
             Tags:    {}\n\
             Content:\n{}\n\
             ━━━━━━━━━━━━━━\n\n\
             Ask the user what changes they want to make, then call memory_update with ID {} to save.\n\
             You can update title, content, and/or tags. Omit fields you are not changing.",
            mem.id, mem.title, mem.layer, tags, mem.content, mem.id
        );
        Ok(vec![PromptMessage::new_text(PromptMessageRole::User, body)])
    }

    async fn do_memory_flag_prompt(
        &self,
        p: MemoryFlagInput,
    ) -> Result<Vec<PromptMessage>, ErrorData> {
        let mem = self
            .store
            .recall_by_id(&p.id)
            .map_err(|e| ErrorData::internal_error(e.to_string(), None))?
            .ok_or_else(|| ErrorData::invalid_params(format!("Memory {} not found", p.id), None))?;
        self.store
            .create_feedback(Some(&p.id), None, &p.reason, p.note.as_deref())
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
        Ok(vec![PromptMessage::new_text(PromptMessageRole::User, body)])
    }

    async fn do_memory_merge_prompt(
        &self,
        p: ConflictIdInput,
    ) -> Result<Vec<PromptMessage>, ErrorData> {
        let conflict = self
            .store
            .get_conflict_by_id(&p.id)
            .map_err(|e| ErrorData::internal_error(e.to_string(), None))?
            .ok_or_else(|| {
                ErrorData::invalid_params(format!("Conflict {} not found", p.id), None)
            })?;
        if conflict.status != "open" {
            return Err(ErrorData::invalid_params(
                format!(
                    "Conflict {} is already {} — nothing to merge",
                    p.id, conflict.status
                ),
                None,
            ));
        }
        let title_line = conflict
            .title
            .as_deref()
            .map(|t| format!("Memory: {t}\n"))
            .unwrap_or_default();
        let mem_line = conflict
            .memory_id
            .as_deref()
            .map(|mid| format!("Memory ID: {mid}\n"))
            .unwrap_or_default();
        let body = format!(
            "Sync Conflict — {id}\n\
             {title_line}{mem_line}\
             ━━━━━━━━━━━━━━ WINNER ({winner_src}) ━━━━━━━━━━━━━━\n\
             {winner}\n\n\
             ━━━━━━━━━━━━━━ LOSER ({loser_src}) ━━━━━━━━━━━━━━\n\
             {loser}\n\
             ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n\n\
             The winner was applied automatically (newer timestamp). The loser was preserved here.\n\n\
             Options:\n\
             1. Keep winner — conflict stays resolved, no action needed\n\
             2. Restore loser — call: POST /api/v1/conflicts/{id}/resolve with {{\"action\":\"restore\"}}\n\
             3. Merge — review both, craft a merged version, call memory_update with the memory ID\n\
                then resolve the conflict via the dashboard or API\n\n\
             If merging: call memory_update({mem_id}, {{ \"content\": \"<merged content>\" }}) then\n\
             POST /api/v1/conflicts/{id}/resolve with {{\"action\":\"keep\"}}",
            id = p.id,
            winner_src = conflict.winner_src,
            winner = conflict.winner,
            loser_src = conflict.loser_src,
            loser = conflict.loser,
            mem_id = conflict
                .memory_id
                .as_deref()
                .unwrap_or("(no linked memory)"),
        );
        Ok(vec![PromptMessage::new_text(PromptMessageRole::User, body)])
    }

    async fn do_suggest_connections_prompt(&self) -> Result<Vec<PromptMessage>, ErrorData> {
        let memories = self
            .store
            .list_memories(None, 100)
            .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;
        let edges = self
            .store
            .list_edges(None)
            .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;

        if memories.is_empty() {
            return Ok(vec![PromptMessage::new_text(
                PromptMessageRole::User,
                "No memories stored yet. Add some memories first with memory_store.".to_string(),
            )]);
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
                format!(
                    "{} | {} | {}{} | {}{}",
                    m.id, m.layer, m.title, tags, snippet, ellipsis
                )
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

        let body = format!(
            "HiveMind — Suggest Connections\n\
             ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n\
             You have {} memories and {} existing connections.\n\n\
             MEMORIES:\n\
             {}\n\n\
             EXISTING CONNECTIONS:\n\
             {}\n\n\
             ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n\
             Analyze the memories above and identify meaningful connections not yet captured.\n\
             For each suggested connection, call memory_store_edge with:\n\
               - source_id: the source memory ID\n\
               - target_id: the target memory ID\n\
               - relationship: one of: shares_tag | applies_to | pairs_with | used_in | related_to | custom\n\
             New edges are created with status='pending' and will appear in the dashboard for review.\n\
             Suggest 3–7 connections. Skip obvious ones (same tag already linked). Focus on cross-domain insights.",
            memories.len(),
            edges.len(),
            mem_lines.join("\n"),
            edge_section,
        );
        Ok(vec![PromptMessage::new_text(PromptMessageRole::User, body)])
    }

    async fn do_review_feedback_prompt(&self) -> Result<Vec<PromptMessage>, ErrorData> {
        let items = self
            .store
            .list_feedback(Some("open"))
            .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;

        if items.is_empty() {
            return Ok(vec![PromptMessage::new_text(
                PromptMessageRole::User,
                "No open feedback items. All flagged memories have been reviewed.\n\
                 Tip: Use /memory-flag <id> to flag a memory that needs attention."
                    .to_string(),
            )]);
        }

        let mut lines = vec![
            format!("Open Feedback Items ({} total)", items.len()),
            "\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}".to_string(),
        ];

        for (i, item) in items.iter().enumerate() {
            let target = match (&item.memory_id, &item.edge_id) {
                (Some(mid), _) => {
                    if let Ok(Some(mem)) = self.store.recall_by_id(mid) {
                        format!("Memory: {} \u{2014} \"{}\"", mid, mem.title)
                    } else {
                        format!("Memory: {mid}")
                    }
                }
                (_, Some(eid)) => format!("Edge: {eid}"),
                _ => "(no target)".to_string(),
            };
            let note = item.note.as_deref().unwrap_or("(no note)");
            lines.push(format!(
                "\n{}. [{}] {} | {}\n   Note: {}",
                i + 1,
                item.kind,
                target,
                item.id,
                note
            ));
        }

        lines.push("\n\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}\u{2501}".to_string());
        lines.push("For each item, choose an action:".to_string());
        lines.push("  \u{2022} Dismiss (no action needed) \u{2014} POST /api/v1/feedback/{id} {\"status\":\"dismissed\"}".to_string());
        lines.push(
            "  \u{2022} Fix the memory \u{2014} call memory_update with the memory ID".to_string(),
        );
        lines.push(
            "  \u{2022} Delete if truly wrong \u{2014} call memory_delete with confirm:true"
                .to_string(),
        );
        lines.push("\nAsk the user how to handle each item before taking action.".to_string());

        Ok(vec![PromptMessage::new_text(
            PromptMessageRole::User,
            lines.join("\n"),
        )])
    }

    pub async fn do_session_start(
        &self,
        p: SessionStartInput,
    ) -> Result<CallToolResult, ErrorData> {
        // Validate the path: must exist and be a directory. canonicalize resolves
        // `..`/symlinks so traversal can't escape into a non-directory.
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
            .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;

        let context_loaded: Vec<_> = result
            .loaded
            .iter()
            .map(|l| {
                json!({
                    "id": l.entry.id,
                    "title": l.entry.title,
                    "content": l.entry.content,
                    "layer": l.entry.layer.to_string(),
                    "tags": l.entry.tags,
                })
            })
            .collect();

        let skipped: Vec<_> = result
            .skipped
            .iter()
            .map(|s| {
                json!({
                    "query": s.query,
                    "reason": s.reason.as_str(),
                })
            })
            .collect();

        Ok(CallToolResult::structured(json!({
            "project": result.project,
            "context_loaded": context_loaded,
            "budget": {
                "used_tokens": result.used_tokens,
                "max_tokens": result.max_tokens,
                "remaining": result.remaining(),
                "truncated": result.truncated(),
            },
            "skipped": skipped,
            "hint": "Session context loaded. Incorporate it silently and proceed with the user's request.",
        })))
    }
}

#[tool_router]
impl HiveMind {
    #[tool(
        description = "Store a memory, preference, project context, or personal note for future recall across sessions and devices. Use when the user explicitly asks to remember something, or when important context should persist beyond this session."
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
        description = "Search stored memories by keyword (FTS). Returns ranked snippets (not full content) to conserve context — use memory_recall with an id for full content. Default 5 results, max 10."
    )]
    async fn memory_search(
        &self,
        Parameters(p): Parameters<MemorySearchInput>,
    ) -> Result<CallToolResult, ErrorData> {
        self.do_memory_search(p).await
    }

    #[tool(
        description = "Update an existing memory's title, content, or tags by id. Set merge_content=true to append to existing content rather than replace it. Providing tags replaces all tags on the memory."
    )]
    async fn memory_update(
        &self,
        Parameters(p): Parameters<MemoryUpdateInput>,
    ) -> Result<CallToolResult, ErrorData> {
        self.do_memory_update(p).await
    }

    #[tool(
        description = "Permanently delete a memory by id. Requires confirm=true; always confirm with the user before calling. Removes the memory, its tags, and its connections."
    )]
    async fn memory_delete(
        &self,
        Parameters(p): Parameters<MemoryDeleteInput>,
    ) -> Result<CallToolResult, ErrorData> {
        self.do_memory_delete(p).await
    }

    #[tool(
        description = "Call this once at the start of every session when .hivemind.toml exists in the project root. Returns pre-configured memory context (within a token budget) for this project. Do not call more than once per session."
    )]
    async fn hivemind_session_start(
        &self,
        Parameters(p): Parameters<SessionStartInput>,
    ) -> Result<CallToolResult, ErrorData> {
        self.do_session_start(p).await
    }
}

#[prompt_router]
impl HiveMind {
    /// List all memories with titles, tags, and dates
    #[prompt(
        name = "memory-list",
        description = "List all stored memories with titles, tags, and timestamps. Use to browse what HiveMind knows before searching or editing."
    )]
    async fn memory_list_prompt(&self) -> Result<Vec<PromptMessage>, ErrorData> {
        self.do_memory_list_prompt().await
    }

    /// Show the current memory count, recent activity, and session context
    #[prompt(
        name = "memory-status",
        description = "Show total memory count, recent memories, and sync status. Use at the start of a session to understand what context is available."
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

    /// Fetch a memory and return a prompt for editing its content, title, or tags
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
        description = "Flag a memory for review. Creates a feedback record with the specified reason. Use when you notice a memory contains wrong or stale information."
    )]
    async fn memory_flag_prompt(
        &self,
        Parameters(p): Parameters<MemoryFlagInput>,
    ) -> Result<Vec<PromptMessage>, ErrorData> {
        self.do_memory_flag_prompt(p).await
    }

    /// Present a sync conflict side-by-side for intelligent merging
    #[prompt(
        name = "memory-merge",
        description = "Fetch a sync conflict by ID and present winner vs loser side by side. Review both versions, then call memory_update with a merged result. Finally, resolve the conflict via the dashboard or API."
    )]
    async fn memory_merge_prompt(
        &self,
        Parameters(p): Parameters<ConflictIdInput>,
    ) -> Result<Vec<PromptMessage>, ErrorData> {
        self.do_memory_merge_prompt(p).await
    }

    /// Analyze the memory graph and suggest new connections between related memories
    #[prompt(
        name = "suggest-connections",
        description = "Fetch all memories and existing connections, then analyze them to suggest new edges. For each suggestion, call memory_store_edge to create a pending connection that appears in the dashboard for review."
    )]
    async fn suggest_connections_prompt(&self) -> Result<Vec<PromptMessage>, ErrorData> {
        self.do_suggest_connections_prompt().await
    }

    /// Surface all open feedback items for interactive review and resolution
    #[prompt(
        name = "review-feedback",
        description = "Fetch open feedback items (flagged memories, disputed edges) and present them for interactive resolution. For each item, you can dismiss, resolve, or take corrective action by calling memory_update or memory_delete."
    )]
    async fn review_feedback_prompt(&self) -> Result<Vec<PromptMessage>, ErrorData> {
        self.do_review_feedback_prompt().await
    }
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
    use crate::{db, store::SqliteStore};

    fn test_hivemind() -> HiveMind {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys=ON;").unwrap();
        db::create_schema(&conn).unwrap();
        HiveMind::new(SqliteStore::new(conn))
    }

    #[test]
    fn get_info_advertises_name_and_tools_capability() {
        use rmcp::ServerHandler;
        let info = test_hivemind().get_info();
        assert_eq!(info.server_info.name, "hivemind");
        assert!(
            info.capabilities.tools.is_some(),
            "tools capability must be advertised"
        );
    }

    #[test]
    fn get_info_advertises_prompts_capability() {
        use rmcp::ServerHandler;
        let info = test_hivemind().get_info();
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
        let hm = test_hivemind();
        let result = hm
            .do_memory_store(MemoryStoreInput {
                title: "my preference".to_string(),
                content: "prefer tabs over spaces".to_string(),
                layer: "personal".to_string(),
                tags: vec!["style".to_string()],
                project: None,
            })
            .await
            .unwrap();
        let val = result.structured_content.unwrap();
        assert!(val["id"].as_str().unwrap().starts_with("mem_"));
        assert_eq!(val["auto_connected"], 0);
    }

    #[tokio::test]
    async fn memory_recall_by_id_returns_content() {
        let hm = test_hivemind();
        let stored = hm
            .do_memory_store(MemoryStoreInput {
                title: "rust style".to_string(),
                content: "use clippy, rustfmt, and deny warnings".to_string(),
                layer: "personal".to_string(),
                tags: vec!["rust".to_string()],
                project: None,
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
        let hm = test_hivemind();
        hm.do_memory_store(MemoryStoreInput {
            title: "clean arch".to_string(),
            content: "domain at center, infra at edge".to_string(),
            layer: "personal".to_string(),
            tags: vec!["architecture".to_string()],
            project: None,
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
        let hm = test_hivemind();
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
        let hm = test_hivemind();
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
        let hm = test_hivemind();
        hm.do_memory_store(MemoryStoreInput {
            title: "db driver choice".to_string(),
            content: "we standardized on pgx v5 for postgres".to_string(),
            layer: "personal".to_string(),
            tags: vec!["golang".to_string(), "database".to_string()],
            project: None,
        })
        .await
        .unwrap();

        let result = hm
            .do_memory_search(MemorySearchInput {
                query: "pgx".to_string(),
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
        let hm = test_hivemind();
        let result = hm
            .do_memory_search(MemorySearchInput {
                query: "  ".to_string(),
                limit: None,
            })
            .await
            .unwrap();
        assert_eq!(result.structured_content.unwrap()["count"], 0);
    }

    #[tokio::test]
    async fn memory_update_changes_content() {
        let hm = test_hivemind();
        let stored = hm
            .do_memory_store(MemoryStoreInput {
                title: "deploy notes".to_string(),
                content: "uses docker swarm".to_string(),
                layer: "personal".to_string(),
                tags: vec!["devops".to_string()],
                project: None,
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
                merge_content: None,
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
        let hm = test_hivemind();
        let result = hm
            .do_memory_update(MemoryUpdateInput {
                id: "mem_nope".to_string(),
                title: Some("x".to_string()),
                content: None,
                tags: None,
                merge_content: None,
            })
            .await
            .unwrap();
        assert_eq!(result.structured_content.unwrap()["updated"], false);
    }

    #[tokio::test]
    async fn session_start_loads_configured_recalls() {
        let hm = test_hivemind();
        hm.do_memory_store(MemoryStoreInput {
            title: "golang preferences".to_string(),
            content: "use uber/zap, sqlc, pgx v5".to_string(),
            layer: "personal".to_string(),
            tags: vec!["golang".to_string()],
            project: None,
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
    async fn session_start_rejects_nonexistent_path() {
        let hm = test_hivemind();
        let err = hm
            .do_session_start(SessionStartInput {
                project_path: "/no/such/dir/anywhere".to_string(),
            })
            .await;
        assert!(err.is_err());
    }

    #[tokio::test]
    async fn session_start_errors_without_config() {
        let hm = test_hivemind();
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
            rmcp::model::PromptMessageContent::Text { text } => text.as_str(),
            _ => panic!("expected text content"),
        }
    }

    #[tokio::test]
    async fn memory_list_prompt_returns_no_memories_message() {
        let hm = test_hivemind();
        let result = hm.do_memory_list_prompt().await.unwrap();
        assert_eq!(result.len(), 1);
        assert!(prompt_text(&result[0]).contains("No memories"));
    }

    #[tokio::test]
    async fn memory_status_prompt_includes_count() {
        let hm = test_hivemind();
        hm.do_memory_store(MemoryStoreInput {
            title: "test".to_string(),
            content: "c".to_string(),
            layer: "personal".to_string(),
            tags: vec![],
            project: None,
        })
        .await
        .unwrap();
        let result = hm.do_memory_status_prompt().await.unwrap();
        let text = prompt_text(&result[0]);
        assert!(text.contains("1"), "status should show count of 1");
    }

    #[tokio::test]
    async fn memory_search_prompt_returns_results() {
        let hm = test_hivemind();
        hm.do_memory_store(MemoryStoreInput {
            title: "golang preferences".to_string(),
            content: "use uber/zap and chi router".to_string(),
            layer: "personal".to_string(),
            tags: vec!["golang".to_string()],
            project: None,
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
        let hm = test_hivemind();
        let stored = hm
            .do_memory_store(MemoryStoreInput {
                title: "rust style".to_string(),
                content: "use clippy and rustfmt".to_string(),
                layer: "personal".to_string(),
                tags: vec!["rust".to_string()],
                project: None,
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
        let hm = test_hivemind();
        let result = hm
            .do_memory_edit_prompt(MemoryIdInput {
                id: "mem_nonexistent".to_string(),
            })
            .await;
        assert!(result.is_err(), "should error when memory not found");
    }

    #[tokio::test]
    async fn memory_flag_prompt_creates_feedback_record() {
        let hm = test_hivemind();
        let stored = hm
            .do_memory_store(MemoryStoreInput {
                title: "test".to_string(),
                content: "c".to_string(),
                layer: "personal".to_string(),
                tags: vec![],
                project: None,
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

        let feedback = hm.store.list_feedback(Some("open")).unwrap();
        assert_eq!(feedback.len(), 1, "feedback record should be created");
    }

    #[tokio::test]
    async fn memory_merge_prompt_shows_winner_and_loser() {
        let hm = test_hivemind();
        let cfl_id = hm
            .store
            .write_conflict(
                None,
                "Remote wins: new content",
                "Local old content",
                "remote",
                "local",
            )
            .unwrap();
        let result = hm
            .do_memory_merge_prompt(ConflictIdInput { id: cfl_id.clone() })
            .await
            .unwrap();
        let text = prompt_text(&result[0]);
        assert!(
            text.contains("Remote wins: new content"),
            "should show winner content"
        );
        assert!(
            text.contains("Local old content"),
            "should show loser content"
        );
        assert!(text.contains("remote"), "should show winner source");
    }

    #[tokio::test]
    async fn memory_merge_prompt_errors_for_missing_conflict() {
        let hm = test_hivemind();
        let result = hm
            .do_memory_merge_prompt(ConflictIdInput {
                id: "cfl_nonexistent".to_string(),
            })
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn suggest_connections_prompt_lists_memories_and_edges() {
        let hm = test_hivemind();
        hm.do_memory_store(MemoryStoreInput {
            title: "golang preferences".to_string(),
            content: "use uber/zap and chi router".to_string(),
            layer: "personal".to_string(),
            tags: vec!["golang".to_string()],
            project: None,
        })
        .await
        .unwrap();
        hm.do_memory_store(MemoryStoreInput {
            title: "observability stack".to_string(),
            content: "prometheus, grafana, loki".to_string(),
            layer: "personal".to_string(),
            tags: vec!["observability".to_string()],
            project: None,
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
            "should instruct Claude to call memory_store_edge"
        );
    }

    #[tokio::test]
    async fn review_feedback_prompt_shows_open_items() {
        let hm = test_hivemind();
        let stored = hm
            .do_memory_store(MemoryStoreInput {
                title: "old pref".to_string(),
                content: "stale content".to_string(),
                layer: "personal".to_string(),
                tags: vec![],
                project: None,
            })
            .await
            .unwrap();
        let mem_id = stored.structured_content.unwrap()["id"]
            .as_str()
            .unwrap()
            .to_string();
        hm.store
            .create_feedback(Some(&mem_id), None, "outdated", Some("This is outdated"))
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
        let hm = test_hivemind();
        let result = hm.do_review_feedback_prompt().await.unwrap();
        let text = prompt_text(&result[0]);
        assert!(
            text.to_lowercase().contains("no open"),
            "should indicate no items"
        );
    }

    #[test]
    fn all_eight_prompts_are_registered() {
        let _hm = test_hivemind();
        let prompts = HiveMind::prompt_router().list_all();
        let names: Vec<&str> = prompts.iter().map(|p| p.name.as_str()).collect();
        let expected = [
            "memory-list",
            "memory-status",
            "memory-search",
            "memory-edit",
            "memory-flag",
            "memory-merge",
            "suggest-connections",
            "review-feedback",
        ];
        for name in &expected {
            assert!(
                names.contains(name),
                "prompt {name} must be registered; got: {names:?}"
            );
        }
        assert_eq!(prompts.len(), 8, "exactly 8 prompts expected");
    }

    #[tokio::test]
    async fn memory_delete_requires_confirm() {
        let hm = test_hivemind();
        let stored = hm
            .do_memory_store(MemoryStoreInput {
                title: "temp".to_string(),
                content: "delete me".to_string(),
                layer: "personal".to_string(),
                tags: vec!["tmp".to_string()],
                project: None,
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
}
