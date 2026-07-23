use crate::store::{HiveManifest, SqliteStore};
use anyhow::Result;

pub struct PullSummary {
    pub memories_pulled: usize,
    pub tombstones_applied: usize,
    pub conflicts: usize,
}

/// Pure diff/apply core: given a remote manifest and read access to the
/// remote store (a `HiveClient` GET in production, an in-process second
/// `SqliteStore` in this task's tests), figures out what differs and
/// applies it to `local`. Kept free of any HTTP/TLS specifics so it's
/// testable without a live network.
async fn diff_and_apply(
    local: &SqliteStore,
    remote: &SqliteStore,
    remote_manifest: &HiveManifest,
) -> Result<PullSummary> {
    let local_manifest = local.hive_manifest().await?;
    let mut summary = PullSummary { memories_pulled: 0, tombstones_applied: 0, conflicts: 0 };

    for (id, (remote_hash, _remote_updated_at)) in &remote_manifest.memories {
        let differs = match local_manifest.memories.get(id) {
            Some((local_hash, _)) => local_hash != remote_hash,
            None => true,
        };
        if !differs {
            continue;
        }
        let Some(remote_entry) = remote.recall_by_id(id).await? else { continue };
        match local.apply_incoming_memory(&remote_entry, remote_hash).await? {
            crate::store::ApplyOutcome::Applied => summary.memories_pulled += 1,
            crate::store::ApplyOutcome::Conflicted => summary.conflicts += 1,
            crate::store::ApplyOutcome::KeptLocal => {}
        }
    }

    for (id, remote_deleted_at) in &remote_manifest.tombstones {
        if let Some(local_entry) = local.recall_by_id(id).await?
            && local_entry.updated_at < *remote_deleted_at
        {
            local.delete(id).await?;
            summary.tombstones_applied += 1;
        }
    }

    Ok(summary)
}

use crate::hive::client::HiveClient;

pub async fn pull_from_peer(client: &HiveClient, base_url: &str, local: &SqliteStore) -> Result<PullSummary> {
    let manifest_resp = client.get(&format!("{base_url}/api/v1/hive/manifest")).await?;
    let manifest_json: serde_json::Value = manifest_resp.json().await?;

    let mut summary = PullSummary { memories_pulled: 0, tombstones_applied: 0, conflicts: 0 };
    let local_manifest = local.hive_manifest().await?;

    let remote_memories = manifest_json["memories"].as_object().cloned().unwrap_or_default();
    for (id, entry) in &remote_memories {
        let remote_hash = entry[0].as_str().unwrap_or_default();
        let differs = match local_manifest.memories.get(id) {
            Some((local_hash, _)) => local_hash != remote_hash,
            None => true,
        };
        if !differs {
            continue;
        }
        let mem_resp = client.get(&format!("{base_url}/api/v1/hive/memories/{id}")).await?;
        if !mem_resp.status().is_success() {
            continue; // peer no longer has it or a transient error -- skip, next round retries
        }
        let mem_json: serde_json::Value = mem_resp.json().await?;
        let remote_entry = crate::store::MemoryEntry {
            id: id.clone(),
            title: mem_json["title"].as_str().unwrap_or_default().to_string(),
            content: mem_json["content"].as_str().unwrap_or_default().to_string(),
            tags: mem_json["tags"].as_array().map(|a| a.iter().filter_map(|t| t.as_str().map(String::from)).collect()).unwrap_or_default(),
            created_at: mem_json["updated_at"].as_i64().unwrap_or_default(),
            updated_at: mem_json["updated_at"].as_i64().unwrap_or_default(),
            token_count: None,
            layer: mem_json["layer"].as_str().unwrap_or("workspace").to_string(),
            memory_type: mem_json["memory_type"].as_str().unwrap_or("project").to_string(),
        };
        match local.apply_incoming_memory(&remote_entry, remote_hash).await? {
            crate::store::ApplyOutcome::Applied => summary.memories_pulled += 1,
            crate::store::ApplyOutcome::Conflicted => summary.conflicts += 1,
            crate::store::ApplyOutcome::KeptLocal => {}
        }
    }

    let remote_tombstones = manifest_json["tombstones"].as_object().cloned().unwrap_or_default();
    for (id, deleted_at) in &remote_tombstones {
        let deleted_at = deleted_at.as_i64().unwrap_or_default();
        if let Some(local_entry) = local.recall_by_id(id).await?
            && local_entry.updated_at < deleted_at
        {
            local.delete(id).await?;
            summary.tombstones_applied += 1;
        }
    }

    Ok(summary)
}

