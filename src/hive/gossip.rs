use crate::hive::roster::{verify_join_record, verify_revocation_record, RosterEntry, RosterStatus};

/// Merges an incoming roster (received from a peer over gossip) into the
/// local roster, applying the trust rules that make revocation
/// gossip-safe:
///
/// 1. Any incoming entry whose `join_record` doesn't verify is dropped
///    entirely.
/// 2. An incoming entry not yet known locally is added, active.
/// 3./4. An incoming revocation for a locally-`Active` entry is applied only
///    if it verifies against the revoker's public key AND the revoker is
///    itself a currently-`Active` entry in the *local* roster. This covers
///    both self-revocation (revoker == target) and the common case of a
///    separate trusted revoker.
/// 5. A `Revoked` local entry is never overwritten back to `Active`.
/// 6. Merge is idempotent.
///
/// This is a pure function: no I/O, no clock reads. Callers (pairing
/// handshake, data-sync exchange) are responsible for persisting the result.
pub fn merge_roster(local: Vec<RosterEntry>, incoming: Vec<RosterEntry>) -> Vec<RosterEntry> {
    let mut merged = local;

    for candidate in incoming {
        if !verify_join_record(&candidate.join_record) {
            continue;
        }

        match merged.iter_mut().find(|e| e.device_id == candidate.device_id) {
            None => merged.push(candidate),
            Some(existing) => {
                if existing.status == RosterStatus::Revoked {
                    continue;
                }
                if let Some(revocation) = &candidate.revocation_record {
                    let revoker_is_active_locally = merged
                        .iter()
                        .find(|e| e.device_id == revocation.revoked_by)
                        .map(|e| e.status == RosterStatus::Active)
                        .unwrap_or(false);
                    let revoker_public_key = merged
                        .iter()
                        .find(|e| e.device_id == revocation.revoked_by)
                        .map(|e| e.public_key.clone());
                    let sig_ok = revoker_public_key
                        .map(|pk| verify_revocation_record(revocation, &pk))
                        .unwrap_or(false);
                    if revoker_is_active_locally && sig_ok {
                        if let Some(existing) = merged.iter_mut().find(|e| e.device_id == candidate.device_id) {
                            existing.status = RosterStatus::Revoked;
                            existing.revoked_at = Some(revocation.revoked_at);
                            existing.revoked_by = Some(revocation.revoked_by.clone());
                            existing.revocation_record = Some(revocation.clone());
                        }
                    }
                }
            }
        }
    }

    merged
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hive::identity::{self, DeviceIdentity};
    use crate::hive::roster::create_join_record;
    use crate::hive::roster::create_revocation_record;

    fn entry_for(identity: &DeviceIdentity, name: &str, joined_at: i64) -> RosterEntry {
        let join_record = create_join_record(identity, name, joined_at);
        RosterEntry {
            device_id: identity.device_id.clone(),
            public_key: identity::public_key_hex(identity),
            name: name.to_string(),
            status: RosterStatus::Active,
            joined_at,
            revoked_at: None,
            revoked_by: None,
            join_record,
            revocation_record: None,
        }
    }

    #[test]
    fn adds_new_unknown_device() {
        let device = identity::generate();
        let entry = entry_for(&device, "alice-laptop", 1000);
        let merged = merge_roster(vec![], vec![entry.clone()]);
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].device_id, entry.device_id);
        assert_eq!(merged[0].status, RosterStatus::Active);
    }

    #[test]
    fn drops_entry_with_invalid_join_record() {
        let device = identity::generate();
        let mut entry = entry_for(&device, "alice-laptop", 1000);
        entry.join_record.name = "tampered".to_string();
        let merged = merge_roster(vec![], vec![entry]);
        assert!(merged.is_empty());
    }

    #[test]
    fn applies_revocation_from_active_local_revoker() {
        let a = identity::generate();
        let b = identity::generate();
        let local = vec![entry_for(&a, "alice-laptop", 1000), entry_for(&b, "bob-phone", 1100)];

        let revocation = create_revocation_record(&a, &b.device_id, 2000);
        let mut incoming_b = entry_for(&b, "bob-phone", 1100);
        incoming_b.revocation_record = Some(revocation);

        let merged = merge_roster(local, vec![incoming_b]);
        let b_entry = merged.iter().find(|e| e.device_id == b.device_id).unwrap();
        assert_eq!(b_entry.status, RosterStatus::Revoked);
        assert_eq!(b_entry.revoked_by.as_deref(), Some(a.device_id.as_str()));
    }

    #[test]
    fn rejects_revocation_from_revoker_not_locally_active() {
        let a = identity::generate();
        let b = identity::generate();
        // "a" is NOT in the local roster at all, so it can't be trusted as a revoker.
        let local = vec![entry_for(&b, "bob-phone", 1100)];

        let revocation = create_revocation_record(&a, &b.device_id, 2000);
        let mut incoming_b = entry_for(&b, "bob-phone", 1100);
        incoming_b.revocation_record = Some(revocation);

        let merged = merge_roster(local, vec![incoming_b]);
        let b_entry = merged.iter().find(|e| e.device_id == b.device_id).unwrap();
        assert_eq!(b_entry.status, RosterStatus::Active, "revocation from an untrusted revoker must not apply");
    }

    #[test]
    fn revocation_is_sticky_never_reverts_to_active() {
        let a = identity::generate();
        let b = identity::generate();
        let mut b_entry = entry_for(&b, "bob-phone", 1100);
        b_entry.status = RosterStatus::Revoked;
        b_entry.revoked_by = Some(a.device_id.clone());
        b_entry.revoked_at = Some(2000);
        let local = vec![entry_for(&a, "alice-laptop", 1000), b_entry];

        // A later incoming entry claims b is still active (e.g. b itself gossiping
        // its own unrevoked join record after being revoked) -- must not un-revoke.
        let incoming_b_active_again = entry_for(&b, "bob-phone", 1100);
        let merged = merge_roster(local, vec![incoming_b_active_again]);
        let b_final = merged.iter().find(|e| e.device_id == b.device_id).unwrap();
        assert_eq!(b_final.status, RosterStatus::Revoked);
    }

    #[test]
    fn merge_is_idempotent() {
        let device = identity::generate();
        let entry = entry_for(&device, "alice-laptop", 1000);
        let once = merge_roster(vec![], vec![entry.clone()]);
        let twice = merge_roster(once.clone(), vec![entry]);
        assert_eq!(once.len(), twice.len());
        assert_eq!(once[0].device_id, twice[0].device_id);
        assert_eq!(once[0].status, twice[0].status);
    }
}
