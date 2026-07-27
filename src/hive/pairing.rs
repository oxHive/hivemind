use std::collections::HashMap;
use std::sync::Mutex;

const CODE_CHARS: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789"; // no 0/O/1/I
const CODE_LEN: usize = 8;
const CODE_TTL_SECONDS: i64 = 300;

pub struct PairingCode {
    pub code: String,
    pub expires_at: i64,
}

pub fn generate_pairing_code(now: i64) -> PairingCode {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    let code: String = (0..CODE_LEN)
        .map(|_| CODE_CHARS[rng.gen_range(0..CODE_CHARS.len())] as char)
        .collect();
    PairingCode {
        code,
        expires_at: now + CODE_TTL_SECONDS,
    }
}

pub struct PairingCodeStore {
    outstanding: Mutex<HashMap<String, i64>>,
}

impl PairingCodeStore {
    pub fn new() -> Self {
        Self {
            outstanding: Mutex::new(HashMap::new()),
        }
    }

    pub fn issue(&self, now: i64) -> PairingCode {
        let pairing = generate_pairing_code(now);
        self.outstanding
            .lock()
            .unwrap()
            .insert(pairing.code.clone(), pairing.expires_at);
        PairingCode {
            code: pairing.code,
            expires_at: pairing.expires_at,
        }
    }

    pub fn validate_and_consume(&self, code: &str, now: i64) -> bool {
        let mut outstanding = self.outstanding.lock().unwrap();
        match outstanding.remove(code) {
            Some(expires_at) => now < expires_at,
            None => false,
        }
    }
}

impl Default for PairingCodeStore {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_code_has_expected_length_and_charset() {
        let pairing = generate_pairing_code(1000);
        assert_eq!(pairing.code.len(), CODE_LEN);
        assert!(
            pairing
                .code
                .chars()
                .all(|c| CODE_CHARS.contains(&(c as u8)))
        );
        assert_eq!(pairing.expires_at, 1000 + CODE_TTL_SECONDS);
    }

    #[test]
    fn issued_code_validates_once_then_is_consumed() {
        let store = PairingCodeStore::new();
        let pairing = store.issue(1000);
        assert!(store.validate_and_consume(&pairing.code, 1100));
        assert!(
            !store.validate_and_consume(&pairing.code, 1100),
            "code must be single-use"
        );
    }

    #[test]
    fn expired_code_is_rejected() {
        let store = PairingCodeStore::new();
        let pairing = store.issue(1000);
        let past_expiry = pairing.expires_at + 1;
        assert!(!store.validate_and_consume(&pairing.code, past_expiry));
    }

    #[test]
    fn unknown_code_is_rejected() {
        let store = PairingCodeStore::new();
        assert!(!store.validate_and_consume("NOTAREALCODE", 1000));
    }
}
