use crate::{
    api,
    config::{AgentSettings, ServerSettings, SyncSettings},
    server::HiveMind,
    store::SqliteStore,
    suggest_session::SuggestSessionManager,
};
use anyhow::Result;
use axum::{
    Router,
    body::Body,
    http::{StatusCode, header},
    response::Response,
    routing::get,
};
use include_dir::{Dir, include_dir};
use rmcp::transport::streamable_http_server::{
    StreamableHttpService, session::local::LocalSessionManager,
};
use std::sync::Arc;

static DASHBOARD: Dir = include_dir!("$CARGO_MANIFEST_DIR/dashboard/dist");
static PLACEHOLDER_HTML: &str = include_str!("dashboard_placeholder.html");

pub fn app_router(
    store: Arc<SqliteStore>,
    sync: SyncSettings,
    notify_on_store: Option<Arc<tokio::sync::Notify>>,
    dashboard_origin: &str,
    events_tx: tokio::sync::broadcast::Sender<serde_json::Value>,
    agent: AgentSettings,
    mcp_url: String,
) -> Router {
    // Fires whenever a memory or edge is created/updated/deleted, either via
    // an MCP tool call (below) or the REST API (api::router) — the dashboard
    // subscribes to it over SSE to silently refresh in the background.
    let mcp = StreamableHttpService::new(
        {
            let store = store.clone();
            let trigger = notify_on_store.clone();
            let events_tx = events_tx.clone();
            move || {
                let hivemind = match &trigger {
                    Some(t) => HiveMind::with_sync(store.clone(), t.clone()),
                    None => HiveMind::with_store(store.clone()),
                };
                Ok(hivemind.with_events(events_tx.clone()))
            }
        },
        Arc::new(LocalSessionManager::default()),
        Default::default(),
    );
    let suggest = SuggestSessionManager::new(store.clone(), events_tx.clone(), agent, mcp_url);
    api::router(store, sync, dashboard_origin, events_tx, suggest).nest_service("/mcp", mcp)
}

pub fn dashboard_router(api_url: &str) -> Router {
    let config_js = format!("window.HIVEMIND_API = {};\n", serde_json::json!(api_url));
    Router::new()
        .route(
            "/config.js",
            get({
                let body = config_js.clone();
                move || {
                    let b = body.clone();
                    async move {
                        Response::builder()
                            .header(header::CONTENT_TYPE, "application/javascript")
                            .body(Body::from(b))
                            .unwrap()
                    }
                }
            }),
        )
        .fallback(get(|req: axum::extract::Request| async move {
            if !dashboard_is_bundled() {
                return Response::builder()
                    .header(header::CONTENT_TYPE, "text/html")
                    .body(Body::from(PLACEHOLDER_HTML))
                    .unwrap();
            }
            let path = req.uri().path().trim_start_matches('/');
            let path = if path.is_empty() { "index.html" } else { path };
            match DASHBOARD.get_file(path) {
                Some(file) => {
                    let mime = mime_guess::from_path(path).first_or_octet_stream();
                    Response::builder()
                        .header(header::CONTENT_TYPE, mime.as_ref())
                        .body(Body::from(file.contents()))
                        .unwrap()
                }
                None => {
                    // SPA fallback: serve index.html for unknown paths
                    match DASHBOARD.get_file("index.html") {
                        Some(file) => Response::builder()
                            .header(header::CONTENT_TYPE, "text/html")
                            .body(Body::from(file.contents()))
                            .unwrap(),
                        None => Response::builder()
                            .status(StatusCode::NOT_FOUND)
                            .body(Body::from("not found"))
                            .unwrap(),
                    }
                }
            }
        }))
}

/// A real vite build ships an assets/ directory; a source install has an
/// empty dashboard/dist (see build.rs) and falls back to PLACEHOLDER_HTML.
fn dashboard_is_bundled() -> bool {
    DASHBOARD.get_dir("assets").is_some()
}

