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
