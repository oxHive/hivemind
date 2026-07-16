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
        Some(Command::Status { plain }) => cli::cmd_status(plain),
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
        Some(Command::Migrate) => cli::cmd_migrate(),
        Some(Command::SessionStart { json }) => cli::cmd_session_start(json),
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
    let settings =
        config::load_server_settings(&config::global_config_path()).unwrap_or_else(|e| {
            tracing::warn!("could not load global config ({e:#}); using defaults");
            config::ServerSettings {
                host: "127.0.0.1".into(),
                port: 3456,
                dashboard_port: 3457,
                api_url: "http://127.0.0.1:3456".into(),
                cors_origin: "http://127.0.0.1:3457".into(),
                sync: config::SyncSettings::default(),
                agent: config::AgentSettings::default(),
            }
        });
    let (store, database) = open_store(&settings.sync).await?;
    // Holds the DB handle so it lives past `server.waiting()` when no sync loop owns it.
    let mut _db_guard: Option<libsql::Database> = None;
    let service = if settings.sync.enabled {
        let trigger = Arc::new(Notify::new());
        tokio::spawn(sync::run_sync_loop(
            Arc::new(database),
            store.clone(),
            settings.sync.interval_seconds,
            settings.sync.sync_on_startup,
            trigger.clone(),
        ));
        if settings.sync.sync_on_store {
            HiveMind::with_sync(store, trigger)
        } else {
            HiveMind::with_store(store)
        }
    } else {
        _db_guard = Some(database);
        HiveMind::with_store(store)
    };

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

    // Holds the DB handle so it lives past `http::run_up` when no sync loop owns it.
    let mut _db_guard: Option<libsql::Database> = None;
    let mut notify_on_store = None;
    if settings.sync.enabled {
        let trigger = Arc::new(Notify::new());
        tokio::spawn(sync::run_sync_loop(
            Arc::new(database),
            store.clone(),
            settings.sync.interval_seconds,
            settings.sync.sync_on_startup,
            trigger.clone(),
        ));
        if settings.sync.sync_on_store {
            notify_on_store = Some(trigger);
        }
    } else {
        _db_guard = Some(database);
    }

    http::run_up(store, &settings, headless, notify_on_store).await
}

#[tokio::main]
async fn run_dashboard(open: bool) -> Result<()> {
    init_tracing();
    let settings = config::load_server_settings(&config::global_config_path())?;
    http::run_dashboard(&settings, open).await
}
