use super::*;

// --- export / import ---

pub(super) async fn export(State(store): State<Store>) -> Result<Json<Value>, ApiError> {
    let memories = store.list_memories(100_000, 0).await?;
    let edges = store.list_edges(None).await?;
    Ok(Json(json!({
        "version": env!("CARGO_PKG_VERSION"),
        "exported_at": std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs(),
        "memories": memories.iter().map(entry_json).collect::<Vec<_>>(),
        "edges": edges,
    })))
}

#[derive(Deserialize)]
pub(super) struct ImportBody {
    #[serde(default)]
    memories: Vec<ImportMemory>,
    #[serde(default)]
    edges: Vec<ImportEdge>,
}

#[derive(Deserialize)]
pub(super) struct ImportMemory {
    id: String,
    title: String,
    content: String,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default)]
    token_count: Option<i64>,
    #[serde(default = "default_layer")]
    layer: String,
    #[serde(default = "default_memory_type")]
    memory_type: String,
}

fn default_layer() -> String {
    "workspace".into()
}

fn default_memory_type() -> String {
    "project".into()
}

#[derive(Deserialize)]
pub(super) struct ImportEdge {
    source_id: String,
    target_id: String,
    relationship: String,
    #[serde(default = "default_edge_status")]
    status: String,
    #[serde(default)]
    link_text: Option<String>,
    #[serde(default)]
    reason: Option<String>,
}

fn default_edge_status() -> String {
    "active".into()
}

pub(super) async fn import(
    State(store): State<Store>,
    Json(b): Json<ImportBody>,
) -> Result<Json<Value>, ApiError> {
    let mut mem_count = 0usize;
    for m in &b.memories {
        store
            .store(&crate::store::NewMemoryRow {
                id: &m.id,
                title: &m.title,
                content: &m.content,
                tags: &m.tags,
                token_count: m.token_count,
                layer: &m.layer,
                memory_type: &m.memory_type,
            })
            .await?;
        mem_count += 1;
    }
    let mut edge_count = 0usize;
    for e in &b.edges {
        if !["active", "pending", "rejected"].contains(&e.status.as_str()) {
            continue;
        }
        if matches!(
            store
                .create_edge_with_status(
                    &e.source_id,
                    &e.target_id,
                    &e.relationship,
                    &e.status,
                    e.link_text.as_deref(),
                    e.reason.as_deref(),
                )
                .await?,
            crate::model::EdgeCreate::Created(_)
        ) {
            edge_count += 1;
        }
    }
    Ok(Json(
        json!({ "imported_memories": mem_count, "imported_edges": edge_count }),
    ))
}
