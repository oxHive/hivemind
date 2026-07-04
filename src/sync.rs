use crate::store::SqliteStore;
use std::sync::Arc;
use tokio::sync::Notify;

async fn sync_once(db: &libsql::Database, store: &SqliteStore) {
    let journal = match store.take_journal().await {
        Ok(j) => j,
        Err(e) => {
            tracing::warn!("could not read sync journal: {e:#}");
            Vec::new()
        }
    };
    if let Err(e) = db.sync().await {
        tracing::warn!("sync failed: {e:#}");
        return; // journal rows stay for the next attempt
    }
    match store.detect_conflicts(&journal).await {
        Ok(0) => {}
        Ok(n) => tracing::warn!("{n} sync conflict(s) recorded; review them in the dashboard"),
        Err(e) => tracing::warn!("conflict detection failed: {e:#}"),
    }
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    if let Err(e) = store.set_meta("last_synced_at", &now.to_string()).await {
        tracing::warn!("could not record last_synced_at: {e:#}");
    }
}

pub async fn run_sync_loop(
    db: Arc<libsql::Database>,
    store: Arc<SqliteStore>,
    interval_secs: u64,
    on_startup: bool,
    trigger: Arc<Notify>,
) {
    if on_startup {
        sync_once(&db, &store).await;
    }
    let mut ticker = tokio::time::interval(tokio::time::Duration::from_secs(interval_secs));
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    ticker.tick().await; // consume the immediate first tick
    loop {
        tokio::select! {
            _ = ticker.tick() => {}
            _ = trigger.notified() => {}
        }
        sync_once(&db, &store).await;
    }
}