use crate::hive::identity::DeviceIdentity;
use std::sync::Arc;
use std::time::Duration;

pub async fn run_sync_loop(store: Arc<SqliteStore>, identity: DeviceIdentity, interval_seconds_default: u64) {
    loop {
        let interval_seconds = store
            .hive_settings_override()
            .await
            .ok()
            .flatten()
            .map(|(sync_s, _, _)| sync_s)
            .unwrap_or(interval_seconds_default);
        tokio::time::sleep(Duration::from_secs(interval_seconds)).await;
        sync_once(&store, &identity).await;
    }
}

async fn sync_once(store: &Arc<SqliteStore>, identity: &DeviceIdentity) {
    let Ok(roster) = store.hive_list_roster().await else { return };
    for peer in roster.iter().filter(|e| e.status == crate::hive::roster::RosterStatus::Active && e.device_id != identity.device_id) {
        let Ok(client) = crate::hive::client::HiveClient::new(identity, &peer.public_key) else { continue };
        // Peer address resolution (mDNS-discovered IP:port per device_id) is
        // Task 12's job (the presence loop maintains reachable addresses) --
        // this loop assumes a `peer_address` lookup exists by the time both
        // tasks are wired together in Task 13. Call
        // `crate::hive::peer_status::resolve_address(&peer.device_id)` here
        // once Task 12 lands; until then, this function is structurally
        // complete but not yet reachable from `run_up` (Task 13 wires it in).
        let _ = (&client, peer); // placeholder use to avoid an unused-var warning until Task 13 wires in the real address lookup
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::{compute_hive_content_hash, NewMemoryRow};

    async fn test_store() -> crate::store::SqliteStore {
        let sync = crate::config::SyncSettings::default();
        let database = crate::db::open_database(&sync, ":memory:").await.unwrap();
        let conn = database.connect().unwrap();
        crate::db::run_migrations(&conn).await.unwrap();
        crate::store::SqliteStore::new(conn)
    }

    #[tokio::test]
    async fn diff_and_apply_pulls_a_memory_missing_locally() {
        let local_store = test_store().await;
        let remote_store = test_store().await;
        remote_store
            .store(&NewMemoryRow {
                id: "mem_pulltest0000000000000000001",
                title: "remote-only", content: "c", tags: &[], token_count: None,
                layer: "workspace", memory_type: "project",
            })
            .await
            .unwrap();
        let remote_manifest = remote_store.hive_manifest().await.unwrap();

        let summary = diff_and_apply(&local_store, &remote_store, &remote_manifest).await.unwrap();
        assert_eq!(summary.memories_pulled, 1);
        assert!(local_store.recall_by_id("mem_pulltest0000000000000000001").await.unwrap().is_some());
    }

    #[tokio::test]
    async fn diff_and_apply_applies_a_tombstone_for_an_older_local_copy() {
        let local_store = test_store().await;
        let remote_store = test_store().await;
        local_store
            .store(&NewMemoryRow {
                id: "mem_pulltest0000000000000000002",
                title: "will be deleted", content: "c", tags: &[], token_count: None,
                layer: "workspace", memory_type: "project",
            })
            .await
            .unwrap();
        remote_store
            .store(&NewMemoryRow {
                id: "mem_pulltest0000000000000000002",
                title: "will be deleted", content: "c", tags: &[], token_count: None,
                layer: "workspace", memory_type: "project",
            })
            .await
            .unwrap();
        // `chrono_now()` (store.rs) is second-resolution, and this test's
        // tombstone check is a strict `<` on those timestamps: without a
        // gap, storing local + remote + deleting remote can all land in the
        // same wall-clock second, making local_entry.updated_at ==
        // remote_deleted_at and failing the `<` comparison intermittently.
        // Force a real gap so the assertion is deterministic.
        tokio::time::sleep(std::time::Duration::from_millis(1100)).await;
        remote_store.delete("mem_pulltest0000000000000000002").await.unwrap();
        let remote_manifest = remote_store.hive_manifest().await.unwrap();

        let summary = diff_and_apply(&local_store, &remote_store, &remote_manifest).await.unwrap();
        assert_eq!(summary.tombstones_applied, 1);
        assert!(local_store.recall_by_id("mem_pulltest0000000000000000002").await.unwrap().is_none());
    }
}
