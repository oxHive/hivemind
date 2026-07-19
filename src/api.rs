use crate::{
    config::SyncSettings,
    store::SqliteStore,
    suggest_session::{ReviseError, StartError, SuggestSessionManager},
    update::{SharedUpdateState, UpdateStatus},
};
use axum::{
    Json, Router,
    extract::{Extension, Path, Query, State},
    http::{Method, StatusCode, header},
    response::{
        IntoResponse, Response,
        sse::{Event, KeepAlive, Sse},
    },
    routing::{get, post},
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::sync::Arc;
use tokio::sync::broadcast;
use tokio_stream::{Stream, StreamExt, wrappers::BroadcastStream};
use tower_http::cors::{AllowOrigin, CorsLayer};

type Store = Arc<SqliteStore>;
type Events = broadcast::Sender<serde_json::Value>;

pub struct ApiError(StatusCode, String);

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.0, Json(json!({ "error": self.1 }))).into_response()
    }
}

impl From<anyhow::Error> for ApiError {
    fn from(e: anyhow::Error) -> Self {
        ApiError(StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
    }
}

fn not_found(msg: impl Into<String>) -> ApiError {
    ApiError(StatusCode::NOT_FOUND, msg.into())
}

/// Returns an `AllowOrigin` that accepts both the configured dashboard origin and
/// its `localhost` / `127.0.0.1` counterpart, so the browser CORS check passes
/// regardless of which loopback hostname the user typed.
fn localhost_origins(origin: &str) -> AllowOrigin {
    let mut origins: Vec<axum::http::HeaderValue> = Vec::new();

    if let Ok(v) = origin.parse::<axum::http::HeaderValue>() {
        origins.push(v);
    }

    // Add the `localhost` ↔ `127.0.0.1` sibling so both hostnames are accepted.
    let sibling = if origin.contains("127.0.0.1") {
        origin.replace("127.0.0.1", "localhost")
    } else if origin.contains("localhost") {
        origin.replace("localhost", "127.0.0.1")
    } else {
        String::new()
    };
    if !sibling.is_empty()
        && let Ok(v) = sibling.parse::<axum::http::HeaderValue>()
    {
        origins.push(v);
    }

    if origins.is_empty() {
        AllowOrigin::exact(axum::http::HeaderValue::from_static(
            "http://127.0.0.1:3457",
        ))
    } else {
        AllowOrigin::list(origins)
    }
}

pub fn router(
    store: Store,
    sync: SyncSettings,
    dashboard_origin: &str,
    events: Events,
    suggest: Arc<SuggestSessionManager>,
    update_state: SharedUpdateState,
) -> Router {
    Router::new()
        .route("/api/v1/memories", get(list_memories).post(create_memory))
        .route(
            "/api/v1/memories/all",
            axum::routing::delete(delete_all_memories),
        )
        .route(
            "/api/v1/memories/{id}",
            get(get_memory).patch(patch_memory).delete(delete_memory),
        )
        .route("/api/v1/export", get(export))
        .route("/api/v1/import", post(import))
        .route("/api/v1/search", get(search))
        .route("/api/v1/edges", get(list_edges).post(create_edge))
        .route("/api/v1/edges/{id}", axum::routing::patch(patch_edge))
        .route("/api/v1/feedback", get(list_feedback).post(create_feedback))
        .route(
            "/api/v1/feedback/{id}",
            axum::routing::patch(patch_feedback),
        )
        .route("/api/v1/conflicts", get(list_conflicts))
        .route(
            "/api/v1/conflicts/{id}/resolve",
            post(resolve_conflict_handler),
        )
        .route(
            "/api/v1/settings/sync",
            get(get_sync_settings).post(save_sync_settings),
        )
        .route(
            "/api/v1/settings/tags",
            get(get_tag_settings).post(save_tag_settings),
        )
        .route("/api/v1/session-logs", get(list_session_logs))
        .route("/api/v1/status", get(server_status))
        .route("/api/v1/events", get(sse_events))
        .route("/api/v1/update", get(get_update_state))
        .route("/api/v1/update/apply", post(apply_update))
        .route("/api/v1/suggest-sessions", post(start_suggest_session))
        .route(
            "/api/v1/suggest-sessions/current",
            get(suggest_session_status).delete(end_suggest_session),
        )
        .route(
            "/api/v1/suggest-sessions/current/revise",
            post(revise_suggest_session),
        )
        .with_state(store)
        .layer(Extension(sync))
        .layer(Extension(events))
        .layer(Extension(suggest))
        .layer(Extension(update_state))
        .layer(
            CorsLayer::new()
                .allow_origin(localhost_origins(dashboard_origin))
                .allow_methods([
                    Method::GET,
                    Method::POST,
                    Method::PATCH,
                    Method::DELETE,
                    Method::OPTIONS,
                ])
                .allow_headers([header::CONTENT_TYPE, header::AUTHORIZATION]),
        )
}

async fn sse_events(
    Extension(events): Extension<Events>,
) -> Sse<impl Stream<Item = Result<Event, std::convert::Infallible>>> {
    let stream = BroadcastStream::new(events.subscribe())
        .filter_map(|msg| msg.ok().map(|v| Ok(Event::default().data(v.to_string()))));
    Sse::new(stream).keep_alive(KeepAlive::default())
}

fn entry_json(e: &crate::store::MemoryEntry) -> Value {
    json!({
        "id": e.id,
        "title": e.title,
        "content": e.content,
        "tags": e.tags,
        "created_at": e.created_at,
        "updated_at": e.updated_at,
        "token_count": e.token_count,
        "layer": e.layer,
        "memory_type": e.memory_type,
    })
}

// --- memories ---

#[derive(Deserialize)]
struct ListMemoriesParams {
    limit: Option<i64>,
    offset: Option<i64>,
}

async fn list_memories(
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
struct CreateMemoryBody {
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

async fn create_memory(
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

async fn get_memory(
    State(store): State<Store>,
    Path(id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    match store.recall_by_id(&id).await? {
        None => Err(not_found(format!("no memory {id}"))),
        Some(e) => Ok(Json(entry_json(&e))),
    }
}

#[derive(Deserialize)]
struct PatchMemoryBody {
    title: Option<String>,
    content: Option<String>,
    tags: Option<Vec<String>>,
}

async fn patch_memory(
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

async fn delete_memory(
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

async fn delete_all_memories(
    State(store): State<Store>,
    Extension(events): Extension<Events>,
) -> Result<Json<Value>, ApiError> {
    let deleted = store.delete_all().await?;
    let _ = events.send(json!({ "type": "changed" }));
    Ok(Json(json!({ "deleted": deleted })))
}

// --- export / import ---

async fn export(State(store): State<Store>) -> Result<Json<Value>, ApiError> {
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
struct ImportBody {
    #[serde(default)]
    memories: Vec<ImportMemory>,
    #[serde(default)]
    edges: Vec<ImportEdge>,
}

#[derive(Deserialize)]
struct ImportMemory {
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
struct ImportEdge {
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

async fn import(
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

// --- search ---

#[derive(Deserialize)]
struct SearchParams {
    q: String,
    limit: Option<i64>,
}

async fn search(
    State(store): State<Store>,
    Query(p): Query<SearchParams>,
) -> Result<Json<Value>, ApiError> {
    let limit = p.limit.unwrap_or(20).clamp(1, 50);
    let hits = store.search(&p.q, limit).await?;
    let results: Vec<_> = hits.iter().map(entry_json).collect();
    Ok(Json(json!({ "count": results.len(), "results": results })))
}

// --- edges ---

#[derive(Deserialize)]
struct EdgesParams {
    memory_id: Option<String>,
}

async fn list_edges(
    State(store): State<Store>,
    Query(p): Query<EdgesParams>,
) -> Result<Json<Value>, ApiError> {
    let edges = store.list_edges(p.memory_id.as_deref()).await?;
    Ok(Json(json!({ "count": edges.len(), "edges": edges })))
}

#[derive(Deserialize)]
struct CreateEdgeBody {
    source_id: String,
    target_id: String,
    relationship: String,
}

async fn create_edge(
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
struct StatusBody {
    status: String,
}

async fn patch_edge(
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

// --- session logs ---

#[derive(Deserialize)]
struct SessionLogsParams {
    limit: Option<i64>,
}

async fn list_session_logs(
    State(store): State<Store>,
    Query(p): Query<SessionLogsParams>,
) -> Result<Json<Value>, ApiError> {
    let limit = p.limit.unwrap_or(50).clamp(1, 200);
    let logs = store.list_session_logs(limit).await?;
    Ok(Json(json!({ "count": logs.len(), "logs": logs })))
}

// --- feedback ---

#[derive(Deserialize)]
struct FeedbackParams {
    memory_id: Option<String>,
    status: Option<String>,
}

async fn list_feedback(
    State(store): State<Store>,
    Query(p): Query<FeedbackParams>,
) -> Result<Json<Value>, ApiError> {
    let items = store
        .list_feedback(p.memory_id.as_deref(), p.status.as_deref())
        .await?;
    Ok(Json(json!({ "count": items.len(), "items": items })))
}

#[derive(Deserialize)]
struct CreateFeedbackBody {
    memory_id: String,
    signal: String,
    #[serde(default)]
    note: Option<String>,
}

async fn create_feedback(
    State(store): State<Store>,
    Json(b): Json<CreateFeedbackBody>,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    let entry = store
        .create_feedback(&b.memory_id, &b.signal, b.note.as_deref())
        .await?;
    Ok((StatusCode::CREATED, Json(json!({ "id": entry.id }))))
}

async fn patch_feedback(
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
struct ConflictsParams {
    status: Option<String>,
}

async fn list_conflicts(
    State(store): State<Store>,
    Query(p): Query<ConflictsParams>,
) -> Result<Json<Value>, ApiError> {
    let items = store.list_conflicts(p.status.as_deref()).await?;
    Ok(Json(json!({ "count": items.len(), "conflicts": items })))
}

async fn server_status(
    State(store): State<Store>,
    Extension(sync): Extension<SyncSettings>,
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
    })))
}

async fn get_update_state(
    Extension(update_state): Extension<SharedUpdateState>,
) -> Result<Json<Value>, ApiError> {
    let s = update_state.read().await;
    Ok(Json(
        serde_json::to_value(&*s).unwrap_or_else(|_| json!({})),
    ))
}

async fn apply_update(
    Extension(update_state): Extension<SharedUpdateState>,
    Extension(events): Extension<Events>,
) -> Result<Json<Value>, ApiError> {
    {
        let mut s = update_state.write().await;
        if s.status == UpdateStatus::Updating {
            return Err(ApiError(
                StatusCode::CONFLICT,
                "update already in progress".into(),
            ));
        }
        s.status = UpdateStatus::Updating;
        s.update_started_at = Some(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs() as i64,
        );
        s.error = None;
    }
    let _ = events.send(json!({
        "type": "update_progress",
        "status": "updating",
        "started_at": update_state.read().await.update_started_at,
    }));

    let state = update_state.clone();
    let events2 = events.clone();
    tokio::spawn(async move {
        crate::update::run_update(state, events2).await;
    });

    Ok(Json(json!({"status": "updating"})))
}

// --- conflict resolution ---

#[derive(Deserialize)]
struct ResolveBody {
    #[serde(alias = "action")]
    resolution: String,
}

async fn resolve_conflict_handler(
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

// --- sync settings (read-only from file in v1) ---

async fn get_sync_settings(Extension(sync): Extension<SyncSettings>) -> Json<Value> {
    Json(json!({
        "enabled": sync.enabled,
        "remote_url": sync.remote_url,
        "interval_seconds": sync.interval_seconds,
        "sync_on_store": sync.sync_on_store,
        "sync_on_startup": sync.sync_on_startup,
    }))
}

async fn save_sync_settings(Json(_): Json<Value>) -> Json<Value> {
    Json(
        json!({ "saved": false, "message": "Sync settings are managed via config.toml — restart hivemind after editing." }),
    )
}

fn default_tag_namespaces() -> Value {
    json!({
        "project": { "color": "#4a9eff", "values": [] },
        "lang": { "color": "#e0607e", "values": [] },
        "area": { "color": "#5fb8b0", "values": [] },
        "status": { "color": "#a875d1", "values": [] },
    })
}

async fn get_tag_settings(State(store): State<Store>) -> Result<Json<Value>, ApiError> {
    let raw = store.get_meta("tag_namespaces").await?;
    let registry = match raw {
        Some(s) => serde_json::from_str(&s).unwrap_or_else(|_| default_tag_namespaces()),
        None => default_tag_namespaces(),
    };
    Ok(Json(registry))
}

async fn save_tag_settings(
    State(store): State<Store>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    store.set_meta("tag_namespaces", &body.to_string()).await?;
    Ok(Json(json!({ "saved": true })))
}

// --- suggest sessions ---

async fn start_suggest_session(
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

async fn suggest_session_status(
    Extension(mgr): Extension<Arc<SuggestSessionManager>>,
) -> Json<Value> {
    Json(mgr.status().await)
}

#[derive(Deserialize)]
struct ReviseBody {
    edge_id: String,
    feedback: String,
}

async fn revise_suggest_session(
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

async fn end_suggest_session(Extension(mgr): Extension<Arc<SuggestSessionManager>>) -> Json<Value> {
    mgr.end().await;
    Json(json!({ "ended": true }))
}

#[cfg(test)]
#[path = "api_tests.rs"]
mod tests;
