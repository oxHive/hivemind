use crate::hive::identity::DeviceIdentity;
use crate::hive::roster::RosterStatus;
use crate::store::SqliteStore;
use std::sync::Arc;
use std::time::Duration;

pub struct PeerStatus {
    pub device_id: String,
    pub public_key: String,
    pub address: Option<String>,
    pub online: bool,
    pub last_synced_at: Option<i64>,
}

/// Peers this device currently believes are reachable, for the push-on-change
/// path (Task 11) to target. Addresses are resolved by mDNS discovery
/// (Plan 1's `HiveDiscovery`), keyed by device_id -- wiring that address
/// resolution into this table is a later task's step, not left elsewhere.
pub async fn online_peers(store: &SqliteStore) -> anyhow::Result<Vec<PeerStatus>> {
    let roster = store.hive_list_roster().await?;
    let mut out = Vec::new();
    for entry in roster.into_iter().filter(|e| e.status == RosterStatus::Active) {
        if let Some(status) = store.hive_get_peer_status(&entry.device_id).await?
            && status.online
        {
            out.push(PeerStatus {
                device_id: entry.device_id,
                public_key: entry.public_key,
                address: None, // filled by a later task's mDNS-address table lookup
                online: true,
                last_synced_at: status.last_synced_at,
            });
        }
    }
    Ok(out)
}

pub async fn run_ping_loop(store: Arc<SqliteStore>, identity: DeviceIdentity, interval_seconds_default: u64) {
    loop {
        let interval_seconds = store
            .hive_settings_override()
            .await
            .ok()
            .flatten()
            .map(|(_, ping_s, _)| ping_s)
            .unwrap_or(interval_seconds_default);
        tokio::time::sleep(Duration::from_secs(interval_seconds)).await;
        ping_once(&store, &identity).await;
    }
}

async fn ping_once(store: &Arc<SqliteStore>, identity: &DeviceIdentity) {
    let Ok(roster) = store.hive_list_roster().await else { return };
    for peer in roster.iter().filter(|e| e.status == RosterStatus::Active && e.device_id != identity.device_id) {
        // Address resolution: this plan's mDNS discovery (Plan 1's
        // `HiveDiscovery::browse`) needs to feed resolved addresses into a
        // device_id -> address map for this ping to actually have somewhere
        // to connect. A later task (Task 13) wires `HiveDiscovery::browse`'s
        // event stream into updating that map; until then this loop can mark
        // peers reachable/unreachable structurally but has no real address to
        // dial. Implement the reachability check itself now (a later task's
        // step assumes a `resolve_address` helper exists); Task 13 supplies it.
        let now = crate::store::chrono_now();
        let Ok(client) = crate::hive::client::HiveClient::new(identity, &peer.public_key) else { continue };
        let _ = client; // real ping call wired in Task 13 once address resolution exists
        let _ = store.hive_upsert_peer_status(&peer.device_id, false, None).await;
        let _ = now;
    }
}
