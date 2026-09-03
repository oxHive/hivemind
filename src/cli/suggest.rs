use anyhow::{Context, Result, anyhow};
use clap::Subcommand;
use serde_json::json;

use super::common::{block_on, print_json};

#[derive(Subcommand)]
pub enum SuggestAction {
    /// Start an AI-assisted session that proposes edges between memories
    /// (requires `hivemind up` to be running)
    Start,
    /// Show the current suggest session's phase and pending suggestions
    Status {
        #[arg(long)]
        json: bool,
    },
    /// Send feedback on a suggested edge, asking the agent to revise it
    Revise { edge_id: String, feedback: String },
    /// End the current suggest session
    End,
}

fn api_base() -> Result<String> {
    let settings = crate::config::load_server_settings(&crate::config::global_config_path())?;
    Ok(settings.api_url)
}

fn not_running_hint(e: reqwest::Error) -> anyhow::Error {
    if e.is_connect() {
        anyhow!("could not reach the HiveMind server — start it first with `hivemind up`")
    } else {
        anyhow::Error::new(e)
    }
}

pub fn cmd_suggest(action: SuggestAction) -> Result<()> {
    match action {
        SuggestAction::Start => block_on(async {
            let base = api_base()?;
            let client = reqwest::Client::new();
            let resp = client
                .post(format!("{base}/api/v1/suggest-sessions"))
                .send()
                .await
                .map_err(not_running_hint)?;
            if !resp.status().is_success() {
                let status = resp.status();
                let text = resp.text().await.unwrap_or_default();
                return Err(anyhow!("start failed ({status}): {text}"));
            }
            println!("suggest session started — analyzing memories in the background");
            println!("check progress with: hivemind suggest status");
            Ok(())
        }),
        SuggestAction::Status { json } => block_on(async {
            let base = api_base()?;
            let client = reqwest::Client::new();
            let resp = client
                .get(format!("{base}/api/v1/suggest-sessions/current"))
                .send()
                .await
                .map_err(not_running_hint)?
                .error_for_status()
                .context("fetching suggest session status")?;
            let status: serde_json::Value = resp.json().await?;
            if json {
                print_json(&status);
            } else {
                println!(
                    "active: {}",
                    status
                        .get("active")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false)
                );
                println!(
                    "phase:  {}",
                    status
                        .get("phase")
                        .and_then(|v| v.as_str())
                        .unwrap_or("idle")
                );
                if let Some(edge) = status.get("revising_edge_id").and_then(|v| v.as_str()) {
                    println!("revising: {edge}");
                }
                if let Some(queued) = status.get("queued_edge_ids").and_then(|v| v.as_array())
                    && !queued.is_empty()
                {
                    println!("queued: {}", queued.len());
                }
                println!();
                println!("review pending suggestions with: hivemind edge list --status pending");
            }
            Ok(())
        }),
        SuggestAction::Revise { edge_id, feedback } => block_on(async {
            let base = api_base()?;
            let client = reqwest::Client::new();
            let resp = client
                .post(format!("{base}/api/v1/suggest-sessions/current/revise"))
                .json(&json!({ "edge_id": edge_id, "feedback": feedback }))
                .send()
                .await
                .map_err(not_running_hint)?;
            if !resp.status().is_success() {
                let status = resp.status();
                let text = resp.text().await.unwrap_or_default();
                return Err(anyhow!("revise failed ({status}): {text}"));
            }
            println!("revision queued for {edge_id}");
            Ok(())
        }),
        SuggestAction::End => block_on(async {
            let base = api_base()?;
            let client = reqwest::Client::new();
            client
                .delete(format!("{base}/api/v1/suggest-sessions/current"))
                .send()
                .await
                .map_err(not_running_hint)?;
            println!("suggest session ended");
            Ok(())
        }),
    }
}
