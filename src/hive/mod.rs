pub mod cert;
pub mod client;
pub mod discovery;
pub mod gossip;
pub mod identity;
pub mod keyring_store;
pub mod pairing;
pub mod roster;
pub mod tls_verify;

/// Loads this device's persisted Ed25519 identity, or generates and
/// persists a fresh one on first run. The device_id is stored in the
/// store's `_meta` table (same pattern as `tag_namespaces`); the private
/// key lives in the OS keyring, addressed by that same device_id. Both must
/// be present and consistent for the persisted identity to be reused —
/// either one missing or unreadable falls through to generating (and
/// persisting) a brand new identity.
pub async fn bootstrap_self_identity(
    store: &crate::store::SqliteStore,
    key_store: &dyn keyring_store::HiveKeyStore,
) -> anyhow::Result<identity::DeviceIdentity> {
    if let Some(device_id) = store.get_meta("hive_device_id").await? {
        if let Some(signing_key_hex) = key_store.load(&device_id)?
            && let Some(existing) = identity::from_signing_key_hex(&signing_key_hex)
        {
            return Ok(existing);
        }
        tracing::warn!(
            "hive_device_id {device_id} present but its keyring entry is missing \
             or invalid; generating a new device identity"
        );
    }
    let fresh = identity::generate();
    key_store.save(&fresh.device_id, &hex::encode(fresh.signing_key.to_bytes()))?;
    store.set_meta("hive_device_id", &fresh.device_id).await?;
    Ok(fresh)
}

#[cfg(test)]
mod bootstrap_tests {
    use super::*;
    use keyring_store::{FakeHiveKeyStore, HiveKeyStore};

    #[tokio::test]
    async fn bootstrap_generates_and_persists_on_first_run() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.db");
        let sync = crate::config::SyncSettings::default();
        let database = crate::db::open_database(&sync, path.to_str().unwrap())
            .await
            .unwrap();
        let conn = database.connect().unwrap();
        crate::db::run_migrations(&conn).await.unwrap();
        let store = crate::store::SqliteStore::new(conn);
        let key_store = FakeHiveKeyStore::new();

        let identity = bootstrap_self_identity(&store, &key_store).await.unwrap();
        assert!(identity.device_id.starts_with("hive_"));
        assert_eq!(
            store.get_meta("hive_device_id").await.unwrap(),
            Some(identity.device_id.clone())
        );
        assert!(key_store.load(&identity.device_id).unwrap().is_some());
    }

    #[tokio::test]
    async fn bootstrap_reuses_persisted_identity_on_second_call() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.db");
        let sync = crate::config::SyncSettings::default();
        let database = crate::db::open_database(&sync, path.to_str().unwrap())
            .await
            .unwrap();
        let conn = database.connect().unwrap();
        crate::db::run_migrations(&conn).await.unwrap();
        let store = crate::store::SqliteStore::new(conn);
        let key_store = FakeHiveKeyStore::new();

        let first = bootstrap_self_identity(&store, &key_store).await.unwrap();
        let second = bootstrap_self_identity(&store, &key_store).await.unwrap();
        assert_eq!(first.device_id, second.device_id);
        assert_eq!(
            identity::public_key_hex(&first),
            identity::public_key_hex(&second)
        );
    }
}
