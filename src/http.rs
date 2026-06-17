use crate::{
    api,
    config::{ServerSettings, SyncSettings},
    server::HiveMind,
    store::SqliteStore,
    sync as syncer,
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

pub fn app_router(
    store: Arc<SqliteStore>,
    sync: SyncSettings,
    trigger: Arc<tokio::sync::Notify>,
) -> Router {
    let mcp = StreamableHttpService::new(
        {
            let store = store.clone();
            let trigger = trigger.clone();
            move || Ok(HiveMind::with_sync(store.clone(), trigger.clone()))
        },
        Arc::new(LocalSessionManager::default()),
        Default::default(),
    );
    api::router(store, sync).nest_service("/mcp", mcp)
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

pub async fn run_up(
    store: Arc<SqliteStore>,
    settings: &ServerSettings,
    headless: bool,
) -> Result<()> {
    let trigger = Arc::new(tokio::sync::Notify::new());
    let app = app_router(store.clone(), settings.sync.clone(), trigger.clone());

    if settings.sync.enabled {
        let client = Arc::new(syncer::SyncClient::new(&settings.sync, store.clone()));
        let interval = settings.sync.interval_seconds;
        let on_startup = settings.sync.sync_on_startup;
        let t = trigger.clone();
        tokio::spawn(async move {
            syncer::run_sync_loop(client, interval, on_startup, t).await;
        });
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
    use crate::{db, store::SqliteStore};
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use http_body_util::BodyExt;
    use std::sync::Arc;
    use tower::ServiceExt;

    fn test_store() -> Arc<SqliteStore> {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys=ON;").unwrap();
        db::create_schema(&conn).unwrap();
        Arc::new(SqliteStore::new(conn))
    }

    #[tokio::test]
    async fn app_router_serves_rest_api() {
        let app = app_router(
            test_store(),
            crate::config::SyncSettings::default(),
            Arc::new(tokio::sync::Notify::new()),
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
