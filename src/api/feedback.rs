use super::*;

// --- feedback ---

#[derive(Deserialize)]
pub(super) struct FeedbackParams {
    memory_id: Option<String>,
    status: Option<String>,
}

pub(super) async fn list_feedback(
    State(store): State<Store>,
    Query(p): Query<FeedbackParams>,
) -> Result<Json<Value>, ApiError> {
    let items = store
        .list_feedback(p.memory_id.as_deref(), p.status.as_deref())
        .await?;
    Ok(Json(json!({ "count": items.len(), "items": items })))
}

#[derive(Deserialize)]
pub(super) struct CreateFeedbackBody {
    memory_id: String,
    signal: String,
    #[serde(default)]
    note: Option<String>,
}

pub(super) async fn create_feedback(
    State(store): State<Store>,
    Json(b): Json<CreateFeedbackBody>,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    let entry = store
        .create_feedback(&b.memory_id, &b.signal, b.note.as_deref())
        .await?;
    Ok((StatusCode::CREATED, Json(json!({ "id": entry.id }))))
}

pub(super) async fn patch_feedback(
    State(store): State<Store>,
    Path(id): Path<String>,
    Json(b): Json<StatusBody>,
) -> Result<Json<Value>, ApiError> {
    if !["pending", "resolved", "dismissed"].contains(&b.status.as_str()) {
        return Err(ApiError(
            StatusCode::UNPROCESSABLE_ENTITY,
            "status must be pending|resolved|dismissed".into(),
        ));
    }
    if !store.set_feedback_status(&id, &b.status).await? {
        return Err(not_found(format!("no feedback {id}")));
    }
    Ok(Json(json!({ "id": id, "status": b.status })))
}

// --- conflicts + status ---

#[derive(Deserialize)]
pub(super) struct ConflictsParams {
    status: Option<String>,
}

pub(super) async fn list_conflicts(
    State(store): State<Store>,
    Query(p): Query<ConflictsParams>,
) -> Result<Json<Value>, ApiError> {
    let items = store.list_conflicts(p.status.as_deref()).await?;
    Ok(Json(json!({ "count": items.len(), "conflicts": items })))
}

// --- conflict resolution ---

#[derive(Deserialize)]
pub(super) struct ResolveBody {
    #[serde(alias = "action")]
    resolution: String,
}

pub(super) async fn resolve_conflict_handler(
    State(store): State<Store>,
    Path(id): Path<String>,
    Json(b): Json<ResolveBody>,
) -> Result<Json<Value>, ApiError> {
    if !["keep_local", "keep_remote"].contains(&b.resolution.as_str()) {
        return Err(ApiError(
            StatusCode::UNPROCESSABLE_ENTITY,
            "resolution must be keep_local|keep_remote".into(),
        ));
    }
    if !store.resolve_conflict(&id, &b.resolution).await? {
        return Err(not_found(format!(
            "conflict {id} not found or already resolved"
        )));
    }
    Ok(Json(
        json!({ "resolved": true, "id": id, "resolution": b.resolution }),
    ))
}
