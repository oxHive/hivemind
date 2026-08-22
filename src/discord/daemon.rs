use crate::config::DiscordSettings;

pub struct EventDecision {
    pub should_handle: bool,
    pub is_dm: bool,
}

pub fn decide(
    settings: &DiscordSettings,
    is_dm: bool,
    author_is_bot: bool,
    author_id: &str,
    mentions_bot: bool,
) -> EventDecision {
    if author_is_bot {
        return EventDecision {
            should_handle: false,
            is_dm,
        };
    }
    let should_handle = if is_dm {
        settings.allowed_users.iter().any(|u| u == author_id)
    } else {
        mentions_bot
    };
    EventDecision { should_handle, is_dm }
}

/// Maps the `[discord] permission_gate` config string to a Discord permission
/// bit, used as `/hm`'s `default_member_permissions` at registration time.
pub fn parse_permission_gate(value: &str) -> Result<serenity::model::Permissions, String> {
    use serenity::model::Permissions;
    match value {
        "manage_guild" => Ok(Permissions::MANAGE_GUILD),
        "administrator" => Ok(Permissions::ADMINISTRATOR),
        "manage_channels" => Ok(Permissions::MANAGE_CHANNELS),
        "manage_messages" => Ok(Permissions::MANAGE_MESSAGES),
        "kick_members" => Ok(Permissions::KICK_MEMBERS),
        "ban_members" => Ok(Permissions::BAN_MEMBERS),
        other => Err(format!(
            "unknown [discord] permission_gate \"{other}\" (expected one of: manage_guild, \
             administrator, manage_channels, manage_messages, kick_members, ban_members)"
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::DiscordChannelMapping;

    fn settings() -> DiscordSettings {
        DiscordSettings {
            application_id: "999999999999999999".into(),
            allowed_users: vec!["111111111111111111".into()],
            permission_gate: None,
            channels: vec![DiscordChannelMapping {
                channel_id: "222222222222222222".into(),
                alias: None,
                base_tags: vec!["project:hivemind".into()],
            }],
            session_ttl_seconds: crate::config::DEFAULT_SESSION_TTL_SECONDS,
        }
    }

    #[test]
    fn own_bot_messages_are_never_handled() {
        let d = decide(&settings(), false, true, "999999999999999999", true);
        assert!(!d.should_handle);
    }

    #[test]
    fn dm_from_allowed_user_is_handled() {
        let d = decide(&settings(), true, false, "111111111111111111", false);
        assert!(d.should_handle);
        assert!(d.is_dm);
    }

    #[test]
    fn dm_from_non_allowed_user_is_silently_ignored() {
        let d = decide(&settings(), true, false, "333333333333333333", false);
        assert!(!d.should_handle);
    }

    #[test]
    fn channel_message_without_mention_is_ignored() {
        let d = decide(&settings(), false, false, "111111111111111111", false);
        assert!(!d.should_handle);
    }

    #[test]
    fn channel_message_with_mention_is_handled_regardless_of_sender() {
        let d = decide(&settings(), false, false, "444444444444444444", true);
        assert!(d.should_handle);
        assert!(!d.is_dm);
    }

    #[test]
    fn parse_permission_gate_accepts_known_values() {
        assert_eq!(
            parse_permission_gate("manage_guild").unwrap(),
            serenity::model::Permissions::MANAGE_GUILD
        );
        assert_eq!(
            parse_permission_gate("administrator").unwrap(),
            serenity::model::Permissions::ADMINISTRATOR
        );
    }

    #[test]
    fn parse_permission_gate_rejects_unknown_values() {
        let err = parse_permission_gate("super_admin").unwrap_err();
        assert!(err.contains("super_admin"));
        assert!(err.contains("manage_guild"));
    }
}
