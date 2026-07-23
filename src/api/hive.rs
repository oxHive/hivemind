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
