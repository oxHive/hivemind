use super::*;

// --- suggest sessions ---

pub(super) async fn start_suggest_session(
    Extension(mgr): Extension<Arc<SuggestSessionManager>>,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    match mgr.start().await {
        Ok(()) => Ok((StatusCode::ACCEPTED, Json(json!({ "started": true })))),
        Err(StartError::AlreadyActive) => Err(ApiError(
            StatusCode::CONFLICT,
            "a suggest session is already active".into(),
        )),
        Err(StartError::Failed(msg)) => Err(ApiError(StatusCode::INTERNAL_SERVER_ERROR, msg)),
    }
}

pub(super) async fn suggest_session_status(
    Extension(mgr): Extension<Arc<SuggestSessionManager>>,
) -> Json<Value> {
    Json(mgr.status().await)
}

#[derive(Deserialize)]
pub(super) struct ReviseBody {
    edge_id: String,
    feedback: String,
}

pub(super) async fn revise_suggest_session(
    Extension(mgr): Extension<Arc<SuggestSessionManager>>,
    Json(b): Json<ReviseBody>,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    match mgr.revise(b.edge_id.clone(), b.feedback).await {
        Ok(()) => Ok((
            StatusCode::ACCEPTED,
            Json(json!({ "queued": true, "edge_id": b.edge_id })),
        )),
        Err(ReviseError::NotActive) => Err(ApiError(
            StatusCode::CONFLICT,
            "no active suggest session".into(),
        )),
        Err(ReviseError::UnknownEdge) => Err(not_found(format!("no edge {}", b.edge_id))),
    }
}

pub(super) async fn end_suggest_session(
    Extension(mgr): Extension<Arc<SuggestSessionManager>>,
) -> Json<Value> {
    mgr.end().await;
    Json(json!({ "ended": true }))
}
