use std::sync::Arc;
use axum::{
    Json, Router,
    extract::{Extension, Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, patch, post},
};
use serde::Deserialize;
use serde_json::{Value, json};
use tower_http::cors::CorsLayer;
use crate::{
    config::SyncSettings,
    model::{EdgeCreate, Layer, MemoryEntry, MemoryType, NewMemory, UpdateMemory},
    store::SqliteStore,
};

const RELATIONSHIPS: &[&str] = &["shares_tag", "applies_to", "pairs_with", "used_in", "related_to", "custom"];
const EDGE_STATUSES: &[&str] = &["accepted", "pending", "rejected"];
const FEEDBACK_TYPES: &[&str] = &["incorrect", "outdated", "duplicate", "wrong_connection", "missing_connection", "other"];
const FEEDBACK_STATUSES: &[&str] = &["open", "resolved", "dismissed"];
const CONFLICT_ACTIONS: &[&str] = &["keep", "restore"];

type Store = Arc<SqliteStore>;

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

fn bad_request(msg: impl Into<String>) -> ApiError {
    ApiError(StatusCode::BAD_REQUEST, msg.into())
}

fn not_found(msg: impl Into<String>) -> ApiError {
    ApiError(StatusCode::NOT_FOUND, msg.into())
}

fn validate(value: &str, allowed: &[&str], what: &str) -> Result<(), ApiError> {
    if allowed.contains(&value) {
        Ok(())
    } else {
        Err(bad_request(format!("invalid {what}: {value} (allowed: {})", allowed.join(", "))))
    }
}

pub fn router(store: Store, sync: SyncSettings) -> Router {
    Router::new()
        .route("/api/v1/memories", get(list_memories).post(create_memory))
        .route("/api/v1/memories/{id}", get(get_memory).patch(patch_memory).delete(delete_memory))
        .route("/api/v1/search", get(search))
        .route("/api/v1/edges", get(list_edges).post(create_edge))
        .route("/api/v1/edges/{id}", patch(patch_edge))
        .route("/api/v1/feedback", get(list_feedback).post(create_feedback))
        .route("/api/v1/feedback/{id}", patch(patch_feedback))
        .route("/api/v1/conflicts", get(list_conflicts))
        .route("/api/v1/conflicts/{id}/resolve", post(resolve_conflict_handler))
        .route("/api/v1/settings/sync", get(get_sync_settings).post(save_sync_settings))
        .route("/api/v1/status", get(server_status))
        .route("/api/sync/status", get(sync_status))
        .route("/api/sync/push", post(sync_push))
        .route("/api/sync/pull", get(sync_pull))
        .with_state(store)
        .layer(Extension(sync))
        .layer(CorsLayer::permissive())
}

fn entry_json(e: &MemoryEntry) -> Value {
    json!({
        "id": e.id,
        "layer": e.layer.to_string(),
        "type": e.memory_type.to_string(),
        "title": e.title,
        "content": e.content,
        "source": e.source,
        "project": e.project,
        "tags": e.tags,
        "created_at": e.created_at,
        "updated_at": e.updated_at,
    })
}

// --- memories ---

#[derive(Deserialize)]
struct ListMemoriesParams {
    layer: Option<String>,
    limit: Option<usize>,
}

async fn list_memories(
    State(store): State<Store>,
    Query(p): Query<ListMemoriesParams>,
) -> Result<Json<Value>, ApiError> {
    let layer = match p.layer.as_deref() {
        None | Some("") | Some("all") => None,
        Some(s) => Some(s.parse::<Layer>().map_err(|e| bad_request(e.to_string()))?),
    };
    let limit = p.limit.unwrap_or(200).clamp(1, 1000);
    let entries = store.list_memories(layer, limit)?;
    Ok(Json(json!({
        "count": entries.len(),
        "memories": entries.iter().map(entry_json).collect::<Vec<_>>(),
    })))
}

#[derive(Deserialize)]
struct CreateMemoryBody {
    title: String,
    content: String,
    layer: String,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default)]
    project: Option<String>,
}

async fn create_memory(
    State(store): State<Store>,
    Json(b): Json<CreateMemoryBody>,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    let layer = b.layer.parse::<Layer>().map_err(|e| bad_request(e.to_string()))?;
    let result = store.store(NewMemory {
        title: b.title,
        content: b.content,
        layer,
        memory_type: MemoryType::Preference,
        tags: b.tags,
        project: b.project,
        source: Some("dashboard".to_string()),
    })?;
    Ok((StatusCode::CREATED, Json(json!({ "id": result.id, "auto_connected": result.auto_connected }))))
}

