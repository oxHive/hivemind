use crate::{config::SyncSettings, store::SqliteStore};
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
type Events = broadcast::Sender<()>;

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

pub fn router(store: Store, sync: SyncSettings, dashboard_origin: &str, events: Events) -> Router {
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
        .with_state(store)
        .layer(Extension(sync))
        .layer(Extension(events))
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
        .filter_map(|msg| msg.ok().map(|_| Ok(Event::default().data("changed"))));
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
    let _ = events.send(());
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
    let _ = events.send(());
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
    let _ = events.send(());
    Ok(Json(json!({ "deleted": true, "id": id })))
}

async fn delete_all_memories(
    State(store): State<Store>,
    Extension(events): Extension<Events>,
) -> Result<Json<Value>, ApiError> {
    let deleted = store.delete_all().await?;
    let _ = events.send(());
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
        if matches!(
            store
                .create_edge(&e.source_id, &e.target_id, &e.relationship)
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
            let _ = events.send(());
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
    let _ = events.send(());
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{config::SyncSettings, db, store::SqliteStore};
    use axum::body::Body;
    use axum::http::Request;
    use http_body_util::BodyExt;
    use std::sync::Arc;
    use tempfile::TempDir;
    use tower::ServiceExt;

    async fn test_store() -> (Arc<SqliteStore>, TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.db");
        let sync = SyncSettings::default();
        let database = db::open_database(&sync, path.to_str().unwrap())
            .await
            .unwrap();
        let conn = database.connect().unwrap();
        db::run_migrations(&conn).await.unwrap();
        (Arc::new(SqliteStore::new(conn)), dir)
    }

    async fn test_router() -> (Router, TempDir) {
        let (store, dir) = test_store().await;
        let (events, _) = broadcast::channel(16);
        let r = router(
            store,
            SyncSettings::default(),
            "http://127.0.0.1:3457",
            events,
        );
        (r, dir)
    }

    async fn test_router_with_store() -> (Router, Arc<SqliteStore>, TempDir) {
        let (store, dir) = test_store().await;
        let (events, _) = broadcast::channel(16);
        let r = router(
            Arc::clone(&store),
            SyncSettings::default(),
            "http://127.0.0.1:3457",
            events,
        );
        (r, store, dir)
    }

    async fn req(app: Router, method: &str, uri: &str, body: Option<Value>) -> (StatusCode, Value) {
        let builder = Request::builder().method(method).uri(uri);
        let request = match body {
            Some(v) => builder
                .header("content-type", "application/json")
                .body(Body::from(v.to_string()))
                .unwrap(),
            None => builder.body(Body::empty()).unwrap(),
        };
        let resp = app.oneshot(request).await.unwrap();
        let status = resp.status();
        let bytes = resp.into_body().collect().await.unwrap().to_bytes();
        let val = if bytes.is_empty() {
            Value::Null
        } else {
            serde_json::from_slice(&bytes).unwrap()
        };
        (status, val)
    }

    fn memory_body(title: &str, content: &str, tags: &[&str]) -> Value {
        json!({ "title": title, "content": content, "tags": tags })
    }

    #[tokio::test]
    async fn memories_crud_roundtrip() {
        let (app, _dir) = test_router().await;
        let (st, created) = req(
            app.clone(),
            "POST",
            "/api/v1/memories",
            Some(memory_body(
                "golang preferences",
                "uber/zap, sqlc, pgx v5",
                &["golang"],
            )),
        )
        .await;
        assert_eq!(st, StatusCode::CREATED);
        let id = created["id"].as_str().unwrap().to_string();
        assert!(id.starts_with("mem_"));

        let (st, list) = req(app.clone(), "GET", "/api/v1/memories", None).await;
        assert_eq!(st, StatusCode::OK);
        assert_eq!(list["count"], 1);
        assert_eq!(list["memories"][0]["title"], "golang preferences");

        let (st, one) = req(app.clone(), "GET", &format!("/api/v1/memories/{id}"), None).await;
        assert_eq!(st, StatusCode::OK);
        assert_eq!(one["content"], "uber/zap, sqlc, pgx v5");

        let (st, patched) = req(
            app.clone(),
            "PATCH",
            &format!("/api/v1/memories/{id}"),
            Some(json!({ "content": "now pgx v6" })),
        )
        .await;
        assert_eq!(st, StatusCode::OK);
        assert_eq!(patched["content"], "now pgx v6");

        let (st, _) = req(
            app.clone(),
            "DELETE",
            &format!("/api/v1/memories/{id}"),
            None,
        )
        .await;
        assert_eq!(st, StatusCode::OK);
        let (st, _) = req(app.clone(), "GET", &format!("/api/v1/memories/{id}"), None).await;
        assert_eq!(st, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn patch_memory_updates_title_and_returns_full_entry() {
        let (app, _dir) = test_router().await;
        let (_, created) = req(
            app.clone(),
            "POST",
            "/api/v1/memories",
            Some(memory_body("old", "content", &[])),
        )
        .await;
        let id = created["id"].as_str().unwrap().to_string();
        let (st, body) = req(
            app.clone(),
            "PATCH",
            &format!("/api/v1/memories/{id}"),
            Some(json!({ "title": "renamed" })),
        )
        .await;
        assert_eq!(st, StatusCode::OK);
        assert_eq!(body["title"], "renamed");
        assert_eq!(body["content"], "content");
        assert!(body["updated_at"].is_i64());
    }

    #[tokio::test]
    async fn status_reports_version_and_count() {
        let (app, _dir) = test_router().await;
        req(
            app.clone(),
            "POST",
            "/api/v1/memories",
            Some(memory_body("m", "x", &[])),
        )
        .await;
        let (st, status) = req(app.clone(), "GET", "/api/v1/status", None).await;
        assert_eq!(st, StatusCode::OK);
        assert_eq!(status["version"], env!("CARGO_PKG_VERSION"));
        assert_eq!(status["memory_count"], 1);
        assert_eq!(status["sync"]["enabled"], false);
    }

    #[tokio::test]
    async fn settings_sync_returns_defaults() {
        let (app, _dir) = test_router().await;
        let (status, body) = req(app, "GET", "/api/v1/settings/sync", None).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["enabled"], false);
        assert_eq!(body["interval_seconds"], 300);
    }

    #[tokio::test]
    async fn delete_memory_returns_404_when_not_found() {
        let (app, _dir) = test_router().await;
        let (status, _) = req(app, "DELETE", "/api/v1/memories/mem_nonexistent", None).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn list_conflicts_returns_empty() {
        let (app, _dir) = test_router().await;
        let (status, body) = req(app, "GET", "/api/v1/conflicts", None).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["count"], 0);
    }

    #[tokio::test]
    async fn save_sync_settings_returns_not_saved() {
        let (app, _dir) = test_router().await;
        let (status, body) = req(
            app,
            "POST",
            "/api/v1/settings/sync",
            Some(serde_json::json!({"enabled": true})),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["saved"], false);
    }

    #[tokio::test]
    async fn get_tag_settings_returns_seeded_defaults_when_unset() {
        let (app, _dir) = test_router().await;
        let (status, body) = req(app, "GET", "/api/v1/settings/tags", None).await;
        assert_eq!(status, StatusCode::OK);
        assert!(body["project"]["color"].is_string());
        assert!(body["lang"]["color"].is_string());
        assert!(body["area"]["color"].is_string());
        assert!(body["status"]["color"].is_string());
        assert_eq!(body["project"]["values"], json!([]));
    }

    #[tokio::test]
    async fn save_tag_settings_persists_and_get_returns_it() {
        let (app, _dir) = test_router().await;
        let custom = json!({
            "project": { "color": "#4a9eff", "values": ["hivemind", "oxhive"] },
            "lang": { "color": "#e0607e", "values": ["rust"] },
        });
        let (status, saved) = req(
            app.clone(),
            "POST",
            "/api/v1/settings/tags",
            Some(custom.clone()),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(saved["saved"], true);

        let (status, body) = req(app, "GET", "/api/v1/settings/tags", None).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body, custom);
    }

    #[test]
    fn localhost_origins_with_localhost_url() {
        let result = localhost_origins("http://localhost:3457");
        // Should produce two origins: localhost and 127.0.0.1 sibling
        // We just verify the call succeeds and returns something
        let _ = result;
    }

    #[test]
    fn localhost_origins_with_unrecognized_origin() {
        let result = localhost_origins("https://example.com");
        let _ = result;
    }

    #[test]
    fn localhost_origins_with_empty_string() {
        let result = localhost_origins("");
        let _ = result;
    }

    #[tokio::test]
    async fn create_and_list_feedback() {
        let (app, _dir) = test_router().await;

        let (st, created_mem) = req(
            app.clone(),
            "POST",
            "/api/v1/memories",
            Some(memory_body("ref mem", "content", &[])),
        )
        .await;
        assert_eq!(st, StatusCode::CREATED);
        let mem_id = created_mem["id"].as_str().unwrap().to_string();

        let (st, fb) = req(
            app.clone(),
            "POST",
            "/api/v1/feedback",
            Some(serde_json::json!({"memory_id": mem_id, "signal": "positive", "note": "great"})),
        )
        .await;
        assert_eq!(st, StatusCode::CREATED);
        assert!(fb["id"].as_str().unwrap().starts_with("fb_"));

        let (st, list) = req(app, "GET", "/api/v1/feedback", None).await;
        assert_eq!(st, StatusCode::OK);
        assert_eq!(list["count"], 1);
    }

    #[tokio::test]
    async fn list_edges_filtered() {
        let (app, _dir) = test_router().await;

        let (_, ma) = req(
            app.clone(),
            "POST",
            "/api/v1/memories",
            Some(memory_body("A", "a", &[])),
        )
        .await;
        let (_, mb) = req(
            app.clone(),
            "POST",
            "/api/v1/memories",
            Some(memory_body("B", "b", &[])),
        )
        .await;
        let id_a = ma["id"].as_str().unwrap().to_string();
        let id_b = mb["id"].as_str().unwrap().to_string();

        let (st, _) = req(
            app.clone(),
            "POST",
            "/api/v1/edges",
            Some(serde_json::json!({"source_id": id_a, "target_id": id_b, "relationship": "sibling"})),
        )
        .await;
        assert_eq!(st, StatusCode::CREATED);

        let (st, filtered) =
            req(app, "GET", &format!("/api/v1/edges?memory_id={id_a}"), None).await;
        assert_eq!(st, StatusCode::OK);
        assert_eq!(filtered["count"], 1);
    }

    #[tokio::test]
    async fn resolve_conflict_success() {
        let (app, store, _dir) = test_router_with_store().await;

        store
            .store(&crate::store::NewMemoryRow {
                id: "mem_rc",
                title: "RC Memory",
                content: "content",
                tags: &[],
                token_count: None,
                layer: "workspace",
                memory_type: "project",
            })
            .await
            .unwrap();
        let conflict = store
            .write_conflict("mem_rc", "remote content", "content", 2, 1)
            .await
            .unwrap();

        let (status, body) = req(
            app,
            "POST",
            &format!("/api/v1/conflicts/{}/resolve", conflict.id),
            Some(serde_json::json!({"resolution": "keep_local"})),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["resolved"], true);
        assert_eq!(body["resolution"], "keep_local");
    }

    #[tokio::test]
    async fn resolve_conflict_returns_404_for_missing() {
        let (app, _dir) = test_router().await;
        let (status, _) = req(
            app,
            "POST",
            "/api/v1/conflicts/cfl_missing/resolve",
            Some(json!({ "resolution": "keep_local" })),
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn delete_all_memories_clears_store() {
        let (app, _dir) = test_router().await;
        req(
            app.clone(),
            "POST",
            "/api/v1/memories",
            Some(memory_body("a", "x", &[])),
        )
        .await;
        req(
            app.clone(),
            "POST",
            "/api/v1/memories",
            Some(memory_body("b", "y", &[])),
        )
        .await;
        let (st, body) = req(app.clone(), "DELETE", "/api/v1/memories/all", None).await;
        assert_eq!(st, StatusCode::OK);
        assert_eq!(body["deleted"], 2);
        let (_, list) = req(app, "GET", "/api/v1/memories", None).await;
        assert_eq!(list["count"], 0);
    }

    #[tokio::test]
    async fn export_import_roundtrip() {
        let (app, _dir) = test_router().await;
        req(
            app.clone(),
            "POST",
            "/api/v1/memories",
            Some(memory_body("m1", "c1", &["t"])),
        )
        .await;
        let (st, dump) = req(app.clone(), "GET", "/api/v1/export", None).await;
        assert_eq!(st, StatusCode::OK);
        assert_eq!(dump["memories"].as_array().unwrap().len(), 1);

        let (app2, _dir2) = test_router().await;
        let (st, res) = req(app2.clone(), "POST", "/api/v1/import", Some(dump)).await;
        assert_eq!(st, StatusCode::OK);
        assert_eq!(res["imported_memories"], 1);
        let (_, list) = req(app2, "GET", "/api/v1/memories", None).await;
        assert_eq!(list["memories"][0]["title"], "m1");
    }

    #[tokio::test]
    async fn patch_edge_and_feedback_status() {
        let (app, store, _dir) = test_router_with_store().await;
        let tags: Vec<String> = vec![];
        store
            .store(&crate::store::NewMemoryRow {
                id: "mem_a",
                title: "A",
                content: "a",
                tags: &tags,
                token_count: None,
                layer: "workspace",
                memory_type: "project",
            })
            .await
            .unwrap();
        store
            .store(&crate::store::NewMemoryRow {
                id: "mem_b",
                title: "B",
                content: "b",
                tags: &tags,
                token_count: None,
                layer: "workspace",
                memory_type: "project",
            })
            .await
            .unwrap();
        let crate::model::EdgeCreate::Created(edge_id) = store
            .create_edge("mem_a", "mem_b", "sibling")
            .await
            .unwrap()
        else {
            panic!()
        };
        let (st, body) = req(
            app.clone(),
            "PATCH",
            &format!("/api/v1/edges/{edge_id}"),
            Some(json!({"status": "rejected"})),
        )
        .await;
        assert_eq!(st, StatusCode::OK);
        assert_eq!(body["status"], "rejected");
        let (st, _) = req(
            app.clone(),
            "PATCH",
            &format!("/api/v1/edges/{edge_id}"),
            Some(json!({"status": "bogus"})),
        )
        .await;
        assert_eq!(st, StatusCode::UNPROCESSABLE_ENTITY);

        let fb = store
            .create_feedback("mem_a", "outdated", None)
            .await
            .unwrap();
        let (st, body) = req(
            app,
            "PATCH",
            &format!("/api/v1/feedback/{}", fb.id),
            Some(json!({"status": "dismissed"})),
        )
        .await;
        assert_eq!(st, StatusCode::OK);
        assert_eq!(body["status"], "dismissed");
    }

    #[tokio::test]
    async fn status_includes_sync_details() {
        let (app, store, _dir) = test_router_with_store().await;
        store
            .set_meta("last_synced_at", "1751600000")
            .await
            .unwrap();
        let (st, body) = req(app, "GET", "/api/v1/status", None).await;
        assert_eq!(st, StatusCode::OK);
        assert_eq!(body["sync"]["last_synced_at"], 1751600000_i64);
        assert_eq!(body["sync"]["conflict_count"], 0);
    }
}
