use super::*;

// --- edges ---

#[derive(Deserialize)]
pub(super) struct EdgesParams {
    memory_id: Option<String>,
}

pub(super) async fn list_edges(
    State(store): State<Store>,
    Query(p): Query<EdgesParams>,
) -> Result<Json<Value>, ApiError> {
    let edges = store.list_edges(p.memory_id.as_deref()).await?;
    Ok(Json(json!({ "count": edges.len(), "edges": edges })))
}

#[derive(Deserialize)]
pub(super) struct CreateEdgeBody {
    source_id: String,
    target_id: String,
    relationship: String,
}

pub(super) async fn create_edge(
    State(store): State<Store>,
    Extension(events): Extension<Events>,
    Json(b): Json<CreateEdgeBody>,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    use crate::model::EdgeCreate;
    match store
        .create_edge(&b.source_id, &b.target_id, &b.relationship)
        .await?
    {
        EdgeCreate::Created(id) => {
            let _ = events.send(json!({ "type": "changed" }));
            Ok((StatusCode::CREATED, Json(json!({ "id": id }))))
        }
        EdgeCreate::Duplicate => Err(ApiError(StatusCode::CONFLICT, "edge already exists".into())),
        EdgeCreate::MissingEndpoint => Err(ApiError(
            StatusCode::UNPROCESSABLE_ENTITY,
            "source_id and target_id must be existing, distinct memory IDs".into(),
        )),
        EdgeCreate::InvalidRelationship => Err(ApiError(
            StatusCode::UNPROCESSABLE_ENTITY,
            format!(
                "invalid relationship; valid: {}",
                crate::store::VALID_RELATIONSHIPS.join(", ")
            ),
        )),
    }
}

#[derive(Deserialize)]
pub(super) struct StatusBody {
    pub(super) status: String,
}

pub(super) async fn patch_edge(
    State(store): State<Store>,
    Extension(events): Extension<Events>,
    Path(id): Path<String>,
    Json(b): Json<StatusBody>,
) -> Result<Json<Value>, ApiError> {
    if !["active", "pending", "rejected"].contains(&b.status.as_str()) {
        return Err(ApiError(
            StatusCode::UNPROCESSABLE_ENTITY,
            "status must be active|pending|rejected".into(),
        ));
    }
    if !store.set_edge_status(&id, &b.status).await? {
        return Err(not_found(format!("no edge {id}")));
    }
    let _ = events.send(json!({ "type": "changed" }));
    Ok(Json(json!({ "id": id, "status": b.status })))
}
