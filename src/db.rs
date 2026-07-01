use anyhow::{Context, Result};
use libsql::Builder;

use crate::config::SyncSettings;

/// XDG data dir: $XDG_DATA_HOME/hivemind or ~/.local/share/hivemind
pub fn xdg_data_dir() -> std::path::PathBuf {
    if let Ok(xdg) = std::env::var("XDG_DATA_HOME") {
        return std::path::PathBuf::from(xdg).join("hivemind");
    }
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    std::path::PathBuf::from(home)
        .join(".local")
        .join("share")
        .join("hivemind")
}

/// Legacy path used before XDG migration: ~/.hivemind/memories.db
pub fn legacy_db_path() -> std::path::PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    std::path::PathBuf::from(home)
        .join(".hivemind")
        .join("memories.db")
}

pub fn resolve_db_path() -> String {
    if let Ok(p) = std::env::var("HIVEMIND_DB_PATH") {
        return p;
    }
    xdg_data_dir()
        .join("memories.db")
        .to_string_lossy()
        .into_owned()
}

pub async fn open_database(sync: &SyncSettings, path: &str) -> Result<libsql::Database> {
    if let Some(dir) = std::path::Path::new(path).parent() {
        tokio::fs::create_dir_all(dir).await?;
    }

    if sync.enabled {
        let url = if sync.remote_url.is_empty() {
            anyhow::bail!("sync.remote_url required when sync.enabled = true");
        } else {
            &sync.remote_url
        };
        let token = sync.api_key.as_str();
        let db = Builder::new_remote_replica(path, url.to_string(), token.to_string())
            .build()
            .await
            .context("failed to open remote replica database")?;
        Ok(db)
    } else {
        let db = Builder::new_local(path)
            .build()
            .await
            .context("failed to open local database")?;
        Ok(db)
    }
}

/// Set WAL mode and enable foreign-key enforcement on a connection.
/// Must be called on every connection obtained from `database.connect()`.
pub async fn init_connection(conn: &libsql::Connection) -> Result<()> {
    conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")
        .await?;
    Ok(())
}

pub async fn run_migrations(conn: &libsql::Connection) -> Result<()> {
    // Enable WAL mode and foreign-key enforcement before any DDL runs.
    init_connection(conn).await?;

    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS _migrations (name TEXT PRIMARY KEY, applied_at INTEGER NOT NULL);",
    )
    .await?;

    let memories_exists: i64 = {
        let mut rows = conn
            .query(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='memories'",
                (),
            )
            .await?;
        rows.next().await?.unwrap().get(0)?
    };

    let v1_applied: i64 = {
        let mut rows = conn
            .query(
                "SELECT COUNT(*) FROM _migrations WHERE name='V1__initial_schema'",
                (),
            )
            .await?;
        rows.next().await?.unwrap().get(0)?
    };

    if v1_applied == 0 {
        if memories_exists == 0 {
            conn.execute_batch(include_str!("../migrations/V1__initial_schema.sql"))
                .await
                .context("failed to apply V1 schema migration")?;
        }
        conn.execute(
            "INSERT INTO _migrations (name, applied_at) VALUES ('V1__initial_schema', unixepoch())",
            (),
        )
        .await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn xdg_data_dir_uses_xdg_env_when_set() {
        let dir = tempfile::tempdir().unwrap();
        // SAFETY: test-only env mutation; tests in this module run serially via cargo test.
        unsafe { std::env::set_var("XDG_DATA_HOME", dir.path()) };
        let result = xdg_data_dir();
        unsafe { std::env::remove_var("XDG_DATA_HOME") };
        assert_eq!(result, dir.path().join("hivemind"));
    }

    #[test]
    fn xdg_data_dir_falls_back_to_local_share() {
        unsafe { std::env::remove_var("XDG_DATA_HOME") };
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
        let result = xdg_data_dir();
        assert_eq!(
            result,
            std::path::PathBuf::from(&home)
                .join(".local")
                .join("share")
                .join("hivemind")
        );
    }

    #[test]
    fn legacy_db_path_is_under_dot_hivemind() {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
        let result = legacy_db_path();
        assert_eq!(
            result,
            std::path::PathBuf::from(&home)
                .join(".hivemind")
                .join("memories.db")
        );
    }

    #[test]
    fn resolve_db_path_respects_env_override() {
        unsafe { std::env::set_var("HIVEMIND_DB_PATH", "/custom/path/db.sqlite") };
        let result = resolve_db_path();
        unsafe { std::env::remove_var("HIVEMIND_DB_PATH") };
        assert_eq!(result, "/custom/path/db.sqlite");
    }

    #[test]
    fn resolve_db_path_default_ends_with_memories_db() {
        unsafe { std::env::remove_var("HIVEMIND_DB_PATH") };
        let result = resolve_db_path();
        assert!(result.ends_with("memories.db"), "got: {result}");
        assert!(result.contains("hivemind"), "got: {result}");
    }

    #[tokio::test]
    async fn open_database_fails_when_sync_enabled_without_url() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.db");
        let sync = crate::config::SyncSettings {
            enabled: true,
            remote_url: String::new(),
            api_key: String::new(),
            interval_seconds: 300,
            sync_on_store: false,
            sync_on_startup: false,
        };
        let result = open_database(&sync, path.to_str().unwrap()).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("remote_url"));
    }
}
