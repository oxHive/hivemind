use super::*;

// --- memories ---

#[derive(Deserialize)]
pub(super) struct ListMemoriesParams {
    limit: Option<i64>,
    offset: Option<i64>,
}

pub(super) async fn list_memories(
    State(store): State<Store>,
    Query(p): Query<ListMemoriesParams>,
) -> Result<Json<Value>, ApiError> {
    let limit = p.limit.unwrap_or(200).clamp(1, 1000);
    let offset = p.offset.unwrap_or(0).max(0);
    let entries = store.list_memories(limit, offset).await?;
    Ok(Json(json!({
        "count": entries.len(),
        "memories": entries.iter().map(entry_json).collect::<Vec<_>>(),
    })))
}

#[derive(Deserialize)]
pub(super) struct CreateMemoryBody {
    title: String,
    content: String,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default)]
    token_count: Option<i64>,
    #[serde(default)]
    layer: Option<String>,
    #[serde(default)]
    memory_type: Option<String>,
}

pub(super) async fn create_memory(
    State(store): State<Store>,
    Extension(events): Extension<Events>,
    Json(b): Json<CreateMemoryBody>,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    let layer = match &b.layer {
        Some(l) => l
            .parse::<crate::model::Layer>()
            .map_err(|e| ApiError(StatusCode::UNPROCESSABLE_ENTITY, e.to_string()))?
            .to_string(),
        None => "workspace".to_string(),
    };
    let memory_type = match &b.memory_type {
        Some(t) => t
            .parse::<crate::model::MemoryType>()
            .map_err(|e| ApiError(StatusCode::UNPROCESSABLE_ENTITY, e.to_string()))?
            .to_string(),
        None => "project".to_string(),
    };
    let id = format!("mem_{}", uuid::Uuid::new_v4().simple());
    store
        .store(&crate::store::NewMemoryRow {
            id: &id,
            title: &b.title,
            content: &b.content,
            tags: &b.tags,
            token_count: b.token_count,
            layer: &layer,
            memory_type: &memory_type,
        })
        .await?;
    let _ = events.send(json!({ "type": "changed" }));
    Ok((StatusCode::CREATED, Json(json!({ "id": id }))))
}

pub(super) async fn get_memory(
    State(store): State<Store>,
    Path(id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    match store.recall_by_id(&id).await? {
        None => Err(not_found(format!("no memory {id}"))),
        Some(e) => Ok(Json(entry_json(&e))),
    }
}

#[derive(Deserialize)]
pub(super) struct PatchMemoryBody {
    title: Option<String>,
    content: Option<String>,
    tags: Option<Vec<String>>,
}

pub(super) async fn patch_memory(
    State(store): State<Store>,
    Extension(events): Extension<Events>,
    Path(id): Path<String>,
    Json(b): Json<PatchMemoryBody>,
) -> Result<Json<Value>, ApiError> {
    // Fetch current state to fill in unchanged fields
    let current = store
        .recall_by_id(&id)
        .await?
        .ok_or_else(|| not_found(format!("no memory {id}")))?;
    let title = b.title.as_deref().unwrap_or(&current.title);
    let content = b.content.as_deref().unwrap_or(&current.content);
    let tags = b.tags.as_deref().unwrap_or(&current.tags);
    let updated = store.update(&id, title, content, tags).await?;
    if !updated {
        return Err(not_found(format!("no memory {id}")));
    }
    let entry = store
        .recall_by_id(&id)
        .await?
        .ok_or_else(|| not_found(format!("no memory {id}")))?;
    let _ = events.send(json!({ "type": "changed" }));
    Ok(Json(entry_json(&entry)))
}

pub(super) async fn delete_memory(
    State(store): State<Store>,
    Extension(events): Extension<Events>,
    Path(id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    if !store.delete(&id).await? {
        return Err(not_found(format!("no memory {id}")));
    }
    let _ = events.send(json!({ "type": "changed" }));
    Ok(Json(json!({ "deleted": true, "id": id })))
}

pub(super) async fn delete_all_memories(
    State(store): State<Store>,
    Extension(events): Extension<Events>,
) -> Result<Json<Value>, ApiError> {
    let deleted = store.delete_all().await?;
    let _ = events.send(json!({ "type": "changed" }));
    Ok(Json(json!({ "deleted": deleted })))
}

// --- search ---

#[derive(Deserialize)]
pub(super) struct SearchParams {
    q: String,
    limit: Option<i64>,
}

pub(super) async fn search(
    State(store): State<Store>,
    Query(p): Query<SearchParams>,
) -> Result<Json<Value>, ApiError> {
    let limit = p.limit.unwrap_or(20).clamp(1, 50);
    let hits = store.search(&p.q, limit).await?;
    let results: Vec<_> = hits.iter().map(entry_json).collect();
    Ok(Json(json!({ "count": results.len(), "results": results })))
}
