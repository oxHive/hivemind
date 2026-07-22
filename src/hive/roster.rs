use crate::hive::identity::{self, DeviceIdentity};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JoinRecord {
    pub device_id: String,
    pub public_key: String,
    pub name: String,
    pub joined_at: i64,
    pub signature: String,
}

pub fn create_join_record(identity: &DeviceIdentity, name: &str, joined_at: i64) -> JoinRecord {
    let public_key = identity::public_key_hex(identity);
    let message = format!("join:{}:{}:{}:{}", identity.device_id, public_key, name, joined_at);
    let signature = identity::sign(identity, message.as_bytes());
    JoinRecord { device_id: identity.device_id.clone(), public_key, name: name.to_string(), joined_at, signature }
}

pub fn verify_join_record(record: &JoinRecord) -> bool {
    let message = format!(
        "join:{}:{}:{}:{}",
        record.device_id, record.public_key, record.name, record.joined_at
    );
    if !identity::verify(&record.public_key, message.as_bytes(), &record.signature) {
        return false;
    }
    let expected_device_id = format!("hive_{}", {
        let Ok(bytes) = hex::decode(&record.public_key) else { return false };
        if bytes.len() < 16 { return false; }
        hex::encode(&bytes[..16])
    });
    record.device_id == expected_device_id
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RevocationRecord {
    pub device_id: String,
    pub revoked_by: String,
    pub revoked_at: i64,
    pub signature: String,
}

pub fn create_revocation_record(revoker: &DeviceIdentity, target_device_id: &str, revoked_at: i64) -> RevocationRecord {
    let message = format!("revoke:{}:{}:{}", target_device_id, revoker.device_id, revoked_at);
    let signature = identity::sign(revoker, message.as_bytes());
    RevocationRecord {
        device_id: target_device_id.to_string(),
        revoked_by: revoker.device_id.clone(),
        revoked_at,
        signature,
    }
}

pub fn verify_revocation_record(record: &RevocationRecord, revoker_public_key: &str) -> bool {
    let message = format!("revoke:{}:{}:{}", record.device_id, record.revoked_by, record.revoked_at);
    identity::verify(revoker_public_key, message.as_bytes(), &record.signature)
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum RosterStatus {
    Active,
    Revoked,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RosterEntry {
    pub device_id: String,
    pub public_key: String,
    pub name: String,
    pub status: RosterStatus,
    pub joined_at: i64,
    pub revoked_at: Option<i64>,
    pub revoked_by: Option<String>,
    pub join_record: JoinRecord,
    pub revocation_record: Option<RevocationRecord>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hive::identity;

    #[test]
    fn join_record_round_trips() {
        let identity = identity::generate();
        let record = create_join_record(&identity, "alice-laptop", 1000);
        assert!(verify_join_record(&record));
    }

    #[test]
    fn join_record_rejects_tampered_name() {
        let identity = identity::generate();
        let mut record = create_join_record(&identity, "alice-laptop", 1000);
        record.name = "attacker-device".to_string();
        assert!(!verify_join_record(&record));
    }

    #[test]
    fn join_record_rejects_device_id_public_key_mismatch() {
        let a = identity::generate();
        let b = identity::generate();
        let mut record = create_join_record(&a, "alice-laptop", 1000);
        record.public_key = identity::public_key_hex(&b);
        // Signature no longer matches the (now swapped) public key either,
        // but this also independently fails the device_id/public_key
        // consistency check even if signature verification were somehow bypassed.
        assert!(!verify_join_record(&record));
    }

    #[test]
    fn revocation_record_round_trips() {
        let revoker = identity::generate();
        let record = create_revocation_record(&revoker, "hive_targetdeviceid00", 2000);
        let revoker_pk = identity::public_key_hex(&revoker);
        assert!(verify_revocation_record(&record, &revoker_pk));
    }

    #[test]
    fn revocation_record_rejects_wrong_revoker_key() {
        let revoker = identity::generate();
        let impostor = identity::generate();
        let record = create_revocation_record(&revoker, "hive_targetdeviceid00", 2000);
        let impostor_pk = identity::public_key_hex(&impostor);
        assert!(!verify_revocation_record(&record, &impostor_pk));
    }
}
