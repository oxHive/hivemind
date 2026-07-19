use super::*;

// --- session logs ---

#[derive(Deserialize)]
pub(super) struct SessionLogsParams {
    limit: Option<i64>,
}

pub(super) async fn list_session_logs(
    State(store): State<Store>,
    Query(p): Query<SessionLogsParams>,
) -> Result<Json<Value>, ApiError> {
    let limit = p.limit.unwrap_or(50).clamp(1, 200);
    let logs = store.list_session_logs(limit).await?;
    Ok(Json(json!({ "count": logs.len(), "logs": logs })))
}

pub(super) async fn server_status(
    State(store): State<Store>,
    Extension(sync): Extension<SyncSettings>,
    Extension(agent): Extension<crate::config::AgentSettings>,
) -> Result<Json<Value>, ApiError> {
    let count = store.count().await?;
    let last_synced_at = store
        .get_meta("last_synced_at")
        .await?
        .and_then(|v| v.parse::<i64>().ok());
    let conflict_count = store.pending_conflict_count().await?;
    Ok(Json(json!({
        "version": env!("CARGO_PKG_VERSION"),
        "memory_count": count,
        "db_path": crate::db::resolve_db_path(),
        "sync": {
            "enabled": sync.enabled,
            "last_synced_at": last_synced_at,
            "conflict_count": conflict_count,
        },
        "agent": {
            "kind": agent.kind.as_str(),
            "command": agent.command,
        },
    })))
}
