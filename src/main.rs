use anyhow::Result;
use clap::Parser;
use oxhivemind::cli::{self, Cli, Command, McpAction, ServiceAction};
use oxhivemind::{config, db, http, server, store, sync};
use rmcp::ServiceExt;
use server::HiveMind;
use std::sync::Arc;
use store::SqliteStore;
use tokio::sync::Notify;

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
        Some(Command::Version) => cli::cmd_version(),
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

async fn open_store(
    sync_settings: &config::SyncSettings,
) -> Result<(Arc<SqliteStore>, libsql::Database)> {
    let db_path = db::resolve_db_path();
    tracing::info!("opening database at {db_path}");
    let database = db::open_database(sync_settings, &db_path).await?;
    let conn = database.connect()?;
    db::run_migrations(&conn).await?;
    let store = Arc::new(SqliteStore::new(conn));
    Ok((store, database))
}

#[tokio::main]
async fn run_server() -> Result<()> {
    init_tracing();
    let sync_settings = config::SyncSettings::default();
    let (store, _db) = open_store(&sync_settings).await?;
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
    cli::warn_if_not_initialized();
    init_tracing();
    let settings = config::load_server_settings(&config::global_config_path())?;
    let (store, database) = open_store(&settings.sync).await?;

    if settings.sync.enabled {
        let db_arc = Arc::new(database);
        let trigger = Arc::new(Notify::new());
        let interval = settings.sync.interval_seconds;
        let on_startup = settings.sync.sync_on_startup;
        tokio::spawn(sync::run_sync_loop(db_arc, interval, on_startup, trigger));
    }

    http::run_up(store, &settings, headless).await
}

#[tokio::main]
async fn run_dashboard(open: bool) -> Result<()> {
    init_tracing();
    let settings = config::load_server_settings(&config::global_config_path())?;
    http::run_dashboard(&settings, open).await
}
