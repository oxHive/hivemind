mod db;
mod model;
mod server;
mod store;

use anyhow::Result;
use rmcp::ServiceExt;
use server::HiveMind;
use store::SqliteStore;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "hivemind=info".into()),
        )
        .init();

    let db_path = resolve_db_path();
    tracing::info!("opening database at {db_path}");

    let conn = db::open(&db_path)?;
    let service = HiveMind::new(SqliteStore::new(conn));

    tracing::info!("HiveMind MCP server starting on stdio");
    let server = service
        .serve((tokio::io::stdin(), tokio::io::stdout()))
        .await?;
    server.waiting().await?;
    Ok(())
}

fn resolve_db_path() -> String {
    if let Ok(path) = std::env::var("HIVEMIND_DB_PATH") {
        return path;
    }
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    let dir = format!("{home}/.local/share/hivemind");
    std::fs::create_dir_all(&dir).ok();
    format!("{dir}/memory.db")
}
