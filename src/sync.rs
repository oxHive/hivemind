use std::sync::Arc;
use tokio::sync::Notify;

pub async fn run_sync_loop(
    db: Arc<libsql::Database>,
    interval_secs: u64,
    on_startup: bool,
    trigger: Arc<Notify>,
) {
    if on_startup {
        if let Err(e) = db.sync().await {
            tracing::warn!("initial sync failed: {e:#}");
        }
    }
    let mut ticker =
        tokio::time::interval(tokio::time::Duration::from_secs(interval_secs));
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    ticker.tick().await; // consume the immediate first tick
    loop {
        tokio::select! {
            _ = ticker.tick() => {}
            _ = trigger.notified() => {}
        }
        if let Err(e) = db.sync().await {
            tracing::warn!("sync failed: {e:#}");
        }
    }
}
