use super::*;
use crate::hive::gossip::merge_roster;
use crate::hive::pairing::PairingCodeStore;
use crate::hive::roster::{JoinRecord, RosterEntry, RosterStatus, verify_join_record};
use std::sync::Arc;

#[derive(Deserialize)]
pub(super) struct PairRequestBody {
    code: String,
    join_record: JoinRecord,
}

pub(super) async fn hive_pair(
    State(store): State<Store>,
    Extension(pairing_codes): Extension<Arc<PairingCodeStore>>,
    Json(body): Json<PairRequestBody>,
) -> Result<Json<Value>, ApiError> {
    let now = chrono::Utc::now().timestamp();
    if !pairing_codes.validate_and_consume(&body.code, now) {
        return Err(ApiError(
            StatusCode::UNAUTHORIZED,
            "invalid or expired pairing code".to_string(),
        ));
    }
    if !verify_join_record(&body.join_record) {
        return Err(ApiError(
            StatusCode::UNPROCESSABLE_ENTITY,
            "join record signature invalid".to_string(),
        ));
    }

    let local_roster = store.hive_list_roster().await?;
    let new_entry = RosterEntry {
        device_id: body.join_record.device_id.clone(),
        public_key: body.join_record.public_key.clone(),
        name: body.join_record.name.clone(),
        status: RosterStatus::Active,
        joined_at: body.join_record.joined_at,
        revoked_at: None,
        revoked_by: None,
        join_record: body.join_record.clone(),
        revocation_record: None,
    };
    let merged = merge_roster(local_roster, vec![new_entry]);
    for entry in &merged {
        store.hive_upsert_roster_entry(entry).await?;
    }

    Ok(Json(json!({ "roster": merged })))
}

#[derive(Deserialize)]
pub(super) struct JoinHiveBody {
    peer_address: String,
    pairing_code: String,
    // Obtained out-of-band alongside the pairing code itself (e.g. shown next
    // to the code on the peer's own screen) -- see issue #26. Pinning the
    // pairing TLS connection to this key, rather than accepting any
    // certificate, closes the on-path MITM gap in the original blind-trust
    // bootstrap: an attacker impersonating the peer during pairing now fails
    // the TLS handshake instead of being silently trusted.
    peer_public_key: String,
}

