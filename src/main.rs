use anyhow::Result;
use clap::Parser;
use oxhivemind::cli::{self, Cli, Command, McpAction, ServiceAction};
use oxhivemind::{config, db, http, server, store};
use rmcp::ServiceExt;
use server::HiveMind;
use std::sync::Arc;
use store::SqliteStore;

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        None => run_server(),
        Some(Command::Init) => cli::cmd_init(),
        Some(Command::Status) => cli::cmd_status(),
        Some(Command::Up { headless }) => run_up(headless),
        Some(Command::Dashboard { open }) => run_dashboard(open),
        Some(Command::Mcp { action }) => match action {
            McpAction::Install { client } => cli::cmd_mcp_install(&client),
        },
        Some(Command::Service { action }) => match action {
            ServiceAction::Install => cli::cmd_service_install(),
            ServiceAction::Uninstall => cli::cmd_service_uninstall(),
            ServiceAction::Status => cli::cmd_service_status(),
        },
    }
}

fn init_tracing() {
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "hivemind=info".into()),
        )
        .init();
}

fn open_store() -> Result<Arc<SqliteStore>> {
    let db_path = db::resolve_db_path();
    tracing::info!("opening database at {db_path}");
    let conn = db::open(&db_path)?;
    Ok(Arc::new(SqliteStore::new(conn)))
}

#[tokio::main]
async fn run_server() -> Result<()> {
    init_tracing();
    let store = open_store()?;
    let service = HiveMind::with_store(store);

    tracing::info!("HiveMind MCP server starting on stdio");
    let server = service
        .serve((tokio::io::stdin(), tokio::io::stdout()))
        .await?;
    server.waiting().await?;
    Ok(())
}

#[tokio::main]
async fn run_up(headless: bool) -> Result<()> {
    init_tracing();
    let settings = config::load_server_settings(&config::global_config_path())?;
    let store = open_store()?;
    http::run_up(store, &settings, headless).await
}

#[tokio::main]
async fn run_dashboard(open: bool) -> Result<()> {
    init_tracing();
    let settings = config::load_server_settings(&config::global_config_path())?;
    http::run_dashboard(&settings, open).await
}
