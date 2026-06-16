use std::sync::Arc;
use anyhow::Result;
use crate::{config::SyncSettings, model::MemoryEntry, store::SqliteStore};

pub struct SyncClient {
    remote_url: String,
    api_key: String,
    store: Arc<SqliteStore>,
    http: reqwest::Client,
}

#[allow(dead_code)]
pub struct SyncReport {
    pub pushed: usize,
    pub pulled: usize,
    pub conflicts: usize,
}

impl SyncClient {
    pub fn new(settings: &SyncSettings, store: Arc<SqliteStore>) -> Self {
        SyncClient {
            remote_url: settings.remote_url.trim_end_matches('/').to_string(),
            api_key: settings.api_key.clone(),
            store,
            http: reqwest::Client::new(),
        }
    }

    pub async fn sync_once(&self) -> Result<SyncReport> {
        let last_synced_at = self.store.get_kv("last_synced_at")?
            .and_then(|v| v.parse::<i64>().ok())
            .unwrap_or(0);

        // Push local changes to remote
        let local_records = self.store.memories_since(last_synced_at)?;
        let pushed = local_records.len();
        let push_resp: serde_json::Value = self.http
            .post(format!("{}/api/sync/push", self.remote_url))
            .bearer_auth(&self.api_key)
            .json(&serde_json::json!({ "records": local_records, "client_id": "local" }))
            .send().await?
            .error_for_status()?
            .json().await?;
        let push_conflicts = push_resp["conflicts"].as_array().map(|a| a.len()).unwrap_or(0);

        // Pull remote changes
        let pull_resp: serde_json::Value = self.http
            .get(format!("{}/api/sync/pull?since={last_synced_at}", self.remote_url))
            .bearer_auth(&self.api_key)
            .send().await?
            .error_for_status()?
            .json().await?;

        let mut pulled = 0usize;
        let mut pull_conflicts = 0usize;
        if let Some(records) = pull_resp["records"].as_array() {
            for val in records {
                if let Ok(entry) = serde_json::from_value::<MemoryEntry>(val.clone()) {
                    if self.store.upsert_memory(&entry)?.is_some() {
                        pull_conflicts += 1;
                    }
                    pulled += 1;
                }
            }
        }

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?.as_secs() as i64;
        self.store.set_kv("last_synced_at", &now.to_string())?;

        tracing::info!("sync: pushed={pushed} pulled={pulled} conflicts={}", push_conflicts + pull_conflicts);
        Ok(SyncReport { pushed, pulled, conflicts: push_conflicts + pull_conflicts })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use crate::{config::SyncSettings, db, store::SqliteStore};

    fn test_store() -> Arc<SqliteStore> {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys=ON;").unwrap();
        db::create_schema(&conn).unwrap();
        Arc::new(SqliteStore::new(conn))
    }

    #[test]
    fn sync_client_new_trims_trailing_slash() {
        let settings = SyncSettings {
            enabled: true,
            remote_url: "http://pi.local:3456/".to_string(),
            api_key: "secret".to_string(),
            interval_seconds: 60,
            sync_on_store: true,
            sync_on_startup: false,
        };
        let client = SyncClient::new(&settings, test_store());
        assert_eq!(client.remote_url, "http://pi.local:3456");
        assert_eq!(client.api_key, "secret");
    }

    #[test]
    fn sync_client_new_with_no_trailing_slash() {
        let settings = SyncSettings {
            enabled: true,
            remote_url: "http://pi.local:3456".to_string(),
            api_key: "tok".to_string(),
            interval_seconds: 300,
            sync_on_store: false,
            sync_on_startup: true,
        };
        let client = SyncClient::new(&settings, test_store());
        assert_eq!(client.remote_url, "http://pi.local:3456");
    }

    #[test]
    fn sync_report_fields_are_accessible() {
        let r = SyncReport { pushed: 3, pulled: 1, conflicts: 0 };
        assert_eq!(r.pushed, 3);
        assert_eq!(r.pulled, 1);
        assert_eq!(r.conflicts, 0);
    }
}

pub async fn run_sync_loop(
    client: Arc<SyncClient>,
    interval_secs: u64,
    sync_on_startup: bool,
    trigger: Arc<tokio::sync::Notify>,
) {
    if sync_on_startup && let Err(e) = client.sync_once().await {
        tracing::warn!("initial sync failed: {e:#}");
    }
    let mut ticker = tokio::time::interval(tokio::time::Duration::from_secs(interval_secs));
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    ticker.tick().await; // consume the immediate first tick
    loop {
        tokio::select! {
            _ = ticker.tick() => {},
            _ = trigger.notified() => {},
        }
        if let Err(e) = client.sync_once().await {
            tracing::warn!("sync failed: {e:#}");
        }
    }
}
