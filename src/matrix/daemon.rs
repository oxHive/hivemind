use crate::config::{AgentSettings, MatrixSettings};
use crate::matrix::keyring_store::{KeyringSessionStore, SessionStore};
use crate::matrix::status::StatusReply;
use anyhow::Result;
use std::sync::Arc;
use tokio::sync::Mutex;

pub struct EventDecision {
    pub should_handle: bool,
    pub is_dm: bool,
}

pub fn decide(
    settings: &MatrixSettings,
    _room_id: &str,
    is_dm: bool,
    sender_user_id: &str,
    is_own_message: bool,
    mentions_bot: bool,
) -> EventDecision {
    if is_own_message {
        return EventDecision { should_handle: false, is_dm };
    }
    let should_handle = if is_dm {
        settings.allowed_users.iter().any(|u| u == sender_user_id)
    } else {
        mentions_bot
    };
    EventDecision { should_handle, is_dm }
}

pub async fn run(settings: MatrixSettings, agent: AgentSettings, hivemind_bin: String) -> Result<()> {
    use matrix_sdk::config::SyncSettings as MatrixSyncSettings;
    use matrix_sdk::ruma::events::room::message::{MessageType, OriginalSyncRoomMessageEvent};
    use matrix_sdk::{Client, Room, RoomState};

    let store = KeyringSessionStore;
    let session_json = store
        .load(&settings.user_id)?
        .ok_or_else(|| anyhow::anyhow!("no saved session — run `hivemind matrix login` first"))?;
    let session: matrix_sdk::authentication::matrix::MatrixSession = serde_json::from_str(&session_json)?;

    let client = Client::builder()
        .homeserver_url(&settings.homeserver_url)
        .sqlite_store(crate::db::xdg_data_dir().join("matrix-store"), None)
        .build()
        .await?;
    client.restore_session(session).await?;

    let status_reply = Arc::new(Mutex::new(StatusReply {
        logged_in: true,
        user_id: settings.user_id.clone(),
        sync_state: "connecting".to_string(),
        last_sync_at: None,
        rooms: settings
            .rooms
            .iter()
            .map(|r| crate::matrix::status::RoomStatus {
                room_id: r.room_id.clone(),
                alias: r.alias.clone(),
                active_session: false,
                last_active_at: None,
            })
            .collect(),
    }));
    let socket_status = status_reply.clone();
    let socket_path = crate::matrix::status::socket_path();
    tokio::spawn(async move {
        if let Err(e) = crate::matrix::status::serve_status(&socket_path, socket_status).await {
            tracing::warn!("status socket exited: {e:#}");
        }
    });

    let sessions = crate::matrix::session::SessionMap::new();
    let bot_user_id = settings.user_id.clone();
    let settings = Arc::new(settings);
    let agent = Arc::new(agent);
    let hivemind_bin = Arc::new(hivemind_bin);

    client.add_event_handler(move |event: OriginalSyncRoomMessageEvent, room: Room| {
        let settings = settings.clone();
        let agent = agent.clone();
        let hivemind_bin = hivemind_bin.clone();
        let sessions = sessions.clone();
        let bot_user_id = bot_user_id.clone();
        async move {
            if room.state() != RoomState::Joined {
                return;
            }
            let is_own_message = event.sender.as_str() == bot_user_id;
            let is_dm = room.is_direct().await.unwrap_or(false);
            let MessageType::Text(text) = &event.content.msgtype else { return };
            let mentions_bot = text.body.contains(&bot_user_id);
            let decision = decide(
                &settings,
                room.room_id().as_str(),
                is_dm,
                event.sender.as_str(),
                is_own_message,
                mentions_bot,
            );
            if !decision.should_handle {
                return;
            }
            match crate::matrix::commands::parse(&text.body) {
                crate::matrix::commands::Command::Reset => {
                    sessions.reset(room.room_id().as_str()).await;
                }
                crate::matrix::commands::Command::Store(memory_text) => {
                    let prompt = format!(
                        "Call memory_store with content: {memory_text:?}. Use layer and tags per the room's mapping rules already in your system context."
                    );
                    if let Ok(result) = crate::matrix::agent::run_turn(&agent, &hivemind_bin, &prompt, None).await {
                        let _ = room.send(matrix_sdk::ruma::events::room::message::RoomMessageEventContent::text_plain(result.reply_text)).await;
                    }
                }
                crate::matrix::commands::Command::Chat(message) => {
                    let resume = sessions.get(room.room_id().as_str()).await;
                    match crate::matrix::agent::run_turn(&agent, &hivemind_bin, &message, resume.as_deref()).await {
                        Ok(result) => {
                            sessions.set(room.room_id().as_str(), result.session_id).await;
                            let _ = room.send(matrix_sdk::ruma::events::room::message::RoomMessageEventContent::text_plain(result.reply_text)).await;
                        }
                        Err(e) => {
                            sessions.reset(room.room_id().as_str()).await;
                            let _ = room.send(matrix_sdk::ruma::events::room::message::RoomMessageEventContent::text_plain(format!("hivemind matrix hit an error: {e}"))).await;
                        }
                    }
                }
            }
        }
    });

    {
        let mut r = status_reply.lock().await;
        r.sync_state = "synced".to_string();
    }
    client.sync(MatrixSyncSettings::default()).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::MatrixRoomMapping;

    fn settings() -> MatrixSettings {
        MatrixSettings {
            homeserver_url: "https://matrix.org".into(),
            user_id: "@bot:matrix.org".into(),
            allowed_users: vec!["@you:matrix.org".into()],
            rooms: vec![MatrixRoomMapping {
                room_id: "!abc:matrix.org".into(),
                alias: None,
                base_tags: vec!["project:hivemind".into()],
            }],
        }
    }

    #[test]
    fn own_messages_are_never_handled() {
        let d = decide(&settings(), "!abc:matrix.org", false, "@bot:matrix.org", true, true);
        assert!(!d.should_handle);
    }

    #[test]
    fn dm_from_allowed_user_is_handled() {
        let d = decide(&settings(), "!dm:matrix.org", true, "@you:matrix.org", false, false);
        assert!(d.should_handle);
        assert!(d.is_dm);
    }

    #[test]
    fn dm_from_non_allowed_user_is_silently_ignored() {
        let d = decide(&settings(), "!dm:matrix.org", true, "@stranger:matrix.org", false, false);
        assert!(!d.should_handle);
    }

    #[test]
    fn room_message_without_mention_is_ignored() {
        let d = decide(&settings(), "!abc:matrix.org", false, "@you:matrix.org", false, false);
        assert!(!d.should_handle);
    }

    #[test]
    fn room_message_with_mention_is_handled_regardless_of_sender() {
        let d = decide(&settings(), "!abc:matrix.org", false, "@anyone:matrix.org", false, true);
        assert!(d.should_handle);
        assert!(!d.is_dm);
    }
}