/// Bridges writes made by other processes (the stdio MCP server the Claude
/// Code plugin spawns) into the dashboard SSE stream. In-process writes
/// already emit directly; data_version only moves on foreign commits.
pub fn spawn_change_poller(
    store: Arc<SqliteStore>,
    events: tokio::sync::broadcast::Sender<serde_json::Value>,
    interval: std::time::Duration,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut last: Option<i64> = None;
        loop {
            tokio::time::sleep(interval).await;
            match store.data_version().await {
                Ok(v) => {
                    if let Some(prev) = last
                        && v != prev
                    {
                        let _ = events.send(serde_json::json!({ "type": "changed" }));
                    }
                    last = Some(v);
                }
                Err(e) => tracing::debug!("data_version poll failed: {e:#}"),
            }
        }
    })
}

pub async fn run_up(
    store: Arc<SqliteStore>,
    settings: &ServerSettings,
    headless: bool,
    notify_on_store: Option<Arc<tokio::sync::Notify>>,
) -> Result<()> {
    let (events_tx, _) = tokio::sync::broadcast::channel::<serde_json::Value>(16);
    spawn_change_poller(
        store.clone(),
        events_tx.clone(),
        std::time::Duration::from_secs(2),
    );
    let mcp_host = match settings.host.as_str() {
        "0.0.0.0" | "::" => "127.0.0.1",
        h => h,
    };
    let mcp_url = format!("http://{}:{}/mcp", mcp_host, settings.port);
    let app = app_router(
        store.clone(),
        settings.sync.clone(),
        notify_on_store,
        &settings.cors_origin,
        events_tx,
        settings.agent.clone(),
        mcp_url,
    );

    if !matches!(settings.host.as_str(), "127.0.0.1" | "localhost" | "::1") {
        tracing::warn!(
            "binding to {}: the REST API and MCP endpoint are UNAUTHENTICATED; \
             anyone who can reach this address can read and modify all memories, \
             and can call POST /api/v1/suggest-sessions to spawn the configured agent command",
            settings.host
        );
    }

    let listener = tokio::net::TcpListener::bind((settings.host.as_str(), settings.port)).await?;
    tracing::info!(
        "MCP endpoint:  http://{}:{}/mcp",
        settings.host,
        settings.port
    );
    tracing::info!(
        "REST API:      http://{}:{}/api/v1",
        settings.host,
        settings.port
    );
    if settings.sync.enabled {
        tracing::info!("Sync:          enabled → {}", settings.sync.remote_url);
    }
    if headless {
        axum::serve(listener, app).await?;
        return Ok(());
    }
    if !dashboard_is_bundled() {
        tracing::warn!(
            "dashboard assets are not bundled in this build (source install). \
             The dashboard page will show setup instructions. \
             Use a prebuilt release binary, or run `bun install && bun run build` in dashboard/ and rebuild."
        );
    }
    let dash = dashboard_router(&settings.api_url);
    let dash_listener =
        tokio::net::TcpListener::bind((settings.host.as_str(), settings.dashboard_port)).await?;
    tracing::info!(
        "Dashboard:     http://{}:{}",
        settings.host,
        settings.dashboard_port
    );
    tokio::try_join!(
        async {
            axum::serve(listener, app)
                .await
                .map_err(anyhow::Error::from)
        },
        async {
            axum::serve(dash_listener, dash)
                .await
                .map_err(anyhow::Error::from)
        },
    )?;
    Ok(())
}

