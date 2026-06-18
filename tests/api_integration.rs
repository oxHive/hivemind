use reqwest::StatusCode;
use serde_json::{Value, json};
/// Integration tests: spin up a real HTTP server on a random port and test
/// the full request/response lifecycle over the network stack via reqwest.
///
/// These complement the in-file unit tests (which use tower's `oneshot`) by
/// verifying actual HTTP serialization, routing, and middleware behavior.
use std::sync::Arc;
use tokio::net::TcpListener;

async fn start_server() -> (String, tokio::task::JoinHandle<()>) {
    let mut conn = rusqlite::Connection::open_in_memory().unwrap();
    conn.execute_batch("PRAGMA foreign_keys=ON;").unwrap();
    oxhivemind::db::run_migrations(&mut conn).unwrap();
    let store = Arc::new(oxhivemind::store::SqliteStore::new(conn));

    let router = oxhivemind::api::router(
        store,
        oxhivemind::config::SyncSettings::default(),
        "http://127.0.0.1:3457",
    );
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let base = format!("http://{addr}");

    let handle = tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });

    (base, handle)
}

async fn get(client: &reqwest::Client, url: &str) -> (StatusCode, Value) {
    let resp = client.get(url).send().await.unwrap();
    let status = resp.status();
    let body: Value = resp.json().await.unwrap_or(Value::Null);
    (status, body)
}

async fn post_json(client: &reqwest::Client, url: &str, body: Value) -> (StatusCode, Value) {
    let resp = client.post(url).json(&body).send().await.unwrap();
    let status = resp.status();
    let body: Value = resp.json().await.unwrap_or(Value::Null);
    (status, body)
}

async fn patch_json(client: &reqwest::Client, url: &str, body: Value) -> (StatusCode, Value) {
    let resp = client.patch(url).json(&body).send().await.unwrap();
    let status = resp.status();
    let body: Value = resp.json().await.unwrap_or(Value::Null);
    (status, body)
}

async fn delete(client: &reqwest::Client, url: &str) -> StatusCode {
    client.delete(url).send().await.unwrap().status()
}

// ── Status ────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn status_returns_version_and_zero_count() {
    let (base, _server) = start_server().await;
    let client = reqwest::Client::new();

    let (status, body) = get(&client, &format!("{base}/api/v1/status")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["version"], env!("CARGO_PKG_VERSION"));
    assert_eq!(body["memory_count"], 0);
    assert_eq!(body["sync"]["enabled"], false);
}

// ── Memory CRUD ───────────────────────────────────────────────────────────────

#[tokio::test]
async fn memory_create_and_retrieve() {
    let (base, _server) = start_server().await;
    let client = reqwest::Client::new();

    let (status, created) = post_json(&client, &format!("{base}/api/v1/memories"),
        json!({ "title": "golang prefs", "content": "use zap and chi", "layer": "personal", "tags": ["golang"] })
    ).await;
    assert_eq!(status, StatusCode::CREATED);
    let id = created["id"].as_str().unwrap().to_string();
    assert!(id.starts_with("mem_"));

    let (status, body) = get(&client, &format!("{base}/api/v1/memories/{id}")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["title"], "golang prefs");
    assert_eq!(body["content"], "use zap and chi");
    assert_eq!(body["tags"], json!(["golang"]));
}

