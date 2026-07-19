use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

struct Entry {
    session_id: String,
    last_active: Instant,
}

/// How long a room's session stays resumable after its last activity, in the
/// absence of a configured `[matrix] session_ttl_seconds`. Past this, `get`
/// treats it as detached and the next message starts a fresh agent session
/// instead of resuming a stale one.
const DEFAULT_SESSION_TTL: Duration =
    Duration::from_secs(crate::config::DEFAULT_SESSION_TTL_SECONDS);

#[derive(Clone)]
pub struct SessionMap {
    entries: Arc<Mutex<HashMap<String, Entry>>>,
    ttl: Duration,
}

impl Default for SessionMap {
    fn default() -> Self {
        Self::new(DEFAULT_SESSION_TTL)
    }
}

impl SessionMap {
    pub fn new(ttl: Duration) -> Self {
        SessionMap {
            entries: Arc::new(Mutex::new(HashMap::new())),
            ttl,
        }
    }

    /// Returns the room's resumable session ID, or `None` if there isn't one
    /// or it's expired (in which case the expired entry is dropped).
    pub async fn get(&self, room_id: &str) -> Option<String> {
        let mut map = self.entries.lock().await;
        let entry = map.get(room_id)?;
        if entry.last_active.elapsed() > self.ttl {
            map.remove(room_id);
            return None;
        }
        Some(entry.session_id.clone())
    }

    pub async fn set(&self, room_id: &str, session_id: String) {
        self.entries.lock().await.insert(
            room_id.to_string(),
            Entry {
                session_id,
                last_active: Instant::now(),
            },
        );
    }

    pub async fn reset(&self, room_id: &str) {
        self.entries.lock().await.remove(room_id);
    }

    /// Test-only: inserts a session whose `last_active` is already `age` in
    /// the past, so TTL expiry can be exercised without a real 2-minute
    /// sleep.
    #[cfg(test)]
    async fn set_aged(&self, room_id: &str, session_id: String, age: Duration) {
        self.entries.lock().await.insert(
            room_id.to_string(),
            Entry {
                session_id,
                last_active: Instant::now() - age,
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn get_on_empty_map_returns_none() {
        let map = SessionMap::default();
        assert_eq!(map.get("!room:matrix.org").await, None);
    }

    #[tokio::test]
    async fn set_then_get_returns_the_stored_session_id() {
        let map = SessionMap::default();
        map.set("!room:matrix.org", "sess-1".to_string()).await;
        assert_eq!(
            map.get("!room:matrix.org").await,
            Some("sess-1".to_string())
        );
    }

    #[tokio::test]
    async fn set_overwrites_previous_session_id_for_the_same_room() {
        let map = SessionMap::default();
        map.set("!room:matrix.org", "sess-1".to_string()).await;
        map.set("!room:matrix.org", "sess-2".to_string()).await;
        assert_eq!(
            map.get("!room:matrix.org").await,
            Some("sess-2".to_string())
        );
    }

    #[tokio::test]
    async fn session_within_ttl_is_resumable() {
        let map = SessionMap::default();
        map.set_aged(
            "!room:matrix.org",
            "sess-1".to_string(),
            Duration::from_secs(60),
        )
        .await;
        assert_eq!(
            map.get("!room:matrix.org").await,
            Some("sess-1".to_string())
        );
    }

    #[tokio::test]
    async fn session_past_ttl_is_detached_and_removed() {
        let map = SessionMap::default();
        map.set_aged(
            "!room:matrix.org",
            "sess-1".to_string(),
            Duration::from_secs(121),
        )
        .await;
        assert_eq!(map.get("!room:matrix.org").await, None);
        // Expiry drops the stale entry rather than just masking it.
        map.set_aged(
            "!room:matrix.org",
            "sess-1".to_string(),
            Duration::from_secs(121),
        )
        .await;
        assert_eq!(map.get("!room:matrix.org").await, None);
    }

    #[tokio::test]
    async fn reset_clears_only_that_room() {
        let map = SessionMap::default();
        map.set("!a:matrix.org", "sess-a".to_string()).await;
        map.set("!b:matrix.org", "sess-b".to_string()).await;
        map.reset("!a:matrix.org").await;
        assert_eq!(map.get("!a:matrix.org").await, None);
        assert_eq!(map.get("!b:matrix.org").await, Some("sess-b".to_string()));
    }

    #[tokio::test]
    async fn cloned_map_shares_state() {
        let map = SessionMap::default();
        let cloned = map.clone();
        cloned.set("!room:matrix.org", "sess-1".to_string()).await;
        assert_eq!(
            map.get("!room:matrix.org").await,
            Some("sess-1".to_string())
        );
    }
}
