mod api;
mod budget;
mod cli;
mod config;
mod db;
mod model;
mod server;
mod session;
mod store;

use anyhow::Result;
use clap::Parser;
use cli::{Cli, Command};
use rmcp::ServiceExt;
use server::HiveMind;
use store::SqliteStore;

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        None => run_server(),
        Some(Command::Init) => cli::cmd_init(),
        Some(Command::Status) => cli::cmd_status(),
    }
}

#[tokio::main]
async fn run_server() -> Result<()> {
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "hivemind=info".into()),
        )
        .init();

    let db_path = db::resolve_db_path();
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
