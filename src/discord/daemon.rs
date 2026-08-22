use crate::config::AgentSettings;
use crate::config::DiscordSettings;
use crate::discord::status::{ChannelStatus, StatusReply};
use crate::discord::token_store::{KeyringTokenStore, TokenStore};
use anyhow::Result;
use serenity::all::{
    Command, Context, CreateCommand, CreateCommandOption, CommandOptionType,
    CreateInteractionResponse, CreateInteractionResponseMessage, EventHandler, GatewayIntents,
    Interaction, Message, Ready, ResolvedValue,
};
use serenity::async_trait;
use std::sync::Arc;
use tokio::sync::Mutex;

struct PidGuard(std::path::PathBuf);

impl Drop for PidGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

fn write_pidfile() -> Result<PidGuard> {
    let path = crate::db::discord_pidfile_path();
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(&path, std::process::id().to_string())?;
    Ok(PidGuard(path))
}

fn now_ts() -> String {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        .to_string()
}

async fn mark_channel_active(status_reply: &Arc<Mutex<StatusReply>>, channel_id: &str) {
    let mut r = status_reply.lock().await;
    if let Some(channel) = r.channels.iter_mut().find(|c| c.channel_id == channel_id) {
        channel.active_session = true;
        channel.last_active_at = Some(now_ts());
    } else {
        r.channels.push(ChannelStatus {
            channel_id: channel_id.to_string(),
            alias: None,
            active_session: true,
            last_active_at: Some(now_ts()),
        });
    }
}

async fn mark_channel_inactive(status_reply: &Arc<Mutex<StatusReply>>, channel_id: &str) {
    let mut r = status_reply.lock().await;
    if let Some(channel) = r.channels.iter_mut().find(|c| c.channel_id == channel_id) {
        channel.active_session = false;
    }
}

const HELP_TEXT: &str = "/hm store text:<text>  — directly store a memory, skipping the agent\n\
                          /hm reset              — start a fresh conversation in this channel\n\
                          /hm help                — show this message\n\
                          Mention me in a channel, or DM me directly, to chat freely.";

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

struct Handler {
    settings: Arc<DiscordSettings>,
    agent: Arc<AgentSettings>,
    hivemind_bin: Arc<String>,
    sessions: crate::discord::session::SessionMap,
    status_reply: Arc<Mutex<StatusReply>>,
    permission_gate: Option<serenity::model::Permissions>,
}

fn build_hm_command(permission_gate: Option<serenity::model::Permissions>) -> CreateCommand {
    let mut cmd = CreateCommand::new("hm")
        .description("HiveMind memory bot")
        .add_option(
            CreateCommandOption::new(CommandOptionType::SubCommand, "store", "Directly store a memory")
                .add_sub_option(
                    CreateCommandOption::new(CommandOptionType::String, "text", "Memory text")
                        .required(true),
                ),
        )
        .add_option(CreateCommandOption::new(
            CommandOptionType::SubCommand,
            "reset",
            "Reset this channel's conversation",
        ))
        .add_option(CreateCommandOption::new(
            CommandOptionType::SubCommand,
            "help",
            "List /hm commands",
        ));
    if let Some(perm) = permission_gate {
        cmd = cmd.default_member_permissions(perm);
    }
    cmd
}

async fn respond_ephemeral(ctx: &Context, command: &serenity::all::CommandInteraction, text: &str) {
    let _ = command
        .create_response(
            &ctx.http,
            CreateInteractionResponse::Message(
                CreateInteractionResponseMessage::new()
                    .content(text)
                    .ephemeral(true),
            ),
        )
        .await;
}

#[async_trait]
impl EventHandler for Handler {
    async fn ready(&self, ctx: Context, _ready: Ready) {
        tracing::debug!("discord gateway ready, registering /hm command");
        if let Err(e) = Command::create_global_command(&ctx.http, build_hm_command(self.permission_gate)).await {
            tracing::warn!("failed to register /hm command: {e:#}");
        }
        let mut r = self.status_reply.lock().await;
        r.sync_state = "connected".to_string();
        r.last_sync_at = Some(now_ts());
    }

    async fn message(&self, ctx: Context, msg: Message) {
        if msg.author.bot {
            return;
        }
        let is_dm = msg.guild_id.is_none();
        let bot_id = ctx.cache.current_user().id;
        let mentions_bot = msg.mentions_user_id(bot_id);
        let decision = crate::discord::daemon::decide(
            &self.settings,
            is_dm,
            msg.author.bot,
            &msg.author.id.to_string(),
            mentions_bot,
        );
        if !decision.should_handle {
            return;
        }

        let channel_id = msg.channel_id.to_string();
        {
            let mut r = self.status_reply.lock().await;
            r.last_sync_at = Some(now_ts());
        }
        let target = crate::discord::channels::resolve_target(&self.settings, &channel_id, is_dm);
        let system_prompt = crate::discord::channels::context_system_prompt(&target);
        let resume = self.sessions.get(&channel_id).await;
        match crate::chat_bot::agent::run_turn(
            &self.agent,
            &self.hivemind_bin,
            &msg.content,
            resume.as_deref(),
            Some(&system_prompt),
        )
        .await
        {
            Ok(result) => {
                self.sessions.set(&channel_id, result.session_id).await;
                mark_channel_active(&self.status_reply, &channel_id).await;
                let _ = msg.channel_id.say(&ctx.http, result.reply_text).await;
            }
            Err(e) => {
                self.sessions.reset(&channel_id).await;
                mark_channel_inactive(&self.status_reply, &channel_id).await;
                let _ = msg
                    .channel_id
                    .say(&ctx.http, format!("hivemind discord hit an error: {e}"))
                    .await;
            }
        }
    }