async fn get_memory(
    State(store): State<Store>,
    Path(id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    match store.recall_by_id(&id)? {
        None => Err(not_found(format!("no memory {id}"))),
        Some(e) => Ok(Json(entry_json(&e))),
    }
}

#[derive(Deserialize)]
struct PatchMemoryBody {
    title: Option<String>,
    content: Option<String>,
    tags: Option<Vec<String>>,
    #[serde(default)]
    merge_content: bool,
}

async fn patch_memory(
    State(store): State<Store>,
    Path(id): Path<String>,
    Json(b): Json<PatchMemoryBody>,
) -> Result<Json<Value>, ApiError> {
    let updated = store.update(&id, UpdateMemory {
        title: b.title,
        content: b.content,
        tags: b.tags,
        merge_content: b.merge_content,
    })?;
    if !updated {
        return Err(not_found(format!("no memory {id}")));
    }
    Ok(Json(json!({ "updated": true, "id": id })))
}

async fn delete_memory(
    State(store): State<Store>,
    Path(id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    if !store.delete(&id)? {
        return Err(not_found(format!("no memory {id}")));
    }
    Ok(Json(json!({ "deleted": true, "id": id })))
}

// --- search ---

#[derive(Deserialize)]
struct SearchParams {
    q: String,
    limit: Option<u32>,
}

async fn search(
    State(store): State<Store>,
    Query(p): Query<SearchParams>,
) -> Result<Json<Value>, ApiError> {
    let limit = p.limit.unwrap_or(20).clamp(1, 50) as usize;
    let hits = store.search(&p.q, limit)?;
    let results: Vec<_> = hits.iter().map(|h| json!({
        "id": h.id,
        "title": h.title,
        "snippet": h.snippet,
        "layer": h.layer.to_string(),
        "tags": h.tags,
    })).collect();
    Ok(Json(json!({ "count": results.len(), "results": results })))
}

// --- edges ---

#[derive(Deserialize)]
struct EdgesParams {
    status: Option<String>,
}

async fn list_edges(
    State(store): State<Store>,
    Query(p): Query<EdgesParams>,
) -> Result<Json<Value>, ApiError> {
    if let Some(ref st) = p.status {
        validate(st, EDGE_STATUSES, "edge status")?;
    }
    let edges = store.list_edges(p.status.as_deref())?;
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
    Json(b): Json<CreateEdgeBody>,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    validate(&b.relationship, RELATIONSHIPS, "relationship")?;
    match store.create_edge(&b.source_id, &b.target_id, &b.relationship)? {
        EdgeCreate::Created(id) => Ok((StatusCode::CREATED, Json(json!({ "id": id })))),
        EdgeCreate::Duplicate => Err(ApiError(StatusCode::CONFLICT, "edge already exists".to_string())),
        EdgeCreate::MissingEndpoint => Err(not_found("source or target memory does not exist")),
    }
}

#[derive(Deserialize)]
struct PatchEdgeBody {
    status: String,
}

async fn patch_edge(
    State(store): State<Store>,
    Path(id): Path<String>,
    Json(b): Json<PatchEdgeBody>,
) -> Result<Json<Value>, ApiError> {
    validate(&b.status, EDGE_STATUSES, "edge status")?;
    if !store.set_edge_status(&id, &b.status)? {
        return Err(not_found(format!("no edge {id}")));
    }
    Ok(Json(json!({ "updated": true, "id": id, "status": b.status })))
}

// --- feedback ---

#[derive(Deserialize)]
struct FeedbackParams {
    status: Option<String>,
}

async fn list_feedback(
    State(store): State<Store>,
    Query(p): Query<FeedbackParams>,
) -> Result<Json<Value>, ApiError> {
    if let Some(ref st) = p.status {
        validate(st, FEEDBACK_STATUSES, "feedback status")?;
    }
    let items = store.list_feedback(p.status.as_deref())?;
    Ok(Json(json!({ "count": items.len(), "items": items })))
}

#[derive(Deserialize)]
struct CreateFeedbackBody {
    #[serde(default)]
    memory_id: Option<String>,
    #[serde(default)]
    edge_id: Option<String>,
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    note: Option<String>,
}

async fn create_feedback(
    State(store): State<Store>,
    Json(b): Json<CreateFeedbackBody>,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    validate(&b.kind, FEEDBACK_TYPES, "feedback type")?;
    if b.memory_id.is_none() && b.edge_id.is_none() {
        return Err(bad_request("provide memory_id or edge_id"));
    }
    match store.create_feedback(b.memory_id.as_deref(), b.edge_id.as_deref(), &b.kind, b.note.as_deref())? {
        None => Err(not_found("referenced memory or edge does not exist")),
        Some(id) => Ok((StatusCode::CREATED, Json(json!({ "id": id })))),
    }
}

#[derive(Deserialize)]
struct PatchFeedbackBody {
    status: String,
}

async fn patch_feedback(
    State(store): State<Store>,
    Path(id): Path<String>,
    Json(b): Json<PatchFeedbackBody>,
) -> Result<Json<Value>, ApiError> {
    validate(&b.status, FEEDBACK_STATUSES, "feedback status")?;
    if !store.set_feedback_status(&id, &b.status)? {
        return Err(not_found(format!("no feedback {id}")));
    }
    Ok(Json(json!({ "updated": true, "id": id, "status": b.status })))
}

// --- conflicts + status ---

#[derive(Deserialize)]
struct ConflictQuery { status: Option<String> }

async fn list_conflicts(
    State(store): State<Store>,
    Query(p): Query<ConflictQuery>,
) -> Result<Json<Value>, ApiError> {
    let items = store.list_conflicts(p.status.as_deref())?;
    Ok(Json(json!({ "count": items.len(), "conflicts": items })))
}

async fn server_status(
    State(store): State<Store>,
    Extension(sync): Extension<SyncSettings>,
) -> Result<Json<Value>, ApiError> {
    let last_synced_at = store.get_kv("last_synced_at")?
        .and_then(|v| v.parse::<i64>().ok());
    let conflict_count = store.list_conflicts(Some("open"))?.len();
    Ok(Json(json!({
        "version": env!("CARGO_PKG_VERSION"),
        "memory_count": store.count()?,
        "db_path": crate::db::resolve_db_path(),
        "sync": {
            "enabled": sync.enabled,
            "last_synced_at": last_synced_at,
            "conflict_count": conflict_count,
        },
    })))
}

// --- sync server endpoints ---

async fn sync_status(State(store): State<Store>) -> Result<Json<Value>, ApiError> {
    let server_time = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() as i64;
    Ok(Json(json!({ "server_time": server_time, "memory_count": store.count()? })))
}

#[derive(Deserialize)]
struct PushBody {
    records: Vec<MemoryEntry>,
    #[allow(dead_code)]
    client_id: Option<String>,
}

async fn sync_push(
    State(store): State<Store>,
    Json(body): Json<PushBody>,
) -> Result<Json<Value>, ApiError> {
    let mut accepted = 0usize;
    let mut conflicts = vec![];
    for record in &body.records {
        match store.upsert_memory(record)? {
            Some(c) => conflicts.push(c),
            None => accepted += 1,
        }
    }
    Ok(Json(json!({ "accepted": accepted, "conflicts": conflicts })))
}

#[derive(Deserialize)]
struct PullQuery { since: Option<i64> }

async fn sync_pull(
    State(store): State<Store>,
    Query(q): Query<PullQuery>,
) -> Result<Json<Value>, ApiError> {
    let records = store.memories_since(q.since.unwrap_or(0))?;
    let server_time = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() as i64;
    Ok(Json(json!({ "records": records, "server_time": server_time })))
}

// --- conflict resolution ---

#[derive(Deserialize)]
struct ResolveBody { action: String }

async fn resolve_conflict_handler(
    State(store): State<Store>,
    Path(id): Path<String>,
    Json(b): Json<ResolveBody>,
) -> Result<Json<Value>, ApiError> {
    validate(&b.action, CONFLICT_ACTIONS, "action")?;
    if !store.resolve_conflict(&id, &b.action)? {
        return Err(not_found(format!("conflict {id} not found or already resolved")));
    }
    Ok(Json(json!({ "resolved": true, "id": id, "action": b.action })))
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
    Json(json!({ "saved": false, "message": "Sync settings are managed via config.toml — restart hivemind after editing." }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::Request;
    use http_body_util::BodyExt;
    use tower::ServiceExt;
    use crate::db;

    fn test_router() -> Router {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys=ON;").unwrap();
        db::create_schema(&conn).unwrap();
        router(Arc::new(SqliteStore::new(conn)), crate::config::SyncSettings::default())
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
        let val = if bytes.is_empty() { Value::Null } else { serde_json::from_slice(&bytes).unwrap() };
        (status, val)
    }

    fn memory_body(title: &str, content: &str, tags: &[&str]) -> Value {
        json!({ "title": title, "content": content, "layer": "personal", "tags": tags })
    }

    #[tokio::test]
    async fn memories_crud_roundtrip() {
        let app = test_router();
        let (st, created) = req(app.clone(), "POST", "/api/v1/memories",
            Some(memory_body("golang preferences", "uber/zap, sqlc, pgx v5", &["golang"]))).await;
        assert_eq!(st, StatusCode::CREATED);
        let id = created["id"].as_str().unwrap().to_string();
        assert!(id.starts_with("mem_"));

        let (st, list) = req(app.clone(), "GET", "/api/v1/memories", None).await;
        assert_eq!(st, StatusCode::OK);
        assert_eq!(list["count"], 1);
        assert_eq!(list["memories"][0]["title"], "golang preferences");
        assert_eq!(list["memories"][0]["tags"][0], "golang");

        let (st, one) = req(app.clone(), "GET", &format!("/api/v1/memories/{id}"), None).await;
        assert_eq!(st, StatusCode::OK);
        assert_eq!(one["content"], "uber/zap, sqlc, pgx v5");

        let (st, patched) = req(app.clone(), "PATCH", &format!("/api/v1/memories/{id}"),
            Some(json!({ "content": "now pgx v6" }))).await;
        assert_eq!(st, StatusCode::OK);
        assert_eq!(patched["updated"], true);

        let (st, _) = req(app.clone(), "DELETE", &format!("/api/v1/memories/{id}"), None).await;
        assert_eq!(st, StatusCode::OK);
        let (st, _) = req(app.clone(), "GET", &format!("/api/v1/memories/{id}"), None).await;
        assert_eq!(st, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn list_memories_rejects_bad_layer_and_filters_good_one() {
        let app = test_router();
        req(app.clone(), "POST", "/api/v1/memories",
            Some(memory_body("p", "personal entry", &[]))).await;
        let (st, _) = req(app.clone(), "GET", "/api/v1/memories?layer=bogus", None).await;
        assert_eq!(st, StatusCode::BAD_REQUEST);
        let (st, list) = req(app.clone(), "GET", "/api/v1/memories?layer=workspace", None).await;
        assert_eq!(st, StatusCode::OK);
        assert_eq!(list["count"], 0);
    }

    #[tokio::test]
    async fn search_returns_snippets() {
        let app = test_router();
        req(app.clone(), "POST", "/api/v1/memories",
            Some(memory_body("db choice", "standardized on pgx v5", &["golang"]))).await;
        let (st, hits) = req(app.clone(), "GET", "/api/v1/search?q=pgx", None).await;
        assert_eq!(st, StatusCode::OK);
        assert_eq!(hits["count"], 1);
        assert_eq!(hits["results"][0]["title"], "db choice");
        assert!(hits["results"][0].get("content").is_none(), "snippets only");
    }

    #[tokio::test]
    async fn edges_flow_create_duplicate_and_status_patch() {
        let app = test_router();
        let (_, a) = req(app.clone(), "POST", "/api/v1/memories",
            Some(memory_body("a", "x", &[]))).await;
        let (_, b) = req(app.clone(), "POST", "/api/v1/memories",
            Some(memory_body("b", "y", &[]))).await;
        let (a, b) = (a["id"].as_str().unwrap().to_string(), b["id"].as_str().unwrap().to_string());

        let edge_body = json!({ "source_id": a, "target_id": b, "relationship": "pairs_with" });
        let (st, created) = req(app.clone(), "POST", "/api/v1/edges", Some(edge_body.clone())).await;
        assert_eq!(st, StatusCode::CREATED);
        let edge_id = created["id"].as_str().unwrap().to_string();

        let (st, _) = req(app.clone(), "POST", "/api/v1/edges", Some(edge_body)).await;
        assert_eq!(st, StatusCode::CONFLICT);

        let (st, _) = req(app.clone(), "POST", "/api/v1/edges",
            Some(json!({ "source_id": a, "target_id": "mem_nope", "relationship": "pairs_with" }))).await;
        assert_eq!(st, StatusCode::NOT_FOUND);

        let (st, _) = req(app.clone(), "POST", "/api/v1/edges",
            Some(json!({ "source_id": a, "target_id": b, "relationship": "invented" }))).await;
        assert_eq!(st, StatusCode::BAD_REQUEST);

        let (st, _) = req(app.clone(), "PATCH", &format!("/api/v1/edges/{edge_id}"),
            Some(json!({ "status": "rejected" }))).await;
        assert_eq!(st, StatusCode::OK);
        let (_, edges) = req(app.clone(), "GET", "/api/v1/edges?status=rejected", None).await;
        assert_eq!(edges["count"], 1);
        let (st, _) = req(app.clone(), "PATCH", "/api/v1/edges/edge_nope",
            Some(json!({ "status": "accepted" }))).await;
        assert_eq!(st, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn feedback_flow_and_conflicts_empty() {
        let app = test_router();
        let (_, m) = req(app.clone(), "POST", "/api/v1/memories",
            Some(memory_body("m", "x", &[]))).await;
        let mid = m["id"].as_str().unwrap().to_string();

        let (st, fb) = req(app.clone(), "POST", "/api/v1/feedback",
            Some(json!({ "memory_id": mid, "type": "outdated", "note": "stale" }))).await;
        assert_eq!(st, StatusCode::CREATED);
        let fb_id = fb["id"].as_str().unwrap().to_string();

        let (st, _) = req(app.clone(), "POST", "/api/v1/feedback",
            Some(json!({ "memory_id": "mem_nope", "type": "outdated" }))).await;
        assert_eq!(st, StatusCode::NOT_FOUND);
        let (st, _) = req(app.clone(), "POST", "/api/v1/feedback",
            Some(json!({ "memory_id": mid, "type": "invented" }))).await;
        assert_eq!(st, StatusCode::BAD_REQUEST);

        let (_, open) = req(app.clone(), "GET", "/api/v1/feedback?status=open", None).await;
        assert_eq!(open["count"], 1);
        assert_eq!(open["items"][0]["type"], "outdated");

        let (st, _) = req(app.clone(), "PATCH", &format!("/api/v1/feedback/{fb_id}"),
            Some(json!({ "status": "resolved" }))).await;
        assert_eq!(st, StatusCode::OK);
        let (_, open) = req(app.clone(), "GET", "/api/v1/feedback?status=open", None).await;
        assert_eq!(open["count"], 0);

        let (st, conflicts) = req(app.clone(), "GET", "/api/v1/conflicts", None).await;
        assert_eq!(st, StatusCode::OK);
        assert_eq!(conflicts["count"], 0);
    }

    #[tokio::test]
    async fn status_reports_version_and_count() {
        let app = test_router();
        req(app.clone(), "POST", "/api/v1/memories",
            Some(memory_body("m", "x", &[]))).await;
        let (st, status) = req(app.clone(), "GET", "/api/v1/status", None).await;
        assert_eq!(st, StatusCode::OK);
        assert_eq!(status["version"], env!("CARGO_PKG_VERSION"));
        assert_eq!(status["memory_count"], 1);
        assert_eq!(status["sync"]["enabled"], false);
    }

    #[tokio::test]
    async fn sync_status_returns_server_time_and_count() {
        let (status, body) = req(test_router(), "GET", "/api/sync/status", None).await;
        assert_eq!(status, StatusCode::OK);
        assert!(body["server_time"].is_number());
        assert_eq!(body["memory_count"], 0);
    }

    #[tokio::test]
    async fn sync_push_inserts_new_memory() {
        let app = test_router();
        let record = json!({
            "id": "mem_test001", "layer": "personal", "memory_type": "preference",
            "title": "Test", "content": "Content", "source": null, "project": null,
            "tags": ["tag1"], "created_at": 1000, "updated_at": 1000
        });
        let (status, body) = req(app, "POST", "/api/sync/push",
            Some(json!({ "records": [record], "client_id": "test-client" }))).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["accepted"], 1);
        assert_eq!(body["conflicts"].as_array().unwrap().len(), 0);
    }

    #[tokio::test]
    async fn sync_pull_returns_records_since() {
        let app = test_router();
        req(app.clone(), "POST", "/api/v1/memories",
            Some(json!({ "title": "T", "content": "C", "layer": "personal", "tags": [] }))).await;
        let (status, body) = req(app, "GET", "/api/sync/pull?since=0", None).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["records"].as_array().unwrap().len(), 1);
        assert!(body["server_time"].is_number());
    }

    #[tokio::test]
    async fn resolve_conflict_returns_404_for_missing() {
        let (status, _) = req(test_router(), "POST", "/api/v1/conflicts/cfl_missing/resolve",
            Some(json!({ "action": "keep" }))).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn resolve_conflict_rejects_invalid_action() {
        let (status, _) = req(test_router(), "POST", "/api/v1/conflicts/any/resolve",
            Some(json!({ "action": "delete" }))).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn settings_sync_returns_defaults() {
        let (status, body) = req(test_router(), "GET", "/api/v1/settings/sync", None).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["enabled"], false);
        assert_eq!(body["interval_seconds"], 300);
    }

    #[tokio::test]
    async fn server_status_includes_sync_info() {
        let (status, body) = req(test_router(), "GET", "/api/v1/status", None).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["sync"]["enabled"], false);
    }
}
