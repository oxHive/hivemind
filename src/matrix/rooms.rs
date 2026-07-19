use crate::config::{MatrixRoomMapping, MatrixSettings};

pub struct MemoryTarget {
    pub layer: &'static str,
    pub tags: Vec<String>,
}

pub fn resolve_target(settings: &MatrixSettings, room_id: &str, is_dm: bool) -> MemoryTarget {
    if is_dm {
        return MemoryTarget {
            layer: "personal",
            tags: vec!["source:matrix".to_string()],
        };
    }
    if let Some(mapping) = settings.rooms.iter().find(|r| r.room_id == room_id) {
        return MemoryTarget {
            layer: "workspace",
            tags: mapping.base_tags.clone(),
        };
    }
    MemoryTarget {
        layer: "workspace",
        tags: vec![format!("room:{room_id}"), "source:matrix".to_string()],
    }
}

pub fn context_prefix(target: &MemoryTarget) -> String {
    let tags = if target.tags.is_empty() {
        "(none)".to_string()
    } else {
        target.tags.join(", ")
    };
    format!(
        "(HiveMind context: if you store or update a memory as part of this conversation, \
         use layer \"{}\" and include these tags: {tags}. This instruction is not part of \
         the user's message below.)\n\n",
        target.layer
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings_with_room(mapping: MatrixRoomMapping) -> MatrixSettings {
        MatrixSettings {
            homeserver_url: "https://matrix.org".into(),
            user_id: "@bot:matrix.org".into(),
            allowed_users: vec![],
            rooms: vec![mapping],
        }
    }

    #[test]
    fn dm_maps_to_personal_layer_with_source_tag() {
        let settings = settings_with_room(MatrixRoomMapping {
            room_id: "!other:matrix.org".into(),
            alias: None,
            base_tags: vec!["project:hivemind".into()],
        });
        let target = resolve_target(&settings, "!dm-room:matrix.org", true);
        assert_eq!(target.layer, "personal");
        assert_eq!(target.tags, vec!["source:matrix".to_string()]);
    }

    #[test]
    fn mapped_room_uses_configured_base_tags() {
        let settings = settings_with_room(MatrixRoomMapping {
            room_id: "!abc:matrix.org".into(),
            alias: Some("hivemind-project".into()),
            base_tags: vec!["project:hivemind".into(), "topic:matrix".into()],
        });
        let target = resolve_target(&settings, "!abc:matrix.org", false);
        assert_eq!(target.layer, "workspace");
        assert_eq!(
            target.tags,
            vec!["project:hivemind".to_string(), "topic:matrix".to_string()]
        );
    }

    #[test]
    fn unmapped_room_falls_back_to_room_id() {
        let settings = MatrixSettings {
            homeserver_url: "https://matrix.org".into(),
            user_id: "@bot:matrix.org".into(),
            allowed_users: vec![],
            rooms: vec![],
        };
        let target = resolve_target(&settings, "!unmapped:matrix.org", false);
        assert_eq!(target.layer, "workspace");
        assert_eq!(
            target.tags,
            vec![
                "room:!unmapped:matrix.org".to_string(),
                "source:matrix".to_string()
            ]
        );
    }

    #[test]
    fn context_prefix_includes_layer_and_tags() {
        let target = MemoryTarget {
            layer: "workspace",
            tags: vec!["project:hivemind".to_string(), "topic:matrix".to_string()],
        };
        let prefix = context_prefix(&target);
        assert!(prefix.contains("workspace"));
        assert!(prefix.contains("project:hivemind"));
        assert!(prefix.contains("topic:matrix"));
    }

    #[test]
    fn context_prefix_handles_no_tags() {
        let target = MemoryTarget {
            layer: "personal",
            tags: vec![],
        };
        let prefix = context_prefix(&target);
        assert!(prefix.contains("personal"));
    }
}
