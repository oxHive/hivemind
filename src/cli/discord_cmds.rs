use anyhow::Result;
use std::io::Write as _;

// ── discord ──────────────────────────────────────────────────────────────

pub fn cmd_discord_login() -> Result<()> {
    print!("Bot token (from the Discord Developer Portal): ");
    std::io::stdout().flush()?;
    let bot_token = rpassword::prompt_password("")?;
    let bot_token = bot_token.trim().to_string();

    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?
        .block_on(async {
            let http = serenity::http::Http::new(&bot_token);
            let current_user = http
                .get_current_user()
                .await
                .map_err(|e| anyhow::anyhow!("token rejected by Discord: {e}"))?;
            let application_id = current_user.id.to_string();

            let store = crate::discord::token_store::KeyringTokenStore;
            crate::discord::login::persist_login(
                &application_id,
                &bot_token,
                &store,
                &crate::config::global_config_path(),
            )?;
            println!(
                "Logged in as {} (application id {application_id}).",
                current_user.name
            );
            println!("Token saved to the OS keyring. Run `hivemind discord run` to start the bot.");
            anyhow::Ok(())
        })
}

pub fn cmd_discord_status() -> Result<()> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?
        .block_on(async {
            let socket_path = crate::discord::status::socket_path();
            match crate::discord::status::query_status(&socket_path).await {
                Ok(reply) => {
                    println!("logged_in:      {}", reply.logged_in);
                    println!("application_id: {}", reply.application_id);
                    println!("sync_state:     {}", reply.sync_state);
                    if let Some(t) = &reply.last_sync_at {
                        println!("last_sync:      {t}");
                    }
                    if reply.channels.is_empty() {
                        println!("channels:       (none)");
                    } else {
                        println!("channels:");
                        for channel in &reply.channels {
                            let label = channel.alias.as_deref().unwrap_or(&channel.channel_id);
                            let session = if channel.active_session {
                                "active session"
                            } else {
                                "no active session"
                            };
                            println!("  {label}  ({session})");
                        }
                    }
                    Ok(())
                }
                Err(crate::discord::status::QueryError::NotRunning) => {
                    println!("hivemind discord is not running.");
                    println!("Start it with: hivemind discord run");
                    Ok(())
                }
                Err(crate::discord::status::QueryError::Protocol(msg)) => {
                    println!(
                        "hivemind discord appears to be running but returned invalid status data: {msg}"
                    );
                    Ok(())
                }
            }
        })
}