pub(super) async fn hive_join(
    State(store): State<Store>,
    Extension(identity): Extension<Arc<crate::hive::identity::DeviceIdentity>>,
    Json(body): Json<JoinHiveBody>,
) -> Result<Json<Value>, ApiError> {
    let join_record = crate::hive::roster::create_join_record(&identity, &identity.device_id, chrono::Utc::now().timestamp());
    let pair_body = json!({
        "code": body.pairing_code,
        "join_record": {
            "device_id": join_record.device_id, "public_key": join_record.public_key,
            "name": join_record.name, "joined_at": join_record.joined_at, "signature": join_record.signature,
        }
    });

    let target_public_key: [u8; 32] = hex::decode(&body.peer_public_key)
        .ok()
        .and_then(|bytes| bytes.try_into().ok())
        .ok_or_else(|| {
            ApiError(
                StatusCode::UNPROCESSABLE_ENTITY,
                "peer_public_key must be 32 bytes of hex".to_string(),
            )
        })?;
    let verifier = std::sync::Arc::new(crate::hive::tls_verify::PinnedServerCertVerifier::new(
        target_public_key,
    ));
    // Pinned, not blind-trust: the pairing endpoint itself needs no client
    // cert (its server-TLS-only listener uses `with_no_client_auth()`), but
    // the server cert it presents must match the expected peer's key exactly.
    let tls_config = rustls::ClientConfig::builder_with_provider(std::sync::Arc::new(
        rustls::crypto::ring::default_provider(),
    ))
    .with_protocol_versions(rustls::DEFAULT_VERSIONS)
    .map_err(|e| ApiError(StatusCode::INTERNAL_SERVER_ERROR, format!("failed to select TLS protocol versions: {e}")))?
    .dangerous()
    .with_custom_certificate_verifier(verifier)
    .with_no_client_auth();
    let pinned_client = reqwest::Client::builder()
        .use_preconfigured_tls(tls_config)
        .build()
        .map_err(|e| ApiError(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let resp = pinned_client
        .post(format!("https://{}/api/v1/hive/pair", body.peer_address))
        .json(&pair_body)
        .send()
        .await
        .map_err(|e| ApiError(StatusCode::BAD_GATEWAY, format!("could not reach peer: {e}")))?;
    if !resp.status().is_success() {
        return Err(ApiError(StatusCode::BAD_GATEWAY, "peer rejected the pairing code".to_string()));
    }
    let pair_response: Value = resp.json().await.map_err(|e| ApiError(StatusCode::BAD_GATEWAY, e.to_string()))?;
    let remote_roster: Vec<crate::hive::roster::RosterEntry> =
        serde_json::from_value(pair_response["roster"].clone())
            .map_err(|e| ApiError(StatusCode::BAD_GATEWAY, format!("malformed roster in pairing response: {e}")))?;

    let local_roster = store.hive_list_roster().await?;
    let merged = crate::hive::gossip::merge_roster(local_roster, remote_roster);
    for entry in &merged {
        store.hive_upsert_roster_entry(entry).await?;
    }

    // Eager first-sync against every newly-known Active peer, rather than
    // waiting up to sync_interval_seconds for the next timer tick.
    let roster_size = merged.len();
    let store_for_sync = store.clone();
    let identity_for_sync = (*identity).clone();
    tokio::spawn(async move {
        for peer in merged.iter().filter(|e| e.status == crate::hive::roster::RosterStatus::Active && e.device_id != identity_for_sync.device_id) {
            if let Some(address) = crate::hive::peer_status::resolve_address(&peer.device_id)
                && let Ok(client) = crate::hive::client::HiveClient::new(&identity_for_sync, &peer.public_key)
            {
                let _ = crate::hive::sync_loop::pull_from_peer(&client, &format!("https://{address}"), &store_for_sync).await;
            }
        }
    });

    Ok(Json(json!({ "joined": true, "roster_size": roster_size })))
}

pub(super) async fn hive_roster(State(store): State<Store>) -> Result<Json<Value>, ApiError> {
    let roster = store.hive_list_roster().await?;
    Ok(Json(json!({ "roster": roster })))
}

pub(super) async fn hive_manifest(State(store): State<Store>) -> Result<Json<Value>, ApiError> {
    let manifest = store.hive_manifest().await?;
    Ok(Json(json!({
        "memories": manifest.memories,
        "tombstones": manifest.tombstones,
        "settings": { "hash": manifest.settings.0, "updated_at": manifest.settings.1 },
        "tag_namespaces": { "hash": manifest.tag_namespaces.0, "updated_at": manifest.tag_namespaces.1 },
    })))
}

pub(super) async fn hive_issue_pairing_code(
    Extension(pairing_codes): Extension<Arc<PairingCodeStore>>,
    Extension(pairing_window): Extension<Arc<crate::hive::pairing_window::PairingWindow>>,
    Extension(identity): Extension<Arc<crate::hive::identity::DeviceIdentity>>,
) -> Json<Value> {
    let now = chrono::Utc::now().timestamp();
    let pairing = pairing_codes.issue(now);
    // Open the server-TLS-only pairing listener for exactly as long as this
    // code is valid; it auto-closes when the window elapses.
    pairing_window.open_for(std::time::Duration::from_secs(
        (pairing.expires_at - now).max(0) as u64,
    ));
    // `public_key` is meant to travel alongside `code` out-of-band (shown on
    // this device's own screen) so the joiner can pin the pairing TLS
    // connection to it instead of blindly trusting whatever cert answers on
    // `peer_address` -- see issue #26 / `JoinHiveBody::peer_public_key`.
    Json(json!({
        "code": pairing.code,
        "expires_at": pairing.expires_at,
        "public_key": crate::hive::identity::public_key_hex(&identity),
    }))
}

pub(super) async fn hive_get_memory(
    State(store): State<Store>,
    Path(id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    match store.recall_by_id(&id).await? {
        None => Err(ApiError(StatusCode::NOT_FOUND, format!("no memory {id}"))),
        Some(e) => {
            let hash = crate::store::compute_hive_content_hash(
                &e.title, &e.content, &e.tags, &e.layer, &e.memory_type,
            );
            Ok(Json(json!({
                "id": e.id, "title": e.title, "content": e.content, "tags": e.tags,
                "layer": e.layer, "memory_type": e.memory_type, "updated_at": e.updated_at,
                "hive_content_hash": hash,
            })))
        }
    }
}

#[derive(Deserialize)]
pub(super) struct AddTrustedNetworkBody {
    id: String,
    label: Option<String>,
}

pub(super) async fn hive_get_trusted_networks(
    State(store): State<Store>,
) -> Result<Json<Value>, ApiError> {
    let trusted = store.hive_trusted_networks().await?;
    let current_network = crate::hive::network::current_network_key_async().await;
    Ok(Json(json!({ "current_network": current_network, "trusted": trusted })))
}

pub(super) async fn hive_add_trusted_network(
    State(store): State<Store>,
    Json(body): Json<AddTrustedNetworkBody>,
) -> Result<Json<Value>, ApiError> {
    store.add_hive_trusted_network(&body.id, body.label).await?;
    Ok(Json(json!({ "trusted": store.hive_trusted_networks().await? })))
}

pub(super) async fn hive_remove_trusted_network(
    State(store): State<Store>,
    Path(id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    store.remove_hive_trusted_network(&id).await?;
    Ok(Json(json!({ "trusted": store.hive_trusted_networks().await? })))
}

pub(super) async fn hive_get_settings(State(store): State<Store>) -> Result<Json<Value>, ApiError> {
    let (sync_s, ping_s, updated_at) = store.hive_settings_override().await?.unwrap_or((300, 60, 0));
    Ok(Json(json!({
        "sync_interval_seconds": sync_s, "ping_interval_seconds": ping_s, "updated_at": updated_at,
    })))
}

pub(super) async fn hive_get_tag_namespaces(State(store): State<Store>) -> Result<Json<Value>, ApiError> {
    let namespaces = store.tag_namespace_registry().await;
    let updated_at: i64 = store
        .get_meta("tag_namespaces_updated_at")
        .await?
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    Ok(Json(json!({ "namespaces": namespaces, "updated_at": updated_at })))
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(super) enum HivePushBody {
    Memory {
        id: String, title: String, content: String, tags: Vec<String>,
        layer: String, memory_type: String, updated_at: i64, hive_content_hash: String,
    },
    Tombstone { memory_id: String, deleted_at: i64 },
    Settings { sync_interval_seconds: u64, ping_interval_seconds: u64, updated_at: i64 },
    TagNamespaces { namespaces: Value, updated_at: i64 },
}

pub(super) async fn hive_push(
    State(store): State<Store>,
    Json(body): Json<HivePushBody>,
) -> Result<Json<Value>, ApiError> {
    match body {
        HivePushBody::Memory { id, title, content, tags, layer, memory_type, updated_at, hive_content_hash } => {
            let incoming = crate::store::MemoryEntry {
                id, title, content, tags, created_at: updated_at, updated_at,
                token_count: None, layer, memory_type,
            };
            let outcome = store.apply_incoming_memory(&incoming, &hive_content_hash, None).await?;
            Ok(Json(json!({ "outcome": format!("{outcome:?}") })))
        }
        HivePushBody::Tombstone { memory_id, deleted_at } => {
            if let Some(local) = store.recall_by_id(&memory_id).await?
                && local.updated_at < deleted_at
            {
                store.delete(&memory_id).await?;
            }
            Ok(Json(json!({ "outcome": "tombstone_processed" })))
        }
        HivePushBody::Settings { sync_interval_seconds, ping_interval_seconds, updated_at } => {
            let current = store.hive_settings_override().await?;
            let should_apply = current.map(|(_, _, cur_updated_at)| updated_at > cur_updated_at).unwrap_or(true);
            if should_apply {
                store.set_hive_settings_override(sync_interval_seconds, ping_interval_seconds, updated_at).await?;
            }
            Ok(Json(json!({ "outcome": if should_apply { "applied" } else { "kept_local" } })))
        }
        HivePushBody::TagNamespaces { namespaces, updated_at } => {
            let current_updated_at: i64 = store
                .get_meta("tag_namespaces_updated_at")
                .await?
                .and_then(|s| s.parse().ok())
                .unwrap_or(0);
            if updated_at > current_updated_at {
                store.set_meta("tag_namespaces", &namespaces.to_string()).await?;
                store.set_meta("tag_namespaces_updated_at", &updated_at.to_string()).await?;
            }
            Ok(Json(json!({ "outcome": if updated_at > current_updated_at { "applied" } else { "kept_local" } })))
        }
    }
}