    async fn interaction_create(&self, ctx: Context, interaction: Interaction) {
        let Interaction::Command(command) = interaction else {
            return;
        };
        if command.data.name != "hm" {
            return;
        }
        let is_dm = command.guild_id.is_none();
        let author_id = command.user.id.to_string();
        if is_dm && !self.settings.allowed_users.iter().any(|u| u == &author_id) {
            return;
        }

        let Some(top) = command.data.options().into_iter().next() else {
            return;
        };
        let ResolvedValue::SubCommand(sub_opts) = top.value else {
            return;
        };
        let channel_id = command.channel_id.to_string();

        match top.name {
            "help" => respond_ephemeral(&ctx, &command, HELP_TEXT).await,
            "reset" => {
                self.sessions.reset(&channel_id).await;
                mark_channel_inactive(&self.status_reply, &channel_id).await;
                respond_ephemeral(&ctx, &command, "Reset.").await;
            }
            "store" => {
                let text = sub_opts.iter().find_map(|o| match &o.value {
                    ResolvedValue::String(s) if o.name == "text" => Some(s.to_string()),
                    _ => None,
                });
                let Some(text) = text else {
                    respond_ephemeral(&ctx, &command, "Missing text.").await;
                    return;
                };
                let target = crate::discord::channels::resolve_target(&self.settings, &channel_id, is_dm);
                match crate::discord::store_direct::store_memory(&self.hivemind_bin, &text, &target).await {
                    Ok(()) => {
                        mark_channel_active(&self.status_reply, &channel_id).await;
                        respond_ephemeral(&ctx, &command, "Stored.").await;
                    }
                    Err(e) => {
                        respond_ephemeral(
                            &ctx,
                            &command,
                            &format!("hivemind discord failed to store that: {e}"),
                        )
                        .await;
                    }
                }
            }
            _ => {}
        }
    }
}

pub async fn run(settings: DiscordSettings, agent: AgentSettings, hivemind_bin: String) -> Result<()> {
    let token = tokio::task::spawn_blocking({
        let application_id = settings.application_id.clone();
        move || KeyringTokenStore.load(&application_id)
    })
    .await??
    .ok_or_else(|| anyhow::anyhow!("no saved bot token — run `hivemind discord login` first"))?;

    let _pid_guard = write_pidfile()?;

    let permission_gate = settings
        .permission_gate
        .as_deref()
        .map(crate::discord::daemon::parse_permission_gate)
        .transpose()
        .map_err(|e| anyhow::anyhow!(e))?;

    let status_reply = Arc::new(Mutex::new(StatusReply {
        logged_in: true,
        application_id: settings.application_id.clone(),
        sync_state: "connecting".to_string(),
        last_sync_at: None,
        channels: settings
            .channels
            .iter()
            .map(|c| ChannelStatus {
                channel_id: c.channel_id.clone(),
                alias: c.alias.clone(),
                active_session: false,
                last_active_at: None,
            })
            .collect(),
    }));
    let socket_status = status_reply.clone();
    let socket_path = crate::discord::status::socket_path();
    tokio::spawn(async move {
        if let Err(e) = crate::discord::status::serve_status(&socket_path, socket_status).await {
            tracing::warn!("status socket exited: {e:#}");
        }
    });

    let sessions = crate::discord::session::SessionMap::new(std::time::Duration::from_secs(
        settings.session_ttl_seconds,
    ));

    let handler = Handler {
        settings: Arc::new(settings),
        agent: Arc::new(agent),
        hivemind_bin: Arc::new(hivemind_bin),
        sessions,
        status_reply,
        permission_gate,
    };

    let intents = GatewayIntents::GUILD_MESSAGES
        | GatewayIntents::DIRECT_MESSAGES
        | GatewayIntents::MESSAGE_CONTENT;
    let mut client = serenity::Client::builder(&token, intents)
        .event_handler(handler)
        .await?;
    client.start().await?;
    Ok(())
}

/// Sends a text message to the given user's DM channel, opening one if
/// needed. Used for one-off connectivity checks (`hivemind discord send`)
/// independent of the daemon's gateway connection.
pub async fn send_direct_message(
    settings: &DiscordSettings,
    to_user_id: &str,
    message: &str,
) -> Result<()> {
    let token = tokio::task::spawn_blocking({
        let application_id = settings.application_id.clone();
        move || KeyringTokenStore.load(&application_id)
    })
    .await??
    .ok_or_else(|| anyhow::anyhow!("no saved bot token — run `hivemind discord login` first"))?;

    let http = serenity::http::Http::new(&token);
    let user_id: serenity::model::id::UserId = to_user_id
        .parse::<u64>()
        .map_err(|e| anyhow::anyhow!("invalid Discord user id {to_user_id:?}: {e}"))?
        .into();
    let dm_channel = user_id.create_dm_channel(&http).await?;
    dm_channel.say(&http, message).await?;
    Ok(())
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
