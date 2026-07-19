use crate::config::write_matrix_login;
use crate::matrix::keyring_store::SessionStore;
use anyhow::Result;
use std::path::Path;

/// Persists a matrix-sdk session (already-obtained, JSON-serialized) and the
/// homeserver/user_id pair. Split out from the actual `matrix_sdk::Client`
/// login call so this — the part with real logic worth testing — doesn't
/// need a live or mocked homeserver to test.
pub fn persist_login(
    homeserver_url: &str,
    user_id: &str,
    session_json: &str,
    store: &dyn SessionStore,
    global_config_path: &Path,
) -> Result<()> {
    store.save(user_id, session_json)?;
    write_matrix_login(global_config_path, homeserver_url, user_id)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::matrix::keyring_store::FakeSessionStore;

    #[test]
    fn persists_session_and_writes_config() {
        let tmp = tempfile::tempdir().unwrap();
        let config_path = tmp.path().join("config.toml");
        let store = FakeSessionStore::new();
        persist_login(
            "https://matrix.org",
            "@bot:matrix.org",
            "{\"access_token\":\"abc\"}",
            &store,
            &config_path,
        )
        .unwrap();
        assert_eq!(
            store.load("@bot:matrix.org").unwrap(),
            Some("{\"access_token\":\"abc\"}".to_string())
        );
        let settings = crate::config::load_matrix_settings(&config_path)
            .unwrap()
            .unwrap();
        assert_eq!(settings.homeserver_url, "https://matrix.org");
        assert_eq!(settings.user_id, "@bot:matrix.org");
    }
}
