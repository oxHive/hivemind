use crate::config::AgentSettings;
use serde_json::{Value, json};
use std::time::Duration;

const TURN_TIMEOUT: Duration = Duration::from_secs(180);

#[derive(Debug)]
pub struct TurnResult {
    pub reply_text: String,
    pub session_id: String,
}

pub async fn run_turn(
    agent: &AgentSettings,
    hivemind_bin: &str,
    prompt: &str,
    resume: Option<&str>,
) -> Result<TurnResult, String> {
    let command_name = std::path::Path::new(&agent.command)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(&agent.command);
    if command_name == "opencode" {
        run_opencode_turn(agent, prompt, resume).await
    } else {
        run_claude_turn(agent, hivemind_bin, prompt, resume).await
    }
}

async fn run_claude_turn(
    agent: &AgentSettings,
    hivemind_bin: &str,
    prompt: &str,
    resume: Option<&str>,
) -> Result<TurnResult, String> {
    let mcp_config = json!({
        "mcpServers": { "hivemind": { "command": hivemind_bin, "args": [] } }
    })
    .to_string();
    let mut cmd = tokio::process::Command::new(&agent.command);
    cmd.args(&agent.args);
    if let Some(id) = resume {
        cmd.arg("--resume").arg(id);
    }
    cmd.arg("-p")
        .arg(prompt)
        .arg("--output-format")
        .arg("json")
        .arg("--mcp-config")
        .arg(&mcp_config)
        .arg("--strict-mcp-config")
        .arg("--allowedTools")
        .arg("mcp__hivemind__memory_store,mcp__hivemind__memory_recall,mcp__hivemind__memory_search,mcp__hivemind__memory_update")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true);
    let out = spawn_and_wait(cmd).await?;
    let v: Value = serde_json::from_str(&out).map_err(|e| format!("unparseable agent output: {e}"))?;
    let session_id = v
        .get("session_id")
        .and_then(|s| s.as_str())
        .ok_or_else(|| "agent output missing session_id".to_string())?
        .to_string();
    let reply_text = v
        .get("result")
        .and_then(|s| s.as_str())
        .unwrap_or_default()
        .to_string();
    Ok(TurnResult { reply_text, session_id })
}

async fn run_opencode_turn(
    agent: &AgentSettings,
    prompt: &str,
    resume: Option<&str>,
) -> Result<TurnResult, String> {
    let mut cmd = tokio::process::Command::new(&agent.command);
    cmd.args(&agent.args).arg("run").arg(prompt);
    if let Some(id) = resume {
        cmd.arg("-s").arg(id);
    }
    cmd.arg("--agent")
        .arg("hivemind-bot")
        .arg("--format")
        .arg("json")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true);
    let out = spawn_and_wait(cmd).await?;
    let v: Value = serde_json::from_str(&out).map_err(|e| format!("unparseable agent output: {e}"))?;
    let session_id = v
        .get("session_id")
        .and_then(|s| s.as_str())
        .ok_or_else(|| "agent output missing session_id".to_string())?
        .to_string();
    let reply_text = v
        .get("result")
        .and_then(|s| s.as_str())
        .unwrap_or_default()
        .to_string();
    Ok(TurnResult { reply_text, session_id })
}

