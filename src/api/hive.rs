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
) -> Json<Value> {
    let now = chrono::Utc::now().timestamp();
    let pairing = pairing_codes.issue(now);
    Json(json!({ "code": pairing.code, "expires_at": pairing.expires_at }))
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
            let outcome = store.apply_incoming_memory(&incoming, &hive_content_hash).await?;
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
