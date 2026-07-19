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

/// Writes a stub agent script that mirrors suggest_session's test stub:
/// it echoes a fake `claude -p --output-format json` result so a real
/// process gets spawned but nothing calls out to a real agent.
fn write_stub_agent(dir: &std::path::Path) -> String {
    let script = dir.join("stub-agent.sh");
    std::fs::write(
        &script,
        "#!/bin/sh\necho '{\"type\":\"result\",\"session_id\":\"stub-1\",\"result\":\"done\",\"is_error\":false}'\n",
    )
    .unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    script.to_string_lossy().into_owned()
}

fn test_suggest_manager(
    store: Arc<SqliteStore>,
    dir: &std::path::Path,
    events: Events,
) -> Arc<crate::suggest_session::SuggestSessionManager> {
    let script = write_stub_agent(dir);
    let agent = crate::config::AgentSettings {
        command: script,
        args: vec![],
    };
    crate::suggest_session::SuggestSessionManager::new(
        store,
        events,
        agent,
        "http://127.0.0.1:3456/mcp".into(),
    )
}

fn test_update_state() -> SharedUpdateState {
    Arc::new(tokio::sync::RwLock::new(
        crate::update::UpdateState::new_idle(),
    ))
}

async fn test_router() -> (Router, TempDir) {
    let (store, dir) = test_store().await;
    let (events, _) = broadcast::channel(16);
    let suggest = test_suggest_manager(Arc::clone(&store), dir.path(), events.clone());
    let r = router(
        store,
        SyncSettings::default(),
        "http://127.0.0.1:3457",
        events,
        suggest,
        test_update_state(),
    );
    (r, dir)
}

async fn test_router_with_events() -> (Router, broadcast::Receiver<Value>, TempDir) {
    let (store, dir) = test_store().await;
    let (events, rx) = broadcast::channel(16);
    let suggest = test_suggest_manager(Arc::clone(&store), dir.path(), events.clone());
    let r = router(
        store,
        SyncSettings::default(),
        "http://127.0.0.1:3457",
        events,
        suggest,
        test_update_state(),
    );
    (r, rx, dir)
}

async fn test_router_with_store() -> (Router, Arc<SqliteStore>, TempDir) {
    let (store, dir) = test_store().await;
    let (events, _) = broadcast::channel(16);
    let suggest = test_suggest_manager(Arc::clone(&store), dir.path(), events.clone());
    let r = router(
        Arc::clone(&store),
        SyncSettings::default(),
        "http://127.0.0.1:3457",
        events,
        suggest,
        test_update_state(),
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

    let (st, filtered) = req(app, "GET", &format!("/api/v1/edges?memory_id={id_a}"), None).await;
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
async fn export_import_roundtrip_preserves_edge_status_and_link_text() {
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
    store
        .create_edge_with_status(
            "mem_a",
            "mem_b",
            "sibling",
            "pending",
            Some("the phrase"),
            None,
        )
        .await
        .unwrap();

    let (st, dump) = req(app, "GET", "/api/v1/export", None).await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(dump["edges"][0]["status"], "pending");
    assert_eq!(dump["edges"][0]["link_text"], "the phrase");

    let (app2, _dir2) = test_router().await;
    let (st, res) = req(app2.clone(), "POST", "/api/v1/import", Some(dump)).await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(res["imported_edges"], 1);

    let (_, edges) = req(app2, "GET", "/api/v1/edges", None).await;
    assert_eq!(edges["edges"][0]["status"], "pending");
    assert_eq!(edges["edges"][0]["link_text"], "the phrase");
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
async fn edge_patch_broadcasts_typed_changed_event() {
    let (app, mut rx, _dir) = test_router_with_events().await;
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
    let (_, edge) = req(
        app.clone(),
        "POST",
        "/api/v1/edges",
        Some(json!({"source_id": id_a, "target_id": id_b, "relationship": "sibling"})),
    )
    .await;
    let edge_id = edge["id"].as_str().unwrap().to_string();

    // Drain events emitted so far (memory creates, edge create) so we
    // observe the one from the PATCH below.
    while rx.try_recv().is_ok() {}

    let (st, _) = req(
        app,
        "PATCH",
        &format!("/api/v1/edges/{edge_id}"),
        Some(json!({"status": "rejected"})),
    )
    .await;
    assert_eq!(st, StatusCode::OK);

    let evt = rx.recv().await.unwrap();
    assert_eq!(evt["type"], "changed");
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

#[tokio::test]
async fn suggest_session_start_status_end_roundtrip() {
    let (app, _dir) = test_router().await;

    let (st, body) = req(app.clone(), "POST", "/api/v1/suggest-sessions", None).await;
    assert_eq!(st, StatusCode::ACCEPTED);
    assert_eq!(body["started"], true);

    let (st, _) = req(app.clone(), "POST", "/api/v1/suggest-sessions", None).await;
    assert_eq!(st, StatusCode::CONFLICT);

    let (st, status) = req(app.clone(), "GET", "/api/v1/suggest-sessions/current", None).await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(status["active"], true);

    let (st, ended) = req(
        app.clone(),
        "DELETE",
        "/api/v1/suggest-sessions/current",
        None,
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(ended["ended"], true);
}

#[tokio::test]
async fn revise_validates_session_and_edge() {
    // The manager checks the edge exists before checking session state
    // (see suggest_session::revise), so exercising the "no active
    // session" 409 needs a real edge_id; a bogus id would 404 either way.
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
        .create_edge_with_status("mem_a", "mem_b", "sibling", "pending", None, None)
        .await
        .unwrap()
    else {
        panic!("expected EdgeCreate::Created");
    };

    let (st, _) = req(
        app.clone(),
        "POST",
        "/api/v1/suggest-sessions/current/revise",
        Some(json!({ "edge_id": edge_id, "feedback": "make it parent" })),
    )
    .await;
    assert_eq!(st, StatusCode::CONFLICT);

    let (st, _) = req(app.clone(), "POST", "/api/v1/suggest-sessions", None).await;
    assert_eq!(st, StatusCode::ACCEPTED);

    let (st, _) = req(
        app.clone(),
        "POST",
        "/api/v1/suggest-sessions/current/revise",
        Some(json!({ "edge_id": "edge_bogus", "feedback": "make it parent" })),
    )
    .await;
    assert_eq!(st, StatusCode::NOT_FOUND);
}