async fn spawn_and_wait(mut cmd: tokio::process::Command) -> Result<String, String> {
    let child = cmd
        .spawn()
        .map_err(|e| format!("failed to spawn agent: {e}"))?;
    let out = tokio::time::timeout(TURN_TIMEOUT, child.wait_with_output())
        .await
        .map_err(|_| format!("agent timed out after {}s", TURN_TIMEOUT.as_secs()))?
        .map_err(|e| e.to_string())?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        return Err(format!("agent exited with {}: {}", out.status, stderr.trim()));
    }
    let stdout = String::from_utf8_lossy(&out.stdout);
    stdout
        .lines()
        .rev()
        .find(|l| !l.trim().is_empty())
        .map(str::to_string)
        .ok_or_else(|| "agent produced no output".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    fn write_stub_claude_agent(dir: &std::path::Path) -> String {
        let script = dir.join("stub-claude.sh");
        std::fs::write(
            &script,
            "#!/bin/sh\nprintf '%s\\n' \"$*\" >> \"$(dirname \"$0\")/args.log\"\n\
             echo '{\"type\":\"result\",\"session_id\":\"stub-sess-1\",\"result\":\"stored it\",\"is_error\":false}'\n",
        )
        .unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        script.to_string_lossy().into_owned()
    }

    fn write_stub_opencode_agent(dir: &std::path::Path) -> String {
        let script = dir.join("stub-opencode.sh");
        std::fs::write(
            &script,
            "#!/bin/sh\nprintf '%s\\n' \"$*\" >> \"$(dirname \"$0\")/args.log\"\n\
             echo '{\"session_id\":\"stub-sess-2\",\"result\":\"stored it\"}'\n",
        )
        .unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        script.to_string_lossy().into_owned()
    }

    #[tokio::test]
    async fn claude_turn_passes_isolation_flags_and_stdio_mcp_config() {
        let dir = tempfile::tempdir().unwrap();
        let script = write_stub_claude_agent(dir.path());
        let agent = AgentSettings { command: script, args: vec![] };
        let result = run_turn(&agent, "/usr/local/bin/hivemind", "remember X", None)
            .await
            .unwrap();
        assert_eq!(result.session_id, "stub-sess-1");
        assert_eq!(result.reply_text, "stored it");
        let log = std::fs::read_to_string(dir.path().join("args.log")).unwrap();
        assert!(log.contains("-p"));
        assert!(log.contains("remember X"));
        assert!(log.contains("--output-format json"));
        assert!(log.contains("--strict-mcp-config"));
        assert!(log.contains("--allowedTools"));
        assert!(log.contains("mcp__hivemind__memory_store"));
        assert!(log.contains("\"command\":\"/usr/local/bin/hivemind\""), "mcp-config must point at hivemind in stdio mode, not an HTTP url");
        assert!(!log.contains("--resume"), "first turn must not pass --resume");
    }

    #[tokio::test]
    async fn claude_turn_resumes_with_the_given_session_id() {
        let dir = tempfile::tempdir().unwrap();
        let script = write_stub_claude_agent(dir.path());
        let agent = AgentSettings { command: script, args: vec![] };
        run_turn(&agent, "/usr/local/bin/hivemind", "again", Some("prior-session"))
            .await
            .unwrap();
        let log = std::fs::read_to_string(dir.path().join("args.log")).unwrap();
        assert!(log.contains("--resume prior-session"));
    }

    #[tokio::test]
    async fn opencode_turn_uses_run_and_agent_profile_flags() {
        let dir = tempfile::tempdir().unwrap();
        let script = write_stub_opencode_agent(dir.path());
        let agent = AgentSettings {
            command: "opencode".to_string(),
            args: vec![],
        };
        // Point at the stub via a wrapper AgentSettings whose command is the
        // stub script but whose *name* still needs to read as "opencode" for
        // dispatch — dispatch is keyed on the configured command's file stem.
        let renamed = dir.path().join("opencode");
        std::fs::copy(&script, &renamed).unwrap();
        std::fs::set_permissions(&renamed, std::fs::Permissions::from_mode(0o755)).unwrap();
        let agent = AgentSettings {
            command: renamed.to_string_lossy().into_owned(),
            ..agent
        };
        let result = run_turn(&agent, "/usr/local/bin/hivemind", "remember X", Some("sess-1"))
            .await
            .unwrap();
        assert_eq!(result.session_id, "stub-sess-2");
        let log = std::fs::read_to_string(dir.path().join("args.log")).unwrap();
        assert!(log.contains("run"));
        assert!(log.contains("--agent hivemind-bot"));
        assert!(log.contains("-s sess-1"));
        assert!(log.contains("--format json"));
    }

    #[tokio::test]
    async fn nonzero_exit_is_reported_as_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let script = dir.path().join("bad-agent.sh");
        std::fs::write(&script, "#!/bin/sh\necho 'boom' >&2\nexit 1\n").unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        let agent = AgentSettings {
            command: script.to_string_lossy().into_owned(),
            args: vec![],
        };
        let err = run_turn(&agent, "/usr/local/bin/hivemind", "hi", None)
            .await
            .unwrap_err();
        assert!(err.contains("boom"));
    }
}
