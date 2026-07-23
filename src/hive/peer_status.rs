use crate::hive::identity::DeviceIdentity;
use crate::hive::roster::RosterStatus;
use crate::store::SqliteStore;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::Mutex;
use std::time::Duration;

/// Process-wide map of device_id -> last-seen "ip:port" from mDNS discovery.
/// A `Mutex<HashMap<..>>` behind a `once_cell`-style static is the simplest
/// way to share this between the mDNS browse task (which only writes to it)
/// and the ping/sync loops (which only read from it) without threading a
/// new parameter through every function that might need an address.
static DISCOVERED_ADDRESSES: std::sync::OnceLock<Mutex<HashMap<String, String>>> = std::sync::OnceLock::new();

fn discovered_addresses() -> &'static Mutex<HashMap<String, String>> {
    DISCOVERED_ADDRESSES.get_or_init(|| Mutex::new(HashMap::new()))
}

pub fn record_discovered_address(device_id: &str, address: String) {
    discovered_addresses().lock().unwrap().insert(device_id.to_string(), address);
}

pub fn resolve_address(device_id: &str) -> Option<String> {
    discovered_addresses().lock().unwrap().get(device_id).cloned()
}

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

pub async fn run_ping_loop(
    store: Arc<SqliteStore>,
    identity: DeviceIdentity,
    interval_seconds_default: u64,
    sync_tls_config: axum_server::tls_rustls::RustlsConfig,
    rebuild_sync_server_config: impl Fn(Vec<crate::hive::roster::RosterEntry>) -> anyhow::Result<rustls::ServerConfig>
        + Send
        + Sync
        + 'static,
) {
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

        // `ping_once` above already re-merged every reachable peer's roster
        // view (gossip refresh, including revocations). Rebuild the sync
        // listener's client-cert verifier from the now-current roster and
        // hot-swap it in (no rebind), so a revoked device stops being
        // accepted, and a newly-paired device starts being accepted, within
        // one ping interval instead of requiring a process restart.
        if let Ok(current_roster) = store.hive_list_roster().await {
            match rebuild_sync_server_config(current_roster) {
                Ok(new_config) => {
                    sync_tls_config.reload_from_config(std::sync::Arc::new(new_config))
                }
                Err(e) => tracing::warn!("failed to rebuild hive sync TLS config: {e:#}"),
            }
        }
    }
}

async fn ping_once(store: &Arc<SqliteStore>, identity: &DeviceIdentity) {
    let Ok(roster) = store.hive_list_roster().await else { return };
    for peer in roster.iter().filter(|e| e.status == RosterStatus::Active && e.device_id != identity.device_id) {
        let Some(address) = resolve_address(&peer.device_id) else {
            let _ = store.hive_upsert_peer_status(&peer.device_id, false, None).await;
            continue;
        };
        let Ok(client) = crate::hive::client::HiveClient::new(identity, &peer.public_key) else { continue };
        let now = crate::store::chrono_now();
        match client.get(&format!("https://{address}/api/v1/hive/roster")).await {
            Ok(resp) if resp.status().is_success() => {
                let _ = store.hive_upsert_peer_status(&peer.device_id, true, Some(now)).await;
                if let Ok(body) = resp.json::<serde_json::Value>().await
                    && let Some(remote_roster_json) = body["roster"].as_array()
                {
                    // Re-merge this peer's roster view, continuing Plan 1's
                    // gossip propagation (including revocations) on the same
                    // round-trip as the liveness check.
                    if let Ok(remote_roster) = serde_json::from_value::<Vec<crate::hive::roster::RosterEntry>>(
                        serde_json::Value::Array(remote_roster_json.clone()),
                    ) {
                        let local_roster = store.hive_list_roster().await.unwrap_or_default();
                        let merged = crate::hive::gossip::merge_roster(local_roster, remote_roster);
                        for entry in &merged {
                            let _ = store.hive_upsert_roster_entry(entry).await;
                        }
                    }
                }
            }
            _ => {
                let _ = store.hive_upsert_peer_status(&peer.device_id, false, None).await;
            }
        }
    }
}