pub async fn run_dashboard(settings: &ServerSettings, open: bool) -> Result<()> {
    let dash = dashboard_router(&settings.api_url);
    let listener =
        tokio::net::TcpListener::bind((settings.host.as_str(), settings.dashboard_port)).await?;
    let url = format!("http://{}:{}", settings.host, settings.dashboard_port);
    tracing::info!("Dashboard:     {url}  (API: {})", settings.api_url);
    if open {
        let opener = if cfg!(target_os = "macos") {
            "open"
        } else {
            "xdg-open"
        };
        let _ = std::process::Command::new(opener).arg(&url).spawn();
    }
    axum::serve(listener, dash).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{config::SyncSettings, db, store::SqliteStore};

    #[tokio::test]
    async fn unbundled_dashboard_serves_placeholder() {
        // Only meaningful for a source build without dashboard/dist/assets;
        // a local `bun run build` before `cargo build` legitimately bundles
        // the real dashboard and this assertion is skipped.
        if dashboard_is_bundled() {
            return;
        }
        let dash = dashboard_router("http://127.0.0.1:3456");
        let resp = dash
            .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = resp.into_body().collect().await.unwrap().to_bytes();
        assert!(std::str::from_utf8(&body).unwrap().contains("not bundled"));
    }
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
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

    /// Writes a stub agent script (mirrors the suggest_session test stub) so
    /// app_router tests don't depend on a real `claude` binary being on PATH.
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

    #[tokio::test]
    async fn app_router_serves_rest_api() {
        let (store, dir) = test_store().await;
        let (events_tx, _) = tokio::sync::broadcast::channel::<serde_json::Value>(16);
        let agent = crate::config::AgentSettings {
            command: write_stub_agent(dir.path()),
            args: vec![],
        };
        let app = app_router(
            store,
            crate::config::SyncSettings::default(),
            None,
            "http://127.0.0.1:3457",
            events_tx,
            agent,
            "http://127.0.0.1:3456/mcp".into(),
        );
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/api/v1/status")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn poller_emits_changed_when_other_connection_writes() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("hm.db");
        let db1 = libsql::Builder::new_local(&path).build().await.unwrap();
        let conn1 = db1.connect().unwrap();
        crate::db::run_migrations(&conn1).await.unwrap();
        let store = Arc::new(crate::store::SqliteStore::new(conn1));

        let (events, mut rx) = tokio::sync::broadcast::channel::<serde_json::Value>(16);
        let _h = spawn_change_poller(store, events, std::time::Duration::from_millis(50));

        // let the poller take its baseline reading first
        tokio::time::sleep(std::time::Duration::from_millis(120)).await;

        // foreign write through a second connection
        let db2 = libsql::Builder::new_local(&path).build().await.unwrap();
        let conn2 = db2.connect().unwrap();
        crate::db::init_connection(&conn2).await.unwrap();
        conn2
            .execute(
                "INSERT INTO memories (id, title, content, created_at, updated_at, token_count, layer, memory_type)
                 VALUES ('mem_x', 't', 'c', 1, 1, 1, 'workspace', 'project')",
                (),
            )
            .await
            .unwrap();

        let evt = tokio::time::timeout(std::time::Duration::from_secs(2), rx.recv())
            .await
            .expect("poller should emit within 2s")
            .unwrap();
        assert_eq!(evt["type"], "changed");
    }

    #[tokio::test]
    async fn dashboard_router_spa_fallback_returns_html_for_unknown_path() {
        let dash = dashboard_router("http://127.0.0.1:3456");
        let resp = dash
            .oneshot(
                Request::builder()
                    .uri("/some/unknown/route")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        // SPA fallback: unknown paths serve index.html (200 with html content-type)
        assert_eq!(resp.status(), StatusCode::OK);
        let ct = resp.headers()["content-type"].to_str().unwrap();
        assert!(
            ct.contains("text/html"),
            "SPA fallback should serve HTML, got: {ct}"
        );
    }

    #[tokio::test]
    async fn dashboard_router_serves_html_and_config_js() {
        let dash = dashboard_router("http://127.0.0.1:3456");
        let resp = dash
            .clone()
            .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = resp.into_body().collect().await.unwrap().to_bytes();
        assert!(std::str::from_utf8(&body).unwrap().contains("<html"));

        let resp = dash
            .oneshot(
                Request::builder()
                    .uri("/config.js")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(resp.headers()["content-type"], "application/javascript");
        let body = resp.into_body().collect().await.unwrap().to_bytes();
        assert_eq!(
            std::str::from_utf8(&body).unwrap().trim(),
            "window.HIVEMIND_API = \"http://127.0.0.1:3456\";"
        );
    }
}
