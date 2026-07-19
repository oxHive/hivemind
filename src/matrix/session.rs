use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

#[derive(Clone)]
pub struct SessionMap(Arc<Mutex<HashMap<String, String>>>);

impl SessionMap {
    pub fn new() -> Self {
        SessionMap(Arc::new(Mutex::new(HashMap::new())))
    }

    pub async fn get(&self, room_id: &str) -> Option<String> {
        self.0.lock().await.get(room_id).cloned()
    }

    pub async fn set(&self, room_id: &str, session_id: String) {
        self.0.lock().await.insert(room_id.to_string(), session_id);
    }

    pub async fn reset(&self, room_id: &str) {
        self.0.lock().await.remove(room_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn get_on_empty_map_returns_none() {
        let map = SessionMap::new();
        assert_eq!(map.get("!room:matrix.org").await, None);
    }

    #[tokio::test]
    async fn set_then_get_returns_the_stored_session_id() {
        let map = SessionMap::new();
        map.set("!room:matrix.org", "sess-1".to_string()).await;
        assert_eq!(
            map.get("!room:matrix.org").await,
            Some("sess-1".to_string())
        );
    }

    #[tokio::test]
    async fn set_overwrites_previous_session_id_for_the_same_room() {
        let map = SessionMap::new();
        map.set("!room:matrix.org", "sess-1".to_string()).await;
        map.set("!room:matrix.org", "sess-2".to_string()).await;
        assert_eq!(
            map.get("!room:matrix.org").await,
            Some("sess-2".to_string())
        );
    }

    #[tokio::test]
    async fn reset_clears_only_that_room() {
        let map = SessionMap::new();
        map.set("!a:matrix.org", "sess-a".to_string()).await;
        map.set("!b:matrix.org", "sess-b".to_string()).await;
        map.reset("!a:matrix.org").await;
        assert_eq!(map.get("!a:matrix.org").await, None);
        assert_eq!(map.get("!b:matrix.org").await, Some("sess-b".to_string()));
    }

    #[tokio::test]
    async fn cloned_map_shares_state() {
        let map = SessionMap::new();
        let cloned = map.clone();
        cloned.set("!room:matrix.org", "sess-1".to_string()).await;
        assert_eq!(
            map.get("!room:matrix.org").await,
            Some("sess-1".to_string())
        );
    }
}