#[tokio::test]
async fn memory_get_returns_404_for_missing() {
    let (base, _server) = start_server().await;
    let client = reqwest::Client::new();

    let (status, _) = get(&client, &format!("{base}/api/v1/memories/mem_nope")).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn memory_create_rejects_invalid_layer() {
    let (base, _server) = start_server().await;
    let client = reqwest::Client::new();

    let (status, body) = post_json(
        &client,
        &format!("{base}/api/v1/memories"),
        json!({ "title": "t", "content": "c", "layer": "invalid", "tags": [] }),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(body["error"].as_str().unwrap().contains("invalid layer"));
}

#[tokio::test]
async fn memory_patch_updates_content() {
    let (base, _server) = start_server().await;
    let client = reqwest::Client::new();

    let (_, created) = post_json(
        &client,
        &format!("{base}/api/v1/memories"),
        json!({ "title": "original", "content": "old", "layer": "personal", "tags": [] }),
    )
    .await;
    let id = created["id"].as_str().unwrap().to_string();

    let (status, _) = patch_json(
        &client,
        &format!("{base}/api/v1/memories/{id}"),
        json!({ "content": "updated content" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let (_, body) = get(&client, &format!("{base}/api/v1/memories/{id}")).await;
    assert_eq!(body["content"], "updated content");
    assert_eq!(body["title"], "original", "title unchanged");
}

#[tokio::test]
async fn memory_delete_removes_entry() {
    let (base, _server) = start_server().await;
    let client = reqwest::Client::new();

    let (_, created) = post_json(
        &client,
        &format!("{base}/api/v1/memories"),
        json!({ "title": "to-delete", "content": "bye", "layer": "personal", "tags": [] }),
    )
    .await;
    let id = created["id"].as_str().unwrap();

    let status = delete(&client, &format!("{base}/api/v1/memories/{id}")).await;
    assert_eq!(status, StatusCode::OK);

    let (status, _) = get(&client, &format!("{base}/api/v1/memories/{id}")).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn memory_list_filters_by_layer() {
    let (base, _server) = start_server().await;
    let client = reqwest::Client::new();

    post_json(
        &client,
        &format!("{base}/api/v1/memories"),
        json!({ "title": "personal one", "content": "c", "layer": "personal", "tags": [] }),
    )
    .await;
    post_json(
        &client,
        &format!("{base}/api/v1/memories"),
        json!({ "title": "workspace one", "content": "c", "layer": "workspace",
                "tags": [], "project": "myproject" }),
    )
    .await;

    let (status, body) = get(&client, &format!("{base}/api/v1/memories?layer=personal")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["count"], 1);
    assert_eq!(body["memories"][0]["title"], "personal one");
}

// ── Search ────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn search_finds_stored_memory() {
    let (base, _server) = start_server().await;
    let client = reqwest::Client::new();

    post_json(&client, &format!("{base}/api/v1/memories"),
        json!({ "title": "db driver choice", "content": "standardized on pgx v5", "layer": "personal", "tags": ["golang"] })
    ).await;

    let (status, body) = get(&client, &format!("{base}/api/v1/search?q=pgx")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["count"], 1);
    assert_eq!(body["results"][0]["title"], "db driver choice");
    assert!(
        body["results"][0].get("content").is_none(),
        "search returns snippets not full content"
    );
}

#[tokio::test]
async fn search_returns_empty_for_no_match() {
    let (base, _server) = start_server().await;
    let client = reqwest::Client::new();

    let (status, body) = get(&client, &format!("{base}/api/v1/search?q=nonexistent_term")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["count"], 0);
}

// ── Edges ─────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn edge_create_list_and_update_status() {
    let (base, _server) = start_server().await;
    let client = reqwest::Client::new();

    let (_, a) = post_json(
        &client,
        &format!("{base}/api/v1/memories"),
        json!({ "title": "a", "content": "x", "layer": "personal", "tags": [] }),
    )
    .await;
    let (_, b) = post_json(
        &client,
        &format!("{base}/api/v1/memories"),
        json!({ "title": "b", "content": "y", "layer": "personal", "tags": [] }),
    )
    .await;
    let (aid, bid) = (
        a["id"].as_str().unwrap().to_string(),
        b["id"].as_str().unwrap().to_string(),
    );

    let (status, edge) = post_json(
        &client,
        &format!("{base}/api/v1/edges"),
        json!({ "source_id": aid, "target_id": bid, "relationship": "pairs_with" }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let eid = edge["id"].as_str().unwrap().to_string();

    let (status, _) = patch_json(
        &client,
        &format!("{base}/api/v1/edges/{eid}"),
        json!({ "status": "accepted" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let (_, edges) = get(&client, &format!("{base}/api/v1/edges?status=accepted")).await;
    assert_eq!(edges["count"], 1);
}

#[tokio::test]
async fn edge_create_rejects_duplicate() {
    let (base, _server) = start_server().await;
    let client = reqwest::Client::new();

    let (_, a) = post_json(
        &client,
        &format!("{base}/api/v1/memories"),
        json!({ "title": "a", "content": "x", "layer": "personal", "tags": [] }),
    )
    .await;
    let (_, b) = post_json(
        &client,
        &format!("{base}/api/v1/memories"),
        json!({ "title": "b", "content": "y", "layer": "personal", "tags": [] }),
    )
    .await;
    let (aid, bid) = (a["id"].as_str().unwrap(), b["id"].as_str().unwrap());

    let body = json!({ "source_id": aid, "target_id": bid, "relationship": "pairs_with" });
    post_json(&client, &format!("{base}/api/v1/edges"), body.clone()).await;
    let (status, _) = post_json(&client, &format!("{base}/api/v1/edges"), body).await;
    assert_eq!(status, StatusCode::CONFLICT);
}

// ── Feedback ──────────────────────────────────────────────────────────────────

#[tokio::test]
async fn feedback_create_and_resolve() {
    let (base, _server) = start_server().await;
    let client = reqwest::Client::new();

    let (_, mem) = post_json(
        &client,
        &format!("{base}/api/v1/memories"),
        json!({ "title": "stale pref", "content": "old content", "layer": "personal", "tags": [] }),
    )
    .await;
    let mid = mem["id"].as_str().unwrap();

    let (status, fb) = post_json(
        &client,
        &format!("{base}/api/v1/feedback"),
        json!({ "memory_id": mid, "type": "outdated", "note": "this is stale" }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let fb_id = fb["id"].as_str().unwrap().to_string();

    let (_, open) = get(&client, &format!("{base}/api/v1/feedback?status=open")).await;
    assert_eq!(open["count"], 1);
    assert_eq!(open["items"][0]["type"], "outdated");

    let (status, _) = patch_json(
        &client,
        &format!("{base}/api/v1/feedback/{fb_id}"),
        json!({ "status": "resolved" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let (_, open) = get(&client, &format!("{base}/api/v1/feedback?status=open")).await;
    assert_eq!(open["count"], 0);
}

// ── Sync endpoints ────────────────────────────────────────────────────────────

#[tokio::test]
async fn sync_push_and_pull_roundtrip() {
    let (base, _server) = start_server().await;
    let client = reqwest::Client::new();

    let record = json!({
        "id": "mem_synctest01", "layer": "personal", "memory_type": "preference",
        "title": "synced memory", "content": "via push", "source": null, "project": null,
        "tags": [], "created_at": 1000, "updated_at": 1000
    });
    let (status, body) = post_json(
        &client,
        &format!("{base}/api/sync/push"),
        json!({ "records": [record], "client_id": "test-node" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["accepted"], 1);

    let (status, pull) = get(&client, &format!("{base}/api/sync/pull?since=0")).await;
    assert_eq!(status, StatusCode::OK);
    let records = pull["records"].as_array().unwrap();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0]["title"], "synced memory");
}

#[tokio::test]
async fn sync_status_returns_server_time() {
    let (base, _server) = start_server().await;
    let client = reqwest::Client::new();

    let (status, body) = get(&client, &format!("{base}/api/sync/status")).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["server_time"].as_i64().unwrap() > 0);
    assert_eq!(body["memory_count"], 0);
}
