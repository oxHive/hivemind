use anyhow::Result;
use clap::{Parser, Subcommand};
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

#[derive(Parser)]
#[command(
    name = "hivemind",
    version,
    about = "HiveMind — persistent memory for AI coding agents"
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Subcommand)]
pub enum Command {
    /// Scaffold .hivemind.toml + CLAUDE.md integration for this project
    Init,
    /// Show config and preview what session start will inject
    Status {
        /// Force plain-text output even on a real terminal
        #[arg(long)]
        plain: bool,
    },
    /// Start the HTTP server: MCP at /mcp, REST at /api/v1, plus the dashboard
    Up {
        /// Serve only MCP + REST API — no dashboard
        #[arg(long)]
        headless: bool,
        /// Force plain log output even on a real terminal
        #[arg(long)]
        plain: bool,
    },
    /// Serve the dashboard only, attached to an already-running server
    Dashboard {
        /// Open the dashboard in a browser
        #[arg(long)]
        open: bool,
    },
    /// Manage MCP client integrations
    Mcp {
        #[command(subcommand)]
        action: McpAction,
    },
    /// Manage HiveMind as a background service
    Service {
        #[command(subcommand)]
        action: ServiceAction,
    },
    /// Matrix chat interface: capture/recall HiveMind memories from a room or DM
    Matrix {
        #[command(subcommand)]
        action: MatrixAction,
    },
    /// Migrate the database from the legacy ~/.hivemind/ path to XDG data dir
    Migrate,
    /// Print the session-start memory context for the current project (for hooks and scripts)
    SessionStart {
        /// Emit machine-readable JSON instead of tagged text
        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand)]
pub enum ServiceAction {
    /// Install and enable HiveMind as a user-level background service
    Install {
        /// Also serve the dashboard from the background service
        #[arg(long)]
        dashboard: bool,
        /// Also install the Matrix bot unit (requires `hivemind matrix login` first)
        #[arg(long)]
        matrix: bool,
    },
    /// Stop and remove the HiveMind background service
    Uninstall,
    /// Show the status of the HiveMind background service
    Status,
}

#[derive(Subcommand)]
pub enum MatrixAction {
    /// Log into a Matrix account once; persists the session to the OS keyring
    Login,
    /// Run the Matrix bot daemon (requires `hivemind matrix login` first)
    Run {
        /// Print verbose connection/message logs to stderr
        #[arg(long)]
        debug: bool,
    },
    /// Show whether the daemon is running and its sync/session state
    Status,
    /// Send a one-off DM to a user (connectivity smoke test, no daemon needed)
    Send {
        /// Recipient's Matrix user ID (e.g. @oxgrad:matrix.org)
        user_id: String,
        /// Message text to send
        message: String,
    },
}

#[derive(Subcommand)]
pub enum McpAction {
    /// Register HiveMind as an MCP server in a supported AI coding client
    Install {
        /// Client to register with: claude, opencode, kimi, codex, cursor, windsurf
        client: String,
    },
}

fn with_spinner<T>(msg: &str, f: impl FnOnce() -> T) -> T {
    let done = Arc::new(AtomicBool::new(false));
    let done2 = done.clone();
    let label = msg.to_string();
    let width = msg.len() + 4;
    let handle = std::thread::spawn(move || {
        let frames = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];
        let mut i = 0usize;
        while !done2.load(Ordering::Relaxed) {
            print!("\r  {} {label}", frames[i % frames.len()]);
            let _ = std::io::stdout().flush();
            std::thread::sleep(std::time::Duration::from_millis(80));
            i += 1;
        }
        print!("\r{}\r", " ".repeat(width));
        let _ = std::io::stdout().flush();
    });
    let result = f();
    done.store(true, Ordering::Relaxed);
    let _ = handle.join();
    result
}

pub fn cmd_init() -> Result<()> {
    let cwd = std::env::current_dir()?;
    let home = home_dir();
    let report = scaffold(&cwd, &home, &crate::config::global_config_dir())?;
    for (path, status) in &report {
        println!("  {status:7}  {}", path.display());
    }
    println!();

    let registered_clients = with_spinner("checking registered MCP clients...", || {
        detect_registered_clients(&home)
    });
    match registered_clients {
        registered if registered.is_empty() => {
            println!("HiveMind initialized.");
            println!();
            println!("Next steps:");
            println!("  1. Register with your AI coding client (run once, not per project):");
            println!("       hivemind mcp install claude      # Claude Code");
            println!("       hivemind mcp install cursor      # Cursor");
            println!("       hivemind mcp install windsurf    # Windsurf");
            println!("       hivemind mcp install opencode    # OpenCode");
            println!("       hivemind mcp install kimi        # Kimi Code CLI");
            println!("       hivemind mcp install codex       # OpenAI Codex CLI");
            println!("  2. Start the server:  hivemind service install  (or: hivemind up)");
            println!("  3. Open a new session in your AI client. Memory hooks are now active.");
        }
        registered => {
            let list = registered.join(", ");
            println!("HiveMind initialized.");
            println!("MCP client already registered: {list}");
            println!();
            println!("Next steps:");
            println!("  1. Start the server:  hivemind service install  (or: hivemind up)");
            println!("  2. Open a new session. Memory hooks are now active.");
        }
    }

    Ok(())
}

/// Returns names of AI clients that already have HiveMind registered.
fn detect_registered_clients(home: &Path) -> Vec<&'static str> {
    let mut found = Vec::new();

    // Claude Code: check config files directly.
    // `claude mcp list` subprocess only shows file-registered servers, not OAuth
    // sessions, so it produces false negatives when the server isn't running yet.
    let claude_dot = home.join(".claude");
    let claude_ok = [
        // stdio/HTTP servers added via `claude mcp add`
        claude_dot.join("mcp.json"),
        // user-scoped settings (mcpServers key)
        claude_dot.join("settings.json"),
        // user-scope registrations from `claude mcp add --scope user`
        home.join(".claude.json"),
    ]
    .iter()
    .any(|p| {
        p.exists()
            && std::fs::read_to_string(p)
                .map(|s| s.contains("hivemind"))
                .unwrap_or(false)
    });
    if claude_ok {
        found.push("claude");
    }

    // File-based clients: just grep for "hivemind" in their config
    let file_clients: &[(&str, &[&str])] = &[
        ("cursor", &[".cursor", "mcp.json"]),
        ("windsurf", &[".codeium", "windsurf", "mcp_config.json"]),
        ("kimi", &[".kimi", "mcp.json"]),
    ];
    for (name, parts) in file_clients {
        let path = parts.iter().fold(home.to_path_buf(), |p, s| p.join(s));
        if path.exists()
            && let Ok(contents) = std::fs::read_to_string(&path)
            && contents.contains("hivemind")
        {
            found.push(name);
        }
    }

    // OpenCode global config
    let xdg_config = std::env::var_os("XDG_CONFIG_HOME")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| home.join(".config"));
    let opencode_cfg = xdg_config.join("opencode").join("opencode.json");
    if opencode_cfg.exists()
        && let Ok(contents) = std::fs::read_to_string(&opencode_cfg)
        && contents.contains("hivemind")
    {
        found.push("opencode");
    }

    // Codex: TOML config
    let codex_cfg = home.join(".codex").join("config.toml");
    if codex_cfg.exists()
        && let Ok(contents) = std::fs::read_to_string(&codex_cfg)
        && contents.contains("[mcp_servers.hivemind]")
    {
        found.push("codex");
    }

    found
}

fn home_dir() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

/// Create project + global config files and CLAUDE.md integration.
/// Returns (path, "created"|"exists") for each target. Idempotent.
pub fn scaffold(
    project_root: &Path,
    home: &Path,
    config_dir: &Path,
) -> Result<Vec<(PathBuf, &'static str)>> {
    let project_name = project_root
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "project".to_string());

    let report = vec![
        write_if_absent(
            &project_root.join(".hivemind.toml"),
            &project_toml(&project_name),
        )?,
        write_if_absent(&project_root.join(".hivemind.local.toml"), LOCAL_TOML)?,
        ensure_line(&project_root.join(".gitignore"), ".hivemind.local.toml")?,
        write_if_absent(
            &project_root.join("CLAUDE.md"),
            &project_claude_md(&project_name),
        )?,
        append_block_if_absent(
            &home.join(".claude").join("CLAUDE.md"),
            GLOBAL_CLAUDE_MARKER,
            GLOBAL_CLAUDE_BLOCK,
        )?,
        write_if_absent(&config_dir.join("config.toml"), GLOBAL_CONFIG)?,
        ensure_claude_settings_hook(project_root)?,
    ];

    Ok(report)
}

/// Write `contents` to `path` atomically: write to a sibling temp file, then
/// rename over the destination. This protects an existing user file (e.g. a
/// customized ~/.claude/CLAUDE.md) from truncation if the process is interrupted
/// mid-write. (tempfile is dev-only, so the temp file is created manually.)
fn write_atomic(path: &Path, contents: &str) -> Result<()> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    std::fs::create_dir_all(parent)?;
    let file_name = path.file_name().and_then(|s| s.to_str()).unwrap_or("file");
    let tmp = parent.join(format!(".{file_name}.hivemind-tmp"));
    std::fs::write(&tmp, contents)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

fn write_if_absent(path: &Path, contents: &str) -> Result<(PathBuf, &'static str)> {
    if path.exists() {
        return Ok((path.to_path_buf(), "exists"));
    }
    write_atomic(path, contents)?;
    Ok((path.to_path_buf(), "created"))
}

/// Ensure `line` is present in the file (create the file if needed).
fn ensure_line(path: &Path, line: &str) -> Result<(PathBuf, &'static str)> {
    let existing = std::fs::read_to_string(path).unwrap_or_default();
    if existing.lines().any(|l| l.trim() == line) {
        return Ok((path.to_path_buf(), "exists"));
    }
    let mut body = existing;
    if !body.is_empty() && !body.ends_with('\n') {
        body.push('\n');
    }
    body.push_str(line);
    body.push('\n');
    write_atomic(path, &body)?;
    Ok((path.to_path_buf(), "created"))
}

/// Append `block` to the file unless `marker` is already present.
fn append_block_if_absent(
    path: &Path,
    marker: &str,
    block: &str,
) -> Result<(PathBuf, &'static str)> {
    let existing = std::fs::read_to_string(path).unwrap_or_default();
    if existing.contains(marker) {
        return Ok((path.to_path_buf(), "exists"));
    }
    let mut body = existing;
    if !body.is_empty() && !body.ends_with('\n') {
        body.push('\n');
    }
    if !body.is_empty() {
        body.push('\n');
    }
    body.push_str(block);
    write_atomic(path, &body)?;
    Ok((path.to_path_buf(), "created"))
}

/// Merge a SessionStart hook running `hivemind session-start` into the
/// project's .claude/settings.json, preserving all existing content.
/// If the file exists but is not valid JSON, returns an error instead of
/// overwriting it, so a malformed user file is never destroyed.
fn ensure_claude_settings_hook(project_root: &Path) -> Result<(PathBuf, &'static str)> {
    use anyhow::Context as _;

    let path = project_root.join(".claude").join("settings.json");
    let existing = std::fs::read_to_string(&path).unwrap_or_else(|_| "{}".to_string());
    if existing.contains("hivemind session-start") {
        return Ok((path, "exists"));
    }
    let mut root: serde_json::Value = serde_json::from_str(&existing).with_context(|| {
        format!(
            "{} is not valid JSON; fix or remove it, then re-run hivemind init",
            path.display()
        )
    })?;
    let obj = root
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("{} root is not a JSON object", path.display()))?;
    let hooks = obj.entry("hooks").or_insert(serde_json::json!({}));
    let session_start = hooks
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("\"hooks\" is not a JSON object"))?
        .entry("SessionStart")
        .or_insert(serde_json::json!([]));
    session_start
        .as_array_mut()
        .ok_or_else(|| anyhow::anyhow!("\"SessionStart\" is not a JSON array"))?
        .push(serde_json::json!({
            "hooks": [{ "type": "command", "command": "hivemind session-start" }]
        }));
    std::fs::create_dir_all(path.parent().unwrap())?;
    write_atomic(&path, &serde_json::to_string_pretty(&root)?)?;
    Ok((path, "created"))
}

fn project_toml(name: &str) -> String {
    format!(
        "[project]\n\
         name = \"{name}\"\n\
         layer = \"workspace\"\n\
         description = \"\"  # fill in: one-line description of this project\n\
         \n\
         [hooks.on_session_start]\n\
         max_tokens = 2000\n\
         recalls = [\n\
         \x20 # Exact memory titles to auto-load at session start, e.g.:\n\
         \x20 # \"golang preferences\",\n\
         \x20 # \"project/{name}\",\n\
         ]\n"
    )
}

const LOCAL_TOML: &str = "# Personal, gitignored recalls — additive on top of .hivemind.toml.\n\
# Teammates do not see these. max_tokens here is ADDED to the team budget.\n\
[hooks.on_session_start]\n\
recalls = []\n\
max_tokens = 0\n";

fn project_claude_md(name: &str) -> String {
    format!(
        "# HiveMind — {name}\n\n\
         Load project context on session start per .hivemind.toml.\n\
         Suggest storing any new architectural decisions made during this session.\n"
    )
}

const GLOBAL_CONFIG: &str = "[defaults]\n\
max_inject_tokens = 2000\n\
suggest_store = true\n\
\n\
[sync]\n\
enabled = false\n\
remote_url = \"\" # Oxhive hosted: https://sync.oxhive.dev  or  self-hosted sqld: http://your-server:8080\n\
api_key = \"\"    # Oxhive account key, or sqld auth token (leave empty if sqld has no auth)\n\
interval_seconds = 300\n\
sync_on_store = true\n\
sync_on_startup = true\n";

const GLOBAL_CLAUDE_MARKER: &str = "# HiveMind Memory System";

const GLOBAL_CLAUDE_BLOCK: &str = "# HiveMind Memory System

You have access to HiveMind via MCP tools: memory_store, memory_recall,
memory_search, memory_update, memory_delete, memory_store_edge, hivemind_session_start.

At the start of every session, before doing anything else:

1. Check if .hivemind.toml exists in the project root.
2. If it exists, call `hivemind_session_start` with the project root path immediately.
3. Incorporate the returned context silently — do not narrate it.

After calling hivemind_session_start:

- If budget.truncated is true, mention once: \"Some memory entries were skipped
  due to token budget. Run `hivemind status` to review.\"
- If any skipped entry has reason not_found, mention once which recalls were not
  found so the user can check their .hivemind.toml.
- Then proceed normally.

If .hivemind.toml does not exist:

- Do not call hivemind_session_start.
- Tools remain available on demand.
- If the user seems to be starting a new project, suggest: \"Run `hivemind init`
  to set up memory hooks for this project.\"

If a <hivemind-context> block is already present in the session context (injected
by the SessionStart hook), do NOT call hivemind_session_start again; use the
injected context directly.

## Suggest storing — never auto-store

When the user shares something worth persisting (preferences, project context,
design decisions), suggest: \"That seems worth remembering — should I store it?\"
Wait for explicit confirmation before calling memory_store.
";

// ── service management ────────────────────────────────────────────────────────

pub fn cmd_service_install(dashboard: bool, matrix: bool) -> Result<()> {
    #[cfg(target_os = "macos")]
    return service_install_macos(dashboard, matrix);
    #[cfg(target_os = "linux")]
    return service_install_linux(dashboard, matrix);
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    anyhow::bail!("hivemind service install is only supported on Linux and macOS");
}

pub fn cmd_service_uninstall() -> Result<()> {
    #[cfg(target_os = "macos")]
    return service_uninstall_macos();
    #[cfg(target_os = "linux")]
    return service_uninstall_linux();
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    anyhow::bail!("hivemind service uninstall is only supported on Linux and macOS");
}

pub fn cmd_service_status() -> Result<()> {
    #[cfg(target_os = "macos")]
    return service_status_macos();
    #[cfg(target_os = "linux")]
    return service_status_linux();
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    anyhow::bail!("hivemind service status is only supported on Linux and macOS");
}

// ── Linux / systemd user unit ─────────────────────────────────────────────────

#[cfg(target_os = "linux")]
fn systemd_unit_dir() -> PathBuf {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| home_dir().join(".config"))
        .join("systemd")
        .join("user")
}

#[cfg(target_os = "linux")]
fn systemd_unit_path(unit_name: &str) -> PathBuf {
    systemd_unit_dir().join(format!("{unit_name}.service"))
}

#[cfg(target_os = "linux")]
fn systemd_unit_content(description: &str, exe: &Path, exec_args: &[&str]) -> String {
    let mut exec = exe.display().to_string();
    for arg in exec_args {
        exec.push(' ');
        exec.push_str(arg);
    }
    format!(
        "[Unit]\n\
         Description={description}\n\
         After=network.target\n\
         \n\
         [Service]\n\
         Type=simple\n\
         ExecStart={exec}\n\
         Restart=on-failure\n\
         RestartSec=5\n\
         \n\
         [Install]\n\
         WantedBy=default.target\n"
    )
}

#[cfg(target_os = "linux")]
fn service_install_unit_linux(
    unit_name: &str,
    description: &str,
    exec_args: &[&str],
) -> Result<()> {
    let exe = std::env::current_exe()?;
    let unit = systemd_unit_content(description, &exe, exec_args);

    let unit_path = systemd_unit_path(unit_name);
    std::fs::create_dir_all(unit_path.parent().unwrap())?;
    std::fs::write(&unit_path, &unit)?;
    println!("Unit file written: {}", unit_path.display());

    // daemon-reload so systemd sees the new unit.
    let reload = std::process::Command::new("systemctl")
        .args(["--user", "daemon-reload"])
        .status();
    match reload {
        Ok(s) if s.success() => {}
        _ => {
            println!("Warning: systemctl --user daemon-reload failed — run it manually.");
        }
    }

    let enable = std::process::Command::new("systemctl")
        .args(["--user", "enable", "--now", unit_name])
        .status();
    match enable {
        Ok(s) if s.success() => {
            println!("{unit_name} service enabled and started.");
        }
        _ => {
            println!("Warning: could not enable/start {unit_name} automatically.");
            println!("Run: systemctl --user enable --now {unit_name}");
        }
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn service_uninstall_unit_linux(unit_name: &str) -> Result<()> {
    let _ = std::process::Command::new("systemctl")
        .args(["--user", "disable", "--now", unit_name])
        .status();

    let unit_path = systemd_unit_path(unit_name);
    if unit_path.exists() {
        std::fs::remove_file(&unit_path)?;
        println!("Removed: {}", unit_path.display());
    } else {
        println!("Unit file for {unit_name} not found — nothing to remove.");
    }

    let _ = std::process::Command::new("systemctl")
        .args(["--user", "daemon-reload"])
        .status();
    Ok(())
}

#[cfg(target_os = "linux")]
fn service_status_unit_linux(unit_name: &str) -> Result<()> {
    let status = std::process::Command::new("systemctl")
        .args(["--user", "status", unit_name])
        .status()?;
    if !status.success() {
        anyhow::bail!("{unit_name} is not running or not installed");
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn service_install_linux(dashboard: bool, matrix: bool) -> Result<()> {
    let (args, desc): (&[&str], &str) = if dashboard {
        (&["up"], "HiveMind server (API + dashboard)")
    } else {
        (&["up", "--headless"], "HiveMind server (API only)")
    };
    service_install_unit_linux("hivemind", desc, args)?;

    if matrix {
        let configured = crate::config::load_matrix_settings(&crate::config::global_config_path())
            .ok()
            .flatten()
            .is_some();
        if !configured {
            anyhow::bail!(
                "--matrix was passed but Matrix is not configured.\n\
                 Run `hivemind matrix login` first, then re-run `hivemind service install --matrix`."
            );
        }
        service_install_unit_linux(
            "hivemind-matrix",
            "HiveMind Matrix chat bot",
            &["matrix", "run"],
        )?;
    }

    println!();
    println!("HiveMind will now start automatically on login.");
    if dashboard {
        let port = crate::config::load_server_settings(&crate::config::global_config_path())
            .map(|s| s.dashboard_port)
            .unwrap_or(3457);
        println!("Dashboard: http://127.0.0.1:{port}");
    }
    println!("Check status: hivemind service status");
    Ok(())
}

#[cfg(target_os = "linux")]
fn service_uninstall_linux() -> Result<()> {
    service_uninstall_unit_linux("hivemind")?;
    if systemd_unit_path("hivemind-matrix").exists() {
        service_uninstall_unit_linux("hivemind-matrix")?;
    }

    println!("HiveMind service uninstalled.");
    Ok(())
}

#[cfg(target_os = "linux")]
fn service_status_linux() -> Result<()> {
    service_status_unit_linux("hivemind")?;
    if systemd_unit_path("hivemind-matrix").exists() {
        service_status_unit_linux("hivemind-matrix")?;
    }
    Ok(())
}

#[cfg(all(test, target_os = "linux"))]
mod matrix_service_tests {
    use super::*;

    #[test]
    fn systemd_unit_content_for_matrix_names_the_unit_and_subcommand() {
        let content = systemd_unit_content(
            "HiveMind Matrix chat bot",
            &std::path::PathBuf::from("/usr/local/bin/hivemind"),
            &["matrix", "run"],
        );
        assert!(content.contains("Description=HiveMind Matrix chat bot"));
        assert!(content.contains("ExecStart=/usr/local/bin/hivemind matrix run"));
        assert!(content.contains("WantedBy=default.target"));
    }

    #[test]
    fn systemd_unit_content_for_up_matches_existing_bare_invocation() {
        // Preserves exact current behavior: the `up` unit has always run the
        // bare binary (stdio MCP mode), zero args — not `up --headless` as
        // one might expect. Not this task's job to change that; just don't
        // silently break it while adding the parameterization.
        let content = systemd_unit_content(
            "HiveMind MCP memory server",
            &std::path::PathBuf::from("/usr/local/bin/hivemind"),
            &[],
        );
        assert!(content.contains("ExecStart=/usr/local/bin/hivemind\n"));
    }
}

// ── macOS / launchd ───────────────────────────────────────────────────────────

#[cfg(target_os = "macos")]
const LAUNCH_AGENT_LABEL: &str = "com.oxhive.hivemind";

#[cfg(target_os = "macos")]
const MATRIX_LAUNCH_AGENT_LABEL: &str = "com.oxhive.hivemind-matrix";

#[cfg(target_os = "macos")]
fn launch_agent_path(label: &str) -> PathBuf {
    home_dir()
        .join("Library")
        .join("LaunchAgents")
        .join(format!("{label}.plist"))
}

#[cfg(target_os = "macos")]
fn launch_agent_plist_content(label: &str, exe: &Path, exec_args: &[&str]) -> String {
    let mut program_arguments = format!("<string>{}</string>\n", exe.display());
    for arg in exec_args {
        program_arguments.push_str("             <string>");
        program_arguments.push_str(arg);
        program_arguments.push_str("</string>\n");
    }
    let log_dir = home_dir().join("Library").join("Logs");
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \
         \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n\
         <plist version=\"1.0\">\n\
         <dict>\n\
           <key>Label</key>\n\
           <string>{label}</string>\n\
           <key>ProgramArguments</key>\n\
           <array>\n\
             {program_arguments}\
           </array>\n\
           <key>RunAtLoad</key>\n\
           <true/>\n\
           <key>KeepAlive</key>\n\
           <true/>\n\
           <key>StandardOutPath</key>\n\
           <string>{log_dir}/hivemind.log</string>\n\
           <key>StandardErrorPath</key>\n\
           <string>{log_dir}/hivemind.log</string>\n\
         </dict>\n\
         </plist>\n",
        log_dir = log_dir.display(),
    )
}

#[cfg(target_os = "macos")]
fn service_install_unit_macos(label: &str, exec_args: &[&str], description: &str) -> Result<()> {
    let exe = std::env::current_exe()?;
    let plist_path = launch_agent_path(label);
    let plist = launch_agent_plist_content(label, &exe, exec_args);

    std::fs::create_dir_all(plist_path.parent().unwrap())?;
    std::fs::write(&plist_path, &plist)?;
    println!("Plist written: {}", plist_path.display());

    let load = std::process::Command::new("launchctl")
        .args(["load", "-w", plist_path.to_str().unwrap()])
        .status();
    match load {
        Ok(s) if s.success() => {
            println!("{description} loaded and started.");
        }
        _ => {
            println!("Warning: launchctl load failed — run manually:");
            println!("  launchctl load -w {}", plist_path.display());
        }
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn service_uninstall_unit_macos(label: &str) -> Result<()> {
    let plist_path = launch_agent_path(label);

    let _ = std::process::Command::new("launchctl")
        .args(["unload", "-w", plist_path.to_str().unwrap()])
        .status();

    if plist_path.exists() {
        std::fs::remove_file(&plist_path)?;
        println!("Removed: {}", plist_path.display());
    } else {
        println!("Plist for {label} not found — nothing to remove.");
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn service_status_unit_macos(label: &str) -> Result<()> {
    let output = std::process::Command::new("launchctl")
        .args(["list", label])
        .output()?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    if output.status.success() && !stdout.trim().is_empty() {
        print!("{stdout}");
    } else {
        println!("{label} is not loaded.");
        println!("Run: hivemind service install");
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn service_install_macos(dashboard: bool, matrix: bool) -> Result<()> {
    let (args, desc): (&[&str], &str) = if dashboard {
        (&["up"], "HiveMind server (API + dashboard)")
    } else {
        (&["up", "--headless"], "HiveMind server (API only)")
    };
    service_install_unit_macos(LAUNCH_AGENT_LABEL, args, desc)?;

    if matrix {
        let configured = crate::config::load_matrix_settings(&crate::config::global_config_path())
            .ok()
            .flatten()
            .is_some();
        if !configured {
            anyhow::bail!(
                "--matrix was passed but Matrix is not configured.\n\
                 Run `hivemind matrix login` first, then re-run `hivemind service install --matrix`."
            );
        }
        service_install_unit_macos(
            MATRIX_LAUNCH_AGENT_LABEL,
            &["matrix", "run"],
            "HiveMind Matrix chat bot",
        )?;
    }

    println!();
    println!("HiveMind will now start automatically on login.");
    if dashboard {
        let port = crate::config::load_server_settings(&crate::config::global_config_path())
            .map(|s| s.dashboard_port)
            .unwrap_or(3457);
        println!("Dashboard: http://127.0.0.1:{port}");
    }
    println!("Logs: ~/Library/Logs/hivemind.log");
    println!("Check status: hivemind service status");
    Ok(())
}

#[cfg(target_os = "macos")]
fn service_uninstall_macos() -> Result<()> {
    service_uninstall_unit_macos(LAUNCH_AGENT_LABEL)?;
    if launch_agent_path(MATRIX_LAUNCH_AGENT_LABEL).exists() {
        service_uninstall_unit_macos(MATRIX_LAUNCH_AGENT_LABEL)?;
    }

    println!("HiveMind service uninstalled.");
    Ok(())
}

#[cfg(target_os = "macos")]
fn service_status_macos() -> Result<()> {
    service_status_unit_macos(LAUNCH_AGENT_LABEL)?;
    if launch_agent_path(MATRIX_LAUNCH_AGENT_LABEL).exists() {
        service_status_unit_macos(MATRIX_LAUNCH_AGENT_LABEL)?;
    }
    Ok(())
}

// ── mcp install ───────────────────────────────────────────────────────────────

fn exe_path() -> String {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.to_str().map(String::from))
        .unwrap_or_else(|| "hivemind".to_string())
}

// ── matrix ────────────────────────────────────────────────────────────────

pub fn cmd_matrix_login() -> Result<()> {
    print!("Homeserver URL [https://matrix.org]: ");
    std::io::stdout().flush()?;
    let mut homeserver_url = String::new();
    std::io::stdin().read_line(&mut homeserver_url)?;
    let homeserver_url = homeserver_url.trim();
    let homeserver_url = if homeserver_url.is_empty() {
        "https://matrix.org".to_string()
    } else {
        homeserver_url.to_string()
    };

    print!("User ID (e.g. @hivemind-bot:matrix.org): ");
    std::io::stdout().flush()?;
    let mut user_id = String::new();
    std::io::stdin().read_line(&mut user_id)?;
    let user_id = user_id.trim().to_string();

    let password = rpassword::prompt_password("Password: ")?;

    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?
        .block_on(async {
            let client = matrix_sdk::Client::builder()
                .homeserver_url(&homeserver_url)
                .sqlite_store(crate::db::xdg_data_dir().join("matrix-store"), None)
                .build()
                .await?;
            let response = client
                .matrix_auth()
                .login_username(&user_id, &password)
                .initial_device_display_name("HiveMind bot")
                .await?;
            drop(password);
            let session = client
                .matrix_auth()
                .session()
                .ok_or_else(|| anyhow::anyhow!("login succeeded but no session was created"))?;
            let session_json = serde_json::to_string(&session)?;
            let store = crate::matrix::keyring_store::KeyringSessionStore;
            crate::matrix::login::persist_login(
                &homeserver_url,
                &user_id,
                &session_json,
                &store,
                &crate::config::global_config_path(),
            )?;
            println!(
                "Logged in as {} (device {}).",
                response.user_id, response.device_id
            );
            println!(
                "Session saved to the OS keyring. Run `hivemind matrix run` to start the bot."
            );
            anyhow::Ok(())
        })
}

pub fn cmd_matrix_status() -> Result<()> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?
        .block_on(async {
            let socket_path = crate::matrix::status::socket_path();
            match crate::matrix::status::query_status(&socket_path).await {
                Ok(reply) => {
                    println!("logged_in:  {}", reply.logged_in);
                    println!("user_id:    {}", reply.user_id);
                    println!("sync_state: {}", reply.sync_state);
                    if let Some(t) = &reply.last_sync_at {
                        println!("last_sync:  {t}");
                    }
                    if reply.rooms.is_empty() {
                        println!("rooms:      (none)");
                    } else {
                        println!("rooms:");
                        for room in &reply.rooms {
                            let label = room.alias.as_deref().unwrap_or(&room.room_id);
                            let session = if room.active_session { "active session" } else { "no active session" };
                            println!("  {label}  ({session})");
                        }
                    }
                    Ok(())
                }
                Err(crate::matrix::status::QueryError::NotRunning) => {
                    println!("hivemind matrix is not running.");
                    println!("Start it with: hivemind matrix run");
                    Ok(())
                }
                Err(crate::matrix::status::QueryError::Protocol(msg)) => {
                    println!("hivemind matrix appears to be running but returned invalid status data: {msg}");
                    Ok(())
                }
            }
        })
}

pub fn cmd_mcp_install(client: &str) -> Result<()> {
    match client {
        "claude" => install_claude(),
        "opencode" => install_opencode(),
        "kimi" => install_kimi(),
        "codex" => install_codex(),
        "cursor" => install_cursor(),
        "windsurf" => install_windsurf(),
        other => anyhow::bail!(
            "unknown client \"{other}\" — supported: claude, opencode, kimi, codex, cursor, windsurf"
        ),
    }
}

fn install_claude() -> Result<()> {
    // Verify the claude CLI is available.
    let claude_check = std::process::Command::new("claude")
        .arg("--version")
        .output();
    if claude_check.is_err() {
        anyhow::bail!(
            "claude CLI not found in PATH\n\
             Install Claude Code first: https://claude.ai/download\n\
             Then re-run: hivemind mcp install claude"
        );
    }

    // Check if already registered so we can skip gracefully.
    let list_out = std::process::Command::new("claude")
        .args(["mcp", "list"])
        .output()?;
    let list_str = String::from_utf8_lossy(&list_out.stdout);
    if list_str.contains("hivemind") {
        println!("HiveMind is already registered with Claude Code.");
        println!("Open a new Claude Code session to use it.");
        return Ok(());
    }

    // Register as stdio using the full binary path so Claude Code can find it
    // regardless of whether ~/.cargo/bin is in its subprocess PATH. User scope:
    // the default (local) scope is per project directory, but registration is
    // meant to happen once per machine.
    let exe = exe_path();
    let status = std::process::Command::new("claude")
        .args(["mcp", "add", "--scope", "user", "hivemind", "--", &exe])
        .status()?;

    if !status.success() {
        anyhow::bail!("claude mcp add failed. Run `claude mcp list` to inspect existing servers");
    }

    println!("HiveMind registered with Claude Code.");
    println!();
    println!("Next steps:");
    println!("  1. Open a new Claude Code session");
    println!("  2. Type /memory-status to verify");
    Ok(())
}

fn install_opencode() -> Result<()> {
    // Try the CLI first; fall back to writing the config file directly.
    let cli_available = std::process::Command::new("opencode")
        .arg("--version")
        .output()
        .is_ok();

    let exe = exe_path();
    if cli_available {
        let list_out = std::process::Command::new("opencode")
            .args(["mcp", "list"])
            .output()?;
        if String::from_utf8_lossy(&list_out.stdout).contains("hivemind") {
            println!("HiveMind is already registered with OpenCode.");
            println!("Open a new OpenCode session to use it.");
            return Ok(());
        }
        let status = std::process::Command::new("opencode")
            .args(["mcp", "add", "hivemind", &exe])
            .status()?;
        if !status.success() {
            anyhow::bail!("opencode mcp add failed. Check `opencode mcp list`");
        }
    } else {
        // Write to the global opencode config.
        let xdg_config = std::env::var_os("XDG_CONFIG_HOME")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| home_dir().join(".config"));
        let config_path = xdg_config.join("opencode").join("opencode.json");
        upsert_json_mcp(
            &config_path,
            "hivemind",
            serde_json::json!({
                "type": "local",
                "command": exe,
                "args": []
            }),
        )?;
        println!("Written to {}", config_path.display());
    }

    println!("HiveMind registered with OpenCode.");
    println!();
    println!("Next steps:");
    println!("  1. Open a new OpenCode session");
    Ok(())
}

fn install_kimi() -> Result<()> {
    let cli_available = std::process::Command::new("kimi")
        .arg("--version")
        .output()
        .is_ok();

    let exe = exe_path();
    if cli_available {
        let list_out = std::process::Command::new("kimi")
            .args(["mcp", "list"])
            .output()?;
        if String::from_utf8_lossy(&list_out.stdout).contains("hivemind") {
            println!("HiveMind is already registered with Kimi.");
            println!("Open a new Kimi session to use it.");
            return Ok(());
        }
        let status = std::process::Command::new("kimi")
            .args(["mcp", "add", "hivemind", &exe])
            .status()?;
        if !status.success() {
            anyhow::bail!("kimi mcp add failed. Check `kimi mcp list`");
        }
    } else {
        let config_path = home_dir().join(".kimi").join("mcp.json");
        upsert_json_mcp(
            &config_path,
            "hivemind",
            serde_json::json!({ "command": exe, "args": [] }),
        )?;
        println!("Written to {}", config_path.display());
    }

    println!("HiveMind registered with Kimi Code CLI.");
    println!();
    println!("Next steps:");
    println!("  1. Open a new Kimi session");
    Ok(())
}

/// Escape a string for use inside a basic (double-quoted) TOML string.
fn toml_escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

fn install_codex() -> Result<()> {
    let config_path = home_dir().join(".codex").join("config.toml");
    std::fs::create_dir_all(config_path.parent().unwrap())?;

    let existing = std::fs::read_to_string(&config_path).unwrap_or_default();
    if existing.contains("[mcp_servers.hivemind]") {
        println!("HiveMind is already registered with Codex CLI.");
        println!("Open a new Codex session to use it.");
        return Ok(());
    }

    let block = format!(
        "\n[mcp_servers.hivemind]\ncommand = \"{}\"\nargs = []\n",
        toml_escape(&exe_path())
    );
    let block = block.as_str();
    let new_content = format!("{}{}", existing.trim_end(), block);
    std::fs::write(&config_path, new_content)?;
    println!("Written to {}", config_path.display());

    println!("HiveMind registered with OpenAI Codex CLI.");
    println!();
    println!("Next steps:");
    println!("  1. Open a new Codex session");
    Ok(())
}

fn install_cursor() -> Result<()> {
    let config_path = home_dir().join(".cursor").join("mcp.json");
    upsert_json_mcp(
        &config_path,
        "hivemind",
        serde_json::json!({ "command": exe_path(), "args": [] }),
    )?;
    println!("Written to {}", config_path.display());
    println!("HiveMind registered with Cursor.");
    println!();
    println!("Next steps:");
    println!("  1. Restart Cursor completely for the change to take effect");
    Ok(())
}

fn install_windsurf() -> Result<()> {
    let config_path = home_dir()
        .join(".codeium")
        .join("windsurf")
        .join("mcp_config.json");
    upsert_json_mcp(
        &config_path,
        "hivemind",
        serde_json::json!({ "command": exe_path(), "args": [] }),
    )?;
    println!("Written to {}", config_path.display());
    println!("HiveMind registered with Windsurf.");
    println!();
    println!("Next steps:");
    println!("  1. Restart Windsurf for the change to take effect");
    Ok(())
}

/// Read an mcpServers-style JSON config, insert or update the named server entry, write back.
/// Creates the file and parent directories if absent.
fn upsert_json_mcp(path: &std::path::Path, name: &str, entry: serde_json::Value) -> Result<()> {
    std::fs::create_dir_all(path.parent().unwrap())?;

    let mut root: serde_json::Value = if path.exists() {
        let raw = std::fs::read_to_string(path)?;
        serde_json::from_str(&raw).unwrap_or(serde_json::json!({}))
    } else {
        serde_json::json!({})
    };

    // OpenCode uses {"mcp": {...}}; all other clients use {"mcpServers": {...}}.
    // Detect from existing file structure; fall back based on whether entry has "type" field.
    let servers_key = if root.get("mcp").is_some() || entry.get("type").is_some() {
        "mcp"
    } else {
        "mcpServers"
    };

    root.as_object_mut()
        .unwrap()
        .entry(servers_key)
        .or_insert(serde_json::json!({}))
        .as_object_mut()
        .unwrap()
        .insert(name.to_string(), entry);

    std::fs::write(path, serde_json::to_string_pretty(&root)?)?;
    Ok(())
}

/// Create the global config file with defaults on first run if it doesn't exist yet.
/// Prints a note to stderr so the user knows where it landed.
pub fn ensure_global_config() {
    let config_path = crate::config::global_config_path();
    if config_path.exists() {
        return;
    }
    match write_if_absent(&config_path, GLOBAL_CONFIG) {
        Ok(_) => {
            eprintln!("note: created default config at {}", config_path.display());
        }
        Err(e) => {
            eprintln!("warning: could not create global config: {e}");
        }
    }
}

/// Print a one-time hint when `hivemind init` has never been run.
/// Called from commands that work without init but benefit from it.
pub fn warn_if_not_initialized() {
    let config_path = crate::config::global_config_path();
    let home = home_dir();

    if !config_path.exists() {
        eprintln!("hint: looks like you haven't run `hivemind init` yet.");
        eprintln!("      Run it in your project directory to create config files and");
        eprintln!("      register HiveMind with your AI coding client.");
        eprintln!();
        return;
    }

    // init was run but no AI client has been registered yet
    if detect_registered_clients(&home).is_empty() {
        eprintln!("hint: no AI client is registered with HiveMind yet.");
        eprintln!("      The server will start, but your AI client won't connect to it.");
        eprintln!("      Register once with:  hivemind mcp install claude");
        eprintln!("      (or cursor, windsurf, opencode, kimi, codex)");
        eprintln!();
    }
}

/// TCP-probes whether a HiveMind server is currently listening on
/// `settings`'s host:port. 0.0.0.0/:: are redirected to 127.0.0.1 since you
/// can't dial a wildcard bind address directly.
pub fn probe_server_up(settings: &crate::config::ServerSettings) -> bool {
    let probe_host = match settings.host.as_str() {
        "0.0.0.0" | "::" => "127.0.0.1",
        h => h,
    };
    format!("{probe_host}:{}", settings.port)
        .parse::<std::net::SocketAddr>()
        .ok()
        .map(|addr| {
            std::net::TcpStream::connect_timeout(&addr, std::time::Duration::from_millis(300))
                .is_ok()
        })
        .unwrap_or(false)
}

pub fn cmd_status(plain: bool) -> Result<()> {
    let home = home_dir();
    let cwd = std::env::current_dir()?;
    let db_path = crate::db::resolve_db_path();

    if crate::tui::is_interactive(plain) {
        return tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?
            .block_on(async {
                let clients = detect_registered_clients(&home);
                let settings =
                    crate::config::load_server_settings(&crate::config::global_config_path())?;
                let server_up = probe_server_up(&settings);
                let sync = crate::config::SyncSettings::default();
                let database = crate::db::open_database(&sync, &db_path).await?;
                let conn = database.connect()?;
                crate::db::run_migrations(&conn).await?;
                let store = crate::store::SqliteStore::new(conn);
                crate::tui::status_view::run(
                    &cwd,
                    &crate::config::global_config_path(),
                    &store,
                    &db_path,
                    &clients,
                    &settings,
                    server_up,
                )
                .await
            });
    }

    let (out, clients) = with_spinner("checking status...", || {
        let clients = detect_registered_clients(&home);
        let settings = crate::config::load_server_settings(&crate::config::global_config_path())?;
        let server_up = probe_server_up(&settings);
        let result = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?
            .block_on(async {
                let sync = crate::config::SyncSettings::default();
                let database = crate::db::open_database(&sync, &db_path).await?;
                let conn = database.connect()?;
                crate::db::run_migrations(&conn).await?;
                let store = crate::store::SqliteStore::new(conn);
                render_status(
                    &cwd,
                    &crate::config::global_config_path(),
                    &store,
                    &db_path,
                    &clients,
                    &settings,
                    server_up,
                )
                .await
            })?;
        Ok::<_, anyhow::Error>((result, clients))
    })?;
    println!("{out}");
    if clients.is_empty() {
        eprintln!("hint: no AI client is registered with HiveMind yet.");
        eprintln!("      Register once with:  hivemind mcp install claude");
        eprintln!("      (or cursor, windsurf, opencode, kimi, codex)");
    }
    Ok(())
}

pub fn cmd_session_start(json: bool) -> Result<()> {
    let cwd = std::env::current_dir()?;
    if crate::config::discover_project_root(&cwd).is_none() {
        return Ok(()); // no project config: stay silent so hooks can run unconditionally
    }
    let db_path = crate::db::resolve_db_path();
    let out = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?
        .block_on(async {
            let settings =
                crate::config::load_server_settings(&crate::config::global_config_path())
                    .map(|s| s.sync)
                    .unwrap_or_default();
            let database = crate::db::open_database(&settings, &db_path).await?;
            let conn = database.connect()?;
            crate::db::run_migrations(&conn).await?;
            let store = crate::store::SqliteStore::new(conn);
            let config = crate::config::load_config(&cwd)?;
            let result = crate::session::execute_session_start(&config, &store).await?;
            if let Err(e) = store
                .log_session_start(&cwd.to_string_lossy(), &result)
                .await
            {
                tracing::warn!("failed to write session_start_log entry: {e:#}");
            }
            Ok::<_, anyhow::Error>(render_session_start(&result, json))
        })?;
    if !out.is_empty() {
        println!("{out}");
    }
    Ok(())
}

fn render_session_start(result: &crate::session::SessionStartResult, json: bool) -> String {
    if json {
        return serde_json::to_string_pretty(&result.to_json()).unwrap_or_default();
    }
    if result.loaded.is_empty() && result.skipped.is_empty() {
        return String::new();
    }
    let mut out = format!(
        "<hivemind-context project=\"{}\" tokens=\"{}/{}\">\n",
        result.project, result.used_tokens, result.max_tokens
    );
    for l in &result.loaded {
        out.push_str(&format!("\n## {}\n{}\n", l.entry.title, l.entry.content));
    }
    out.push_str("</hivemind-context>\n");
    for s in &result.skipped {
        out.push_str(&format!(
            "hivemind: skipped recall \"{}\" ({})\n",
            s.query,
            s.reason.as_str()
        ));
    }
    out
}

pub(crate) fn do_migrate_copy(legacy: &Path, new_path: &Path) -> Result<()> {
    if let Some(dir) = new_path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::copy(legacy, new_path)?;
    Ok(())
}

pub fn cmd_migrate() -> Result<()> {
    let legacy = crate::db::legacy_db_path();
    let new_path = crate::db::xdg_data_dir().join("memories.db");
    let stdin = std::io::stdin();
    cmd_migrate_inner(&legacy, &new_path, &mut stdin.lock())
}

pub(crate) fn cmd_migrate_inner(
    legacy: &Path,
    new_path: &Path,
    stdin: &mut dyn std::io::BufRead,
) -> Result<()> {
    if !legacy.exists() {
        println!(
            "Nothing to migrate: legacy database not found at {}",
            legacy.display()
        );
        println!("New location: {}", new_path.display());
        return Ok(());
    }

    if new_path.exists() {
        println!("New database already exists at {}.", new_path.display());
        println!("Remove it first if you want to replace it with the legacy database.");
        return Ok(());
    }

    println!("Migrating database:");
    println!("  from: {}", legacy.display());
    println!("    to: {}", new_path.display());
    print!("Proceed? [y/N] ");
    std::io::stdout().flush()?;

    let mut input = String::new();
    stdin.read_line(&mut input)?;
    if !input.trim().eq_ignore_ascii_case("y") {
        println!("Cancelled.");
        return Ok(());
    }

    do_migrate_copy(legacy, new_path)?;
    println!(
        "Done. You can now delete the old directory: rm -rf {}",
        legacy.parent().unwrap().display()
    );
    Ok(())
}

/// Build the `hivemind status` report. `global_path` is injectable for testing.
pub struct LoadedEntrySummary {
    pub title: String,
    pub tokens: usize,
    pub is_local: bool,
}

pub struct SkippedEntrySummary {
    pub query: String,
    pub reason: &'static str,
}

pub struct ProjectStatus {
    pub project_name: String,
    pub has_local_config: bool,
    pub file_open_rule_count: usize,
    pub mention_trigger_count: usize,
    pub loaded: Vec<LoadedEntrySummary>,
    pub skipped: Vec<SkippedEntrySummary>,
    pub used_tokens: usize,
    pub max_tokens: usize,
    pub truncated: bool,
}

pub struct StatusData {
    pub version: &'static str,
    pub project_label: Option<String>,
    pub server_up: bool,
    pub server_host: String,
    pub server_port: u16,
    pub db_path: String,
    pub memory_count: i64,
    pub sync_enabled: bool,
    pub sync_remote_url: String,
    pub registered_clients: Vec<String>,
    pub project: Option<ProjectStatus>,
    pub matrix: MatrixStatusLine,
}

pub enum MatrixStatusLine {
    /// No `[matrix]` section in the global config — matrix isn't set up.
    NotConfigured,
    /// Configured, but `hivemind matrix run` isn't currently up.
    NotRunning,
    Running {
        user_id: String,
        sync_state: String,
        room_count: usize,
        active_sessions: usize,
    },
}

pub async fn build_status_data(
    cwd: &Path,
    global_path: &Path,
    store: &crate::store::SqliteStore,
    db_path: &str,
    registered_clients: &[&str],
    settings: &crate::config::ServerSettings,
    server_up: bool,
) -> Result<StatusData> {
    let version = env!("CARGO_PKG_VERSION");
    let memory_count = store.count().await?;

    let root = crate::config::discover_project_root(cwd);
    let config = match &root {
        Some(r) => Some(crate::config::load_config_with_global(r, global_path)?),
        None => None,
    };
    let project_label = config.as_ref().map(|c| c.project_name.clone());

    let probe_host = match settings.host.as_str() {
        "0.0.0.0" | "::" => "127.0.0.1",
        h => h,
    }
    .to_string();

    let matrix = match crate::config::load_matrix_settings(global_path)? {
        None => MatrixStatusLine::NotConfigured,
        Some(_) => {
            let socket_path = crate::matrix::status::socket_path();
            match crate::matrix::status::query_status(&socket_path).await {
                Ok(reply) => MatrixStatusLine::Running {
                    user_id: reply.user_id,
                    sync_state: reply.sync_state,
                    room_count: reply.rooms.len(),
                    active_sessions: reply.rooms.iter().filter(|r| r.active_session).count(),
                },
                Err(_) => MatrixStatusLine::NotRunning,
            }
        }
    };

    let mut data = StatusData {
        version,
        project_label,
        server_up,
        server_host: probe_host,
        server_port: settings.port,
        db_path: db_path.to_string(),
        memory_count,
        sync_enabled: settings.sync.enabled,
        sync_remote_url: settings.sync.remote_url.clone(),
        registered_clients: registered_clients.iter().map(|s| s.to_string()).collect(),
        project: None,
        matrix,
    };

    let (Some(root), Some(config)) = (root, config) else {
        return Ok(data);
    };

    let result = crate::session::execute_session_start(&config, store).await?;

    data.project = Some(ProjectStatus {
        project_name: config.project_name.clone(),
        has_local_config: root.join(".hivemind.local.toml").is_file(),
        file_open_rule_count: config.file_open_rule_count,
        mention_trigger_count: config.mention_trigger_count,
        loaded: result
            .loaded
            .iter()
            .map(|l| LoadedEntrySummary {
                title: l.entry.title.clone(),
                tokens: l.tokens,
                is_local: matches!(l.source, crate::config::RecallSource::Local),
            })
            .collect(),
        skipped: result
            .skipped
            .iter()
            .map(|s| SkippedEntrySummary {
                query: s.query.clone(),
                reason: s.reason.as_str(),
            })
            .collect(),
        used_tokens: result.used_tokens,
        max_tokens: result.max_tokens,
        truncated: result.truncated(),
    });

    Ok(data)
}

pub fn format_status_text(data: &StatusData) -> String {
    use std::fmt::Write as _;
    let mut out = String::new();

    match &data.project_label {
        Some(label) => writeln!(out, "HiveMind v{} — {label}", data.version).unwrap(),
        None => writeln!(out, "HiveMind v{}", data.version).unwrap(),
    }
    writeln!(out, "─────────────────────────────────────────────────────").unwrap();
    if data.server_up {
        writeln!(
            out,
            "Server:     running at http://{}:{} (hivemind up)",
            data.server_host, data.server_port
        )
        .unwrap();
    } else {
        writeln!(
            out,
            "Server:     not running (stdio instance is spawned per session)"
        )
        .unwrap();
    }
    writeln!(
        out,
        "Storage:    {} ({} memories)",
        data.db_path, data.memory_count
    )
    .unwrap();
    if data.sync_enabled {
        writeln!(out, "Sync:       enabled \u{2192} {}", data.sync_remote_url).unwrap();
    } else {
        writeln!(out, "Sync:       disabled (local only)").unwrap();
    }
    if data.registered_clients.is_empty() {
        writeln!(out, "AI clients: none registered").unwrap();
    } else {
        writeln!(out, "AI clients: {}", data.registered_clients.join(", ")).unwrap();
    }
    match &data.matrix {
        MatrixStatusLine::NotConfigured => {}
        MatrixStatusLine::NotRunning => {
            writeln!(
                out,
                "Matrix:     configured, not running (hivemind matrix run)"
            )
            .unwrap();
        }
        MatrixStatusLine::Running {
            user_id,
            sync_state,
            room_count,
            active_sessions,
        } => {
            writeln!(
                out,
                "Matrix:     {user_id} ({sync_state}), {room_count} room(s), \
                 {active_sessions} active session(s)"
            )
            .unwrap();
        }
    }
    writeln!(out).unwrap();

    let Some(project) = &data.project else {
        writeln!(out, "No .hivemind.toml found in this directory tree.").unwrap();
        writeln!(
            out,
            "Run `hivemind init` to set up memory hooks for this project."
        )
        .unwrap();
        return out;
    };

    writeln!(out, "Project:    {}", project.project_name).unwrap();
    writeln!(
        out,
        "Config:     .hivemind.toml{}",
        if project.has_local_config {
            " + .hivemind.local.toml"
        } else {
            ""
        }
    )
    .unwrap();
    writeln!(out).unwrap();
    writeln!(out, "On session start will inject:").unwrap();
    if project.loaded.is_empty() {
        writeln!(out, "  (nothing — no recalls configured or none resolved)").unwrap();
    }
    for entry in &project.loaded {
        let local = if entry.is_local { "  (local)" } else { "" };
        writeln!(
            out,
            "  {:<40} ~{} tokens{}",
            entry.title, entry.tokens, local
        )
        .unwrap();
    }
    for skip in &project.skipped {
        writeln!(out, "  [skipped]   {:<40} ({})", skip.query, skip.reason).unwrap();
    }
    writeln!(
        out,
        "  ──────────────────────────────────────────────────────────"
    )
    .unwrap();
    writeln!(out, "  Total:      ~{} tokens", project.used_tokens).unwrap();
    writeln!(out, "  Budget:     {} tokens", project.max_tokens).unwrap();
    let headroom = if project.truncated { "⚠" } else { "✓" };
    let remaining = project.max_tokens.saturating_sub(project.used_tokens);
    writeln!(out, "  Remaining:  ~{remaining} tokens  {headroom}").unwrap();
    writeln!(out).unwrap();
    writeln!(
        out,
        "On file open rules:    {} configured (reserved, not yet active)",
        project.file_open_rule_count
    )
    .unwrap();
    writeln!(
        out,
        "On mention triggers:   {} (reserved, not yet active)",
        project.mention_trigger_count
    )
    .unwrap();

    out
}

pub async fn render_status(
    cwd: &Path,
    global_path: &Path,
    store: &crate::store::SqliteStore,
    db_path: &str,
    registered_clients: &[&str],
    settings: &crate::config::ServerSettings,
    server_up: bool,
) -> Result<String> {
    let data = build_status_data(
        cwd,
        global_path,
        store,
        db_path,
        registered_clients,
        settings,
        server_up,
    )
    .await?;
    Ok(format_status_text(&data))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn status_plain_flag_parses() {
        let cli = Cli::try_parse_from(["hivemind", "status", "--plain"]).unwrap();
        match cli.command {
            Some(Command::Status { plain }) => assert!(plain),
            _ => panic!("expected Status command"),
        }
    }

    #[test]
    fn parses_matrix_login_subcommand() {
        let cli = Cli::parse_from(["hivemind", "matrix", "login"]);
        assert!(matches!(
            cli.command,
            Some(Command::Matrix {
                action: MatrixAction::Login
            })
        ));
    }

    #[test]
    fn parses_matrix_run_subcommand() {
        let cli = Cli::parse_from(["hivemind", "matrix", "run"]);
        assert!(matches!(
            cli.command,
            Some(Command::Matrix {
                action: MatrixAction::Run { debug: false }
            })
        ));
    }

    #[test]
    fn parses_matrix_send_subcommand() {
        let cli = Cli::parse_from(["hivemind", "matrix", "send", "@oxgrad:matrix.org", "hi"]);
        assert!(matches!(
            cli.command,
            Some(Command::Matrix {
                action: MatrixAction::Send { user_id, message }
            }) if user_id == "@oxgrad:matrix.org" && message == "hi"
        ));
    }

    #[test]
    fn parses_matrix_status_subcommand() {
        let cli = Cli::parse_from(["hivemind", "matrix", "status"]);
        assert!(matches!(
            cli.command,
            Some(Command::Matrix {
                action: MatrixAction::Status
            })
        ));
    }

    #[test]
    fn up_plain_flag_parses() {
        let cli = Cli::try_parse_from(["hivemind", "up", "--plain"]).unwrap();
        match cli.command {
            Some(Command::Up { headless, plain }) => {
                assert!(!headless);
                assert!(plain);
            }
            _ => panic!("expected Up command"),
        }
    }

    /// Default server settings, built the same way `hivemind status` does when
    /// no global config exists.
    fn default_settings() -> crate::config::ServerSettings {
        crate::config::load_server_settings(Path::new("/nonexistent/hivemind-global.toml")).unwrap()
    }

    fn sample_result(loaded: bool, skipped: bool) -> crate::session::SessionStartResult {
        use crate::session::{LoadedEntry, SkipReason, SkippedEntry};
        use crate::store::MemoryEntry;

        let loaded_vec = if loaded {
            vec![LoadedEntry {
                entry: MemoryEntry {
                    id: "mem_1".to_string(),
                    title: "pref a".to_string(),
                    content: "short content a".to_string(),
                    tags: vec![],
                    created_at: 0,
                    updated_at: 0,
                    token_count: None,
                    layer: "workspace".to_string(),
                    memory_type: "project".to_string(),
                },
                tokens: 5,
                source: crate::config::RecallSource::Project,
            }]
        } else {
            vec![]
        };
        let skipped_vec = if skipped {
            vec![SkippedEntry {
                query: "missing".to_string(),
                reason: SkipReason::NotFound,
            }]
        } else {
            vec![]
        };
        crate::session::SessionStartResult {
            project: "test-proj".to_string(),
            loaded: loaded_vec,
            skipped: skipped_vec,
            used_tokens: 5,
            max_tokens: 2000,
            memories_recalled: if loaded { 1 } else { 0 },
        }
    }

    #[test]
    fn render_session_start_text_wraps_in_hivemind_context_tags() {
        let result = sample_result(true, false);
        let out = render_session_start(&result, false);
        assert!(out.contains("<hivemind-context"));
        assert!(out.contains("pref a"));
        assert!(out.contains("short content a"));
    }

    #[test]
    fn render_session_start_text_empty_when_nothing_loaded_or_skipped() {
        let result = sample_result(false, false);
        let out = render_session_start(&result, false);
        assert!(out.is_empty());
    }

    #[test]
    fn render_session_start_json_parses_and_matches_shape() {
        let result = sample_result(true, true);
        let out = render_session_start(&result, true);
        let v: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["project"], "test-proj");
        assert_eq!(v["context_loaded"][0]["title"], "pref a");
        assert_eq!(v["skipped"][0]["query"], "missing");
        assert_eq!(v["budget"]["max_tokens"], 2000);
    }

    #[test]
    fn write_atomic_creates_file_with_content() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("out.txt");
        write_atomic(&path, "hello world").unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "hello world");
    }

    #[test]
    fn write_atomic_overwrites_existing_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("out.txt");
        write_atomic(&path, "first").unwrap();
        write_atomic(&path, "second").unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "second");
    }

    #[test]
    fn write_if_absent_creates_when_missing() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("new.txt");
        let (p, status) = write_if_absent(&path, "content").unwrap();
        assert_eq!(status, "created");
        assert_eq!(p, path);
        assert_eq!(fs::read_to_string(&path).unwrap(), "content");
    }

    #[test]
    fn write_if_absent_skips_when_present() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("existing.txt");
        fs::write(&path, "original").unwrap();
        let (_, status) = write_if_absent(&path, "new content").unwrap();
        assert_eq!(status, "exists");
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            "original",
            "must not overwrite"
        );
    }

    #[test]
    fn ensure_line_appends_to_empty_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(".gitignore");
        let (_, status) = ensure_line(&path, "*.log").unwrap();
        assert_eq!(status, "created");
        assert!(fs::read_to_string(&path).unwrap().contains("*.log"));
    }

    #[test]
    fn ensure_line_is_idempotent() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(".gitignore");
        fs::write(&path, "*.log\n").unwrap();
        let (_, status) = ensure_line(&path, "*.log").unwrap();
        assert_eq!(status, "exists");
        assert_eq!(
            fs::read_to_string(&path).unwrap().matches("*.log").count(),
            1
        );
    }

    #[test]
    fn ensure_line_appends_to_existing_file_without_trailing_newline() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(".gitignore");
        fs::write(&path, "node_modules").unwrap();
        ensure_line(&path, "*.log").unwrap();
        let content = fs::read_to_string(&path).unwrap();
        assert!(content.contains("node_modules"));
        assert!(content.contains("*.log"));
    }

    #[test]
    fn append_block_if_absent_appends_when_marker_missing() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("CLAUDE.md");
        fs::write(&path, "# My rules\n").unwrap();
        let (_, status) =
            append_block_if_absent(&path, "# HiveMind", "# HiveMind\nsome block\n").unwrap();
        assert_eq!(status, "created");
        let content = fs::read_to_string(&path).unwrap();
        assert!(content.contains("My rules"));
        assert!(content.contains("# HiveMind"));
    }

    #[test]
    fn append_block_if_absent_is_idempotent() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("CLAUDE.md");
        fs::write(&path, "# HiveMind\nexisting block\n").unwrap();
        let (_, status) =
            append_block_if_absent(&path, "# HiveMind", "# HiveMind\nnew block\n").unwrap();
        assert_eq!(status, "exists");
        assert_eq!(
            fs::read_to_string(&path)
                .unwrap()
                .matches("# HiveMind")
                .count(),
            1
        );
    }

    #[test]
    fn scaffold_creates_all_files() {
        let proj = tempfile::tempdir().unwrap();
        let home = tempfile::tempdir().unwrap();
        let cfg = tempfile::tempdir().unwrap();
        let report = scaffold(proj.path(), home.path(), cfg.path()).unwrap();

        assert!(proj.path().join(".hivemind.toml").is_file());
        assert!(proj.path().join(".hivemind.local.toml").is_file());
        assert!(proj.path().join("CLAUDE.md").is_file());
        assert!(proj.path().join(".gitignore").is_file());
        assert!(home.path().join(".claude").join("CLAUDE.md").is_file());
        assert!(cfg.path().join("config.toml").is_file());

        let gi = fs::read_to_string(proj.path().join(".gitignore")).unwrap();
        assert!(gi.contains(".hivemind.local.toml"));
        let gc = fs::read_to_string(home.path().join(".claude").join("CLAUDE.md")).unwrap();
        assert!(gc.contains("HiveMind Memory System"));
        let pj = fs::read_to_string(proj.path().join(".hivemind.toml")).unwrap();
        let dirname = proj.path().file_name().unwrap().to_string_lossy();
        assert!(pj.contains(&*dirname));

        assert!(report.iter().all(|(_, status)| *status == "created"));
    }

    #[test]
    fn scaffold_is_idempotent_and_does_not_duplicate_global_block() {
        let proj = tempfile::tempdir().unwrap();
        let home = tempfile::tempdir().unwrap();
        let cfg = tempfile::tempdir().unwrap();

        scaffold(proj.path(), home.path(), cfg.path()).unwrap();
        let report2 = scaffold(proj.path(), home.path(), cfg.path()).unwrap();

        assert!(report2.iter().all(|(_, status)| *status == "exists"));

        let gc = fs::read_to_string(home.path().join(".claude").join("CLAUDE.md")).unwrap();
        assert_eq!(gc.matches("# HiveMind Memory System").count(), 1);
        let gi = fs::read_to_string(proj.path().join(".gitignore")).unwrap();
        assert_eq!(gi.matches(".hivemind.local.toml").count(), 1);
    }

    #[test]
    fn scaffold_preserves_existing_user_claude_md() {
        let proj = tempfile::tempdir().unwrap();
        let home = tempfile::tempdir().unwrap();
        let cfg = tempfile::tempdir().unwrap();

        // The user already has a customized global CLAUDE.md.
        let global = home.path().join(".claude").join("CLAUDE.md");
        fs::create_dir_all(global.parent().unwrap()).unwrap();
        fs::write(&global, "# My personal rules\nAlways write tests first.\n").unwrap();

        scaffold(proj.path(), home.path(), cfg.path()).unwrap();

        let gc = fs::read_to_string(&global).unwrap();
        assert!(
            gc.contains("My personal rules"),
            "user content must be preserved"
        );
        assert!(
            gc.contains("Always write tests first."),
            "user content must be preserved"
        );
        assert!(
            gc.contains("# HiveMind Memory System"),
            "hook block appended"
        );
    }

    #[tokio::test]
    async fn render_status_previews_injection() {
        use crate::{config::SyncSettings, db, store::SqliteStore};

        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let sync = SyncSettings::default();
        let database = db::open_database(&sync, db_path.to_str().unwrap())
            .await
            .unwrap();
        let conn = database.connect().unwrap();
        db::run_migrations(&conn).await.unwrap();
        let store = SqliteStore::new(conn);
        let id = format!("mem_{}", uuid::Uuid::new_v4().simple());
        store
            .store(&crate::store::NewMemoryRow {
                id: &id,
                title: "golang preferences",
                content: "uber/zap, sqlc, pgx v5",
                tags: &["golang".to_string()],
                token_count: None,
                layer: "workspace",
                memory_type: "project",
            })
            .await
            .unwrap();

        let proj = tempfile::tempdir().unwrap();
        std::fs::write(
            proj.path().join(".hivemind.toml"),
            "[project]\nname=\"demo\"\n[hooks.on_session_start]\nmax_tokens=2000\nrecalls=[\"golang preferences\"]\n",
        ).unwrap();
        let missing_global = proj.path().join("no-global.toml");

        let out = render_status(
            proj.path(),
            &missing_global,
            &store,
            "/tmp/x.db",
            &[],
            &default_settings(),
            false,
        )
        .await
        .unwrap();
        assert!(out.contains("demo"), "shows project name");
        assert!(
            out.contains("golang preferences"),
            "lists the injected memory"
        );
        assert!(out.contains("Budget:"), "shows the budget line");
        assert!(
            out.contains("1 memories") || out.contains("1 memorie"),
            "shows memory count"
        );
        assert!(
            out.contains("AI clients: none"),
            "shows no registered clients"
        );
    }

    #[tokio::test]
    async fn render_status_shows_registered_clients() {
        use crate::{config::SyncSettings, db, store::SqliteStore};

        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let sync = SyncSettings::default();
        let database = db::open_database(&sync, db_path.to_str().unwrap())
            .await
            .unwrap();
        let conn = database.connect().unwrap();
        db::run_migrations(&conn).await.unwrap();
        let store = SqliteStore::new(conn);

        let proj = tempfile::tempdir().unwrap();
        let missing_global = proj.path().join("no-global.toml");
        let out = render_status(
            proj.path(),
            &missing_global,
            &store,
            "/tmp/x.db",
            &["claude", "cursor"],
            &default_settings(),
            false,
        )
        .await
        .unwrap();
        assert!(
            out.contains("AI clients: claude, cursor"),
            "lists registered clients"
        );
    }

    #[tokio::test]
    async fn render_status_without_config_reports_missing() {
        use crate::{config::SyncSettings, db, store::SqliteStore};

        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let sync = SyncSettings::default();
        let database = db::open_database(&sync, db_path.to_str().unwrap())
            .await
            .unwrap();
        let conn = database.connect().unwrap();
        db::run_migrations(&conn).await.unwrap();
        let store = SqliteStore::new(conn);

        let proj = tempfile::tempdir().unwrap();
        let missing_global = proj.path().join("no-global.toml");
        let out = render_status(
            proj.path(),
            &missing_global,
            &store,
            "/tmp/x.db",
            &[],
            &default_settings(),
            false,
        )
        .await
        .unwrap();
        assert!(
            out.contains("hivemind init"),
            "suggests init when no config"
        );
    }

    #[tokio::test]
    async fn build_status_data_matches_render_status_text() {
        use crate::{config::SyncSettings, db, store::SqliteStore};

        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let sync = SyncSettings::default();
        let database = db::open_database(&sync, db_path.to_str().unwrap())
            .await
            .unwrap();
        let conn = database.connect().unwrap();
        db::run_migrations(&conn).await.unwrap();
        let store = SqliteStore::new(conn);
        let id = format!("mem_{}", uuid::Uuid::new_v4().simple());
        store
            .store(&crate::store::NewMemoryRow {
                id: &id,
                title: "golang preferences",
                content: "uber/zap, sqlc, pgx v5",
                tags: &["golang".to_string()],
                token_count: None,
                layer: "workspace",
                memory_type: "project",
            })
            .await
            .unwrap();

        let proj = tempfile::tempdir().unwrap();
        std::fs::write(
            proj.path().join(".hivemind.toml"),
            "[project]\nname=\"test-proj\"\n[hooks.on_session_start]\nrecalls=[\"golang preferences\"]\n",
        )
        .unwrap();
        let global_path = dir.path().join("no-global.toml");
        let settings = crate::config::ServerSettings {
            host: "127.0.0.1".into(),
            port: 3456,
            dashboard_port: 3457,
            api_url: "http://127.0.0.1:3456".into(),
            cors_origin: "http://127.0.0.1:3457".into(),
            sync: SyncSettings::default(),
            update: crate::config::UpdateSettings::default(),
            agent: crate::config::AgentSettings::default(),
        };

        let via_text = render_status(
            proj.path(),
            &global_path,
            &store,
            "test.db",
            &["claude"],
            &settings,
            true,
        )
        .await
        .unwrap();

        let data = build_status_data(
            proj.path(),
            &global_path,
            &store,
            "test.db",
            &["claude"],
            &settings,
            true,
        )
        .await
        .unwrap();
        let via_struct = format_status_text(&data);

        assert_eq!(
            via_text, via_struct,
            "render_status() must stay byte-for-byte identical after the struct extraction"
        );
        assert_eq!(data.memory_count, 1);
        assert_eq!(data.project.as_ref().unwrap().project_name, "test-proj");
        assert_eq!(data.project.as_ref().unwrap().loaded.len(), 1);
        assert_eq!(
            data.project.as_ref().unwrap().loaded[0].title,
            "golang preferences"
        );

        // Pin down the actual rendered format so a regression in
        // format_status_text is caught, not just disagreement between
        // build_status_data and render_status.
        assert!(via_struct.contains("HiveMind v"));
        assert!(via_struct.contains(" — test-proj")); // em dash preserved from the original literal output
        assert!(via_struct.contains("Server:     running at http://127.0.0.1:3456 (hivemind up)"));
        assert!(via_struct.contains("Sync:       disabled (local only)"));
        assert!(via_struct.contains("AI clients: claude"));
        assert!(via_struct.contains("Project:    test-proj"));
        assert!(via_struct.contains("golang preferences"));
        // "Remaining" line: verify the saturating_sub substitution in format_status_text
        // matches the real budget arithmetic (used_tokens, max_tokens from the loaded config),
        // not just that build_status_data and render_status agree with each other.
        let project = data.project.as_ref().unwrap();
        let expected_remaining = project.max_tokens.saturating_sub(project.used_tokens);
        assert!(via_struct.contains(&format!("Remaining:  ~{expected_remaining} tokens")));
    }

    // ── detect_registered_clients ────────────────────────────────────────────

    #[test]
    fn detect_registered_clients_empty_when_no_configs() {
        let home = tempfile::tempdir().unwrap();
        let result = detect_registered_clients(home.path());
        assert!(result.is_empty());
    }

    #[test]
    fn detect_registered_clients_claude_via_mcp_json() {
        let home = tempfile::tempdir().unwrap();
        let claude_dir = home.path().join(".claude");
        fs::create_dir_all(&claude_dir).unwrap();
        fs::write(
            claude_dir.join("mcp.json"),
            r#"{"mcpServers":{"hivemind":{"command":"hivemind"}}}"#,
        )
        .unwrap();
        let result = detect_registered_clients(home.path());
        assert!(result.contains(&"claude"));
    }

    #[test]
    fn detect_registered_clients_claude_via_settings_json() {
        let home = tempfile::tempdir().unwrap();
        let claude_dir = home.path().join(".claude");
        fs::create_dir_all(&claude_dir).unwrap();
        fs::write(
            claude_dir.join("settings.json"),
            r#"{"mcpServers":{"hivemind":{"command":"hivemind"}}}"#,
        )
        .unwrap();
        let result = detect_registered_clients(home.path());
        assert!(result.contains(&"claude"));
    }

    #[test]
    fn detect_registered_clients_claude_via_user_scope_claude_json() {
        let home = tempfile::tempdir().unwrap();
        fs::write(
            home.path().join(".claude.json"),
            r#"{"mcpServers":{"hivemind":{"command":"/x/hivemind"}}}"#,
        )
        .unwrap();
        let result = detect_registered_clients(home.path());
        assert!(result.contains(&"claude"));
    }

    #[test]
    fn detect_registered_clients_ignores_claude_files_without_hivemind() {
        let home = tempfile::tempdir().unwrap();
        let claude_dir = home.path().join(".claude");
        fs::create_dir_all(&claude_dir).unwrap();
        fs::write(claude_dir.join("mcp.json"), r#"{"mcpServers":{}}"#).unwrap();
        let result = detect_registered_clients(home.path());
        assert!(!result.contains(&"claude"));
    }

    #[test]
    fn detect_registered_clients_cursor() {
        let home = tempfile::tempdir().unwrap();
        let dir = home.path().join(".cursor");
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join("mcp.json"),
            r#"{"mcpServers":{"hivemind":{"command":"hivemind"}}}"#,
        )
        .unwrap();
        let result = detect_registered_clients(home.path());
        assert!(result.contains(&"cursor"));
    }

    #[test]
    fn detect_registered_clients_kimi() {
        let home = tempfile::tempdir().unwrap();
        let dir = home.path().join(".kimi");
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join("mcp.json"),
            r#"{"mcpServers":{"hivemind":{"command":"hivemind"}}}"#,
        )
        .unwrap();
        let result = detect_registered_clients(home.path());
        assert!(result.contains(&"kimi"));
    }

    #[test]
    fn detect_registered_clients_windsurf() {
        let home = tempfile::tempdir().unwrap();
        let dir = home.path().join(".codeium").join("windsurf");
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join("mcp_config.json"),
            r#"{"mcpServers":{"hivemind":{"command":"hivemind"}}}"#,
        )
        .unwrap();
        let result = detect_registered_clients(home.path());
        assert!(result.contains(&"windsurf"));
    }

    #[test]
    fn detect_registered_clients_codex() {
        let home = tempfile::tempdir().unwrap();
        let dir = home.path().join(".codex");
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join("config.toml"),
            "\n[mcp_servers.hivemind]\ncommand = \"hivemind\"\nargs = []\n",
        )
        .unwrap();
        let result = detect_registered_clients(home.path());
        assert!(result.contains(&"codex"));
    }

    #[test]
    fn detect_registered_clients_opencode_via_config_home() {
        // detect_registered_clients reads XDG_CONFIG_HOME; hold the mutex so
        // other tests that set that env var don't interfere.
        let _lock = crate::test_env_lock::ENV_MUTEX.lock().unwrap();
        unsafe { std::env::remove_var("XDG_CONFIG_HOME") };
        let home = tempfile::tempdir().unwrap();
        let dir = home.path().join(".config").join("opencode");
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join("opencode.json"),
            r#"{"mcp":{"hivemind":{"type":"local","command":"hivemind"}}}"#,
        )
        .unwrap();
        let result = detect_registered_clients(home.path());
        assert!(result.contains(&"opencode"));
    }

    #[test]
    fn detect_registered_clients_multiple() {
        let home = tempfile::tempdir().unwrap();

        let claude_dir = home.path().join(".claude");
        fs::create_dir_all(&claude_dir).unwrap();
        fs::write(
            claude_dir.join("mcp.json"),
            r#"{"mcpServers":{"hivemind":{}}}"#,
        )
        .unwrap();

        let cursor_dir = home.path().join(".cursor");
        fs::create_dir_all(&cursor_dir).unwrap();
        fs::write(
            cursor_dir.join("mcp.json"),
            r#"{"mcpServers":{"hivemind":{}}}"#,
        )
        .unwrap();

        let result = detect_registered_clients(home.path());
        assert!(result.contains(&"claude"));
        assert!(result.contains(&"cursor"));
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn append_block_if_absent_no_trailing_newline_in_existing_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("CLAUDE.md");
        fs::write(&path, "# Existing content").unwrap();
        let (_, status) =
            append_block_if_absent(&path, "# HiveMind", "# HiveMind\nblock\n").unwrap();
        assert_eq!(status, "created");
        let content = fs::read_to_string(&path).unwrap();
        assert!(content.contains("Existing content"));
        assert!(content.contains("# HiveMind"));
    }

    // ── upsert_json_mcp ─────────────────────────────────────────────────────

    #[test]
    fn upsert_json_mcp_creates_new_file_with_mcp_servers_key() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("mcp.json");
        upsert_json_mcp(
            &path,
            "hivemind",
            serde_json::json!({"command": "hivemind"}),
        )
        .unwrap();
        let raw = fs::read_to_string(&path).unwrap();
        let val: serde_json::Value = serde_json::from_str(&raw).unwrap();
        assert!(val["mcpServers"]["hivemind"]["command"] == "hivemind");
    }

    #[test]
    fn upsert_json_mcp_uses_mcp_key_when_entry_has_type_field() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("opencode.json");
        upsert_json_mcp(
            &path,
            "hivemind",
            serde_json::json!({"type": "local", "command": "hivemind", "args": []}),
        )
        .unwrap();
        let raw = fs::read_to_string(&path).unwrap();
        let val: serde_json::Value = serde_json::from_str(&raw).unwrap();
        assert!(val["mcp"]["hivemind"]["type"] == "local");
        assert!(
            val.get("mcpServers").is_none(),
            "should use 'mcp' not 'mcpServers'"
        );
    }

    #[test]
    fn upsert_json_mcp_updates_existing_entry() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("mcp.json");
        fs::write(&path, r#"{"mcpServers":{"other":{"command":"other"}}}"#).unwrap();
        upsert_json_mcp(
            &path,
            "hivemind",
            serde_json::json!({"command": "hivemind"}),
        )
        .unwrap();
        let raw = fs::read_to_string(&path).unwrap();
        let val: serde_json::Value = serde_json::from_str(&raw).unwrap();
        assert!(
            val["mcpServers"]["other"]["command"] == "other",
            "must preserve existing"
        );
        assert!(val["mcpServers"]["hivemind"]["command"] == "hivemind");
    }

    #[test]
    fn upsert_json_mcp_creates_parent_dirs() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested").join("deep").join("mcp.json");
        upsert_json_mcp(
            &path,
            "hivemind",
            serde_json::json!({"command": "hivemind"}),
        )
        .unwrap();
        assert!(path.exists());
    }

    #[test]
    fn upsert_json_mcp_detects_mcp_key_from_existing_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("opencode.json");
        fs::write(&path, r#"{"mcp":{"existing":{"type":"local"}}}"#).unwrap();
        upsert_json_mcp(
            &path,
            "hivemind",
            serde_json::json!({"command": "hivemind"}),
        )
        .unwrap();
        let raw = fs::read_to_string(&path).unwrap();
        let val: serde_json::Value = serde_json::from_str(&raw).unwrap();
        assert!(val["mcp"]["hivemind"]["command"] == "hivemind");
        assert!(val.get("mcpServers").is_none());
    }

    #[test]
    fn home_dir_returns_a_path() {
        let h = home_dir();
        assert!(!h.as_os_str().is_empty());
    }

    #[test]
    fn exe_path_returns_non_empty_string() {
        let p = exe_path();
        assert!(!p.is_empty());
    }

    // ── render_status extra paths ────────────────────────────────────────────

    #[tokio::test]
    async fn render_status_shows_nothing_when_no_recalls_resolve() {
        use crate::{config::SyncSettings, db, store::SqliteStore};

        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let sync = SyncSettings::default();
        let database = db::open_database(&sync, db_path.to_str().unwrap())
            .await
            .unwrap();
        let conn = database.connect().unwrap();
        db::run_migrations(&conn).await.unwrap();
        let store = SqliteStore::new(conn);

        let proj = tempfile::tempdir().unwrap();
        std::fs::write(
            proj.path().join(".hivemind.toml"),
            "[project]\nname=\"empty\"\n[hooks.on_session_start]\nmax_tokens=2000\nrecalls=[\"nonexistent memory\"]\n",
        ).unwrap();
        let missing_global = proj.path().join("no-global.toml");

        let out = render_status(
            proj.path(),
            &missing_global,
            &store,
            "/tmp/x.db",
            &[],
            &default_settings(),
            false,
        )
        .await
        .unwrap();
        assert!(
            out.contains("nothing"),
            "should show nothing when no recalls resolve"
        );
        assert!(
            out.contains("skipped") || out.contains("[skipped]"),
            "should show skipped entry"
        );
    }

    #[tokio::test]
    async fn render_status_shows_local_toml_indicator() {
        use crate::{config::SyncSettings, db, store::SqliteStore};

        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let sync = SyncSettings::default();
        let database = db::open_database(&sync, db_path.to_str().unwrap())
            .await
            .unwrap();
        let conn = database.connect().unwrap();
        db::run_migrations(&conn).await.unwrap();
        let store = SqliteStore::new(conn);

        let proj = tempfile::tempdir().unwrap();
        std::fs::write(
            proj.path().join(".hivemind.toml"),
            "[project]\nname=\"local-test\"\n",
        )
        .unwrap();
        std::fs::write(proj.path().join(".hivemind.local.toml"), "").unwrap();
        let missing_global = proj.path().join("no-global.toml");

        let out = render_status(
            proj.path(),
            &missing_global,
            &store,
            "/tmp/x.db",
            &[],
            &default_settings(),
            false,
        )
        .await
        .unwrap();
        assert!(
            out.contains(".hivemind.local.toml"),
            "should mention local toml"
        );
    }

    // ── do_migrate_copy ──────────────────────────────────────────────────────

    #[test]
    fn do_migrate_copy_copies_file_and_creates_dirs() {
        let src_dir = tempfile::tempdir().unwrap();
        let dst_dir = tempfile::tempdir().unwrap();

        let legacy = src_dir.path().join("memories.db");
        fs::write(&legacy, b"sqlite data").unwrap();

        let new_path = dst_dir.path().join("sub").join("memories.db");
        do_migrate_copy(&legacy, &new_path).unwrap();

        assert!(new_path.exists());
        assert_eq!(fs::read(&new_path).unwrap(), b"sqlite data");
    }

    #[test]
    fn do_migrate_copy_fails_when_source_missing() {
        let dst_dir = tempfile::tempdir().unwrap();
        let legacy = dst_dir.path().join("nonexistent.db");
        let new_path = dst_dir.path().join("new.db");
        assert!(do_migrate_copy(&legacy, &new_path).is_err());
    }

    // ── cmd_migrate_inner ────────────────────────────────────────────────────

    #[test]
    fn cmd_migrate_inner_nothing_to_migrate_when_legacy_missing() {
        let dir = tempfile::tempdir().unwrap();
        let legacy = dir.path().join("memories.db"); // does not exist
        let new_path = dir.path().join("new").join("memories.db");
        let result = cmd_migrate_inner(&legacy, &new_path, &mut std::io::Cursor::new(b""));
        assert!(result.is_ok());
        assert!(!new_path.exists(), "new path should not be created");
    }

    #[test]
    fn cmd_migrate_inner_new_already_exists() {
        let dir = tempfile::tempdir().unwrap();
        let legacy = dir.path().join("legacy.db");
        fs::write(&legacy, b"data").unwrap();
        let new_path = dir.path().join("new.db");
        fs::write(&new_path, b"existing").unwrap();
        let result = cmd_migrate_inner(&legacy, &new_path, &mut std::io::Cursor::new(b""));
        assert!(result.is_ok());
        assert_eq!(
            fs::read(&new_path).unwrap(),
            b"existing",
            "should not overwrite"
        );
    }

    #[test]
    fn cmd_migrate_inner_cancelled_on_n_input() {
        let dir = tempfile::tempdir().unwrap();
        let legacy = dir.path().join("legacy.db");
        fs::write(&legacy, b"data").unwrap();
        let new_path = dir.path().join("new.db");
        let result = cmd_migrate_inner(&legacy, &new_path, &mut std::io::Cursor::new(b"N\n"));
        assert!(result.is_ok());
        assert!(!new_path.exists(), "should not copy when cancelled");
    }

    #[test]
    fn cmd_migrate_inner_proceeds_on_y_input() {
        let dir = tempfile::tempdir().unwrap();
        let legacy = dir.path().join("legacy.db");
        fs::write(&legacy, b"sqlite data").unwrap();
        let new_path = dir.path().join("subdir").join("memories.db");
        let result = cmd_migrate_inner(&legacy, &new_path, &mut std::io::Cursor::new(b"y\n"));
        assert!(result.is_ok());
        assert_eq!(fs::read(&new_path).unwrap(), b"sqlite data");
    }

    // ── ensure_claude_settings_hook ──────────────────────────────────────────

    #[test]
    fn scaffold_writes_claude_session_start_hook() {
        let proj = tempfile::tempdir().unwrap();
        let home = tempfile::tempdir().unwrap();
        let cfg = tempfile::tempdir().unwrap();
        scaffold(proj.path(), home.path(), cfg.path()).unwrap();
        let settings =
            fs::read_to_string(proj.path().join(".claude").join("settings.json")).unwrap();
        assert!(settings.contains("SessionStart"));
        assert!(settings.contains("hivemind session-start"));
        // idempotent
        scaffold(proj.path(), home.path(), cfg.path()).unwrap();
        let again = fs::read_to_string(proj.path().join(".claude").join("settings.json")).unwrap();
        assert_eq!(again.matches("hivemind session-start").count(), 1);
    }

    #[test]
    fn hook_merge_preserves_existing_settings() {
        let proj = tempfile::tempdir().unwrap();
        let dir = proj.path().join(".claude");
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join("settings.json"),
            r#"{"permissions":{"allow":["Bash(ls:*)"]}}"#,
        )
        .unwrap();
        ensure_claude_settings_hook(proj.path()).unwrap();
        let merged = fs::read_to_string(dir.join("settings.json")).unwrap();
        assert!(merged.contains("Bash(ls:*)"), "existing keys preserved");
        assert!(merged.contains("hivemind session-start"));
    }

    #[test]
    fn hook_merge_refuses_to_overwrite_malformed_settings() {
        let proj = tempfile::tempdir().unwrap();
        let dir = proj.path().join(".claude");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("settings.json"), "{oops").unwrap();
        let result = ensure_claude_settings_hook(proj.path());
        assert!(result.is_err(), "malformed JSON must be an error");
        assert_eq!(
            fs::read_to_string(dir.join("settings.json")).unwrap(),
            "{oops",
            "malformed file must be left untouched"
        );
    }

    // ── ensure_global_config ─────────────────────────────────────────────────

    #[test]
    fn ensure_global_config_creates_file_when_missing() {
        let _lock = crate::test_env_lock::ENV_MUTEX.lock().unwrap();
        let cfg_dir = tempfile::tempdir().unwrap();
        // SAFETY: test-only env mutation; serialised by ENV_MUTEX.
        unsafe { std::env::set_var("XDG_CONFIG_HOME", cfg_dir.path()) };
        ensure_global_config();
        unsafe { std::env::remove_var("XDG_CONFIG_HOME") };
        assert!(cfg_dir.path().join("hivemind").join("config.toml").exists());
    }

    #[test]
    fn ensure_global_config_is_idempotent() {
        let _lock = crate::test_env_lock::ENV_MUTEX.lock().unwrap();
        let cfg_dir = tempfile::tempdir().unwrap();
        let config_file = cfg_dir.path().join("hivemind").join("config.toml");
        fs::create_dir_all(config_file.parent().unwrap()).unwrap();
        fs::write(&config_file, "original").unwrap();
        unsafe { std::env::set_var("XDG_CONFIG_HOME", cfg_dir.path()) };
        ensure_global_config();
        unsafe { std::env::remove_var("XDG_CONFIG_HOME") };
        assert_eq!(fs::read_to_string(&config_file).unwrap(), "original");
    }

    // ── warn_if_not_initialized ──────────────────────────────────────────────

    #[test]
    fn warn_if_not_initialized_no_config_prints_hint() {
        let _lock = crate::test_env_lock::ENV_MUTEX.lock().unwrap();
        let cfg_dir = tempfile::tempdir().unwrap();
        // SAFETY: test-only env mutation; serialised by ENV_MUTEX.
        unsafe { std::env::set_var("XDG_CONFIG_HOME", cfg_dir.path()) };
        warn_if_not_initialized(); // exercises the "no config" branch
        unsafe { std::env::remove_var("XDG_CONFIG_HOME") };
    }

    #[test]
    fn warn_if_not_initialized_config_but_no_clients_prints_hint() {
        let _lock = crate::test_env_lock::ENV_MUTEX.lock().unwrap();
        let cfg_dir = tempfile::tempdir().unwrap();
        let home_dir_tmp = tempfile::tempdir().unwrap();
        let config_file = cfg_dir.path().join("hivemind").join("config.toml");
        fs::create_dir_all(config_file.parent().unwrap()).unwrap();
        fs::write(&config_file, "[server]\n").unwrap();
        // SAFETY: test-only env mutation; serialised by ENV_MUTEX.
        unsafe { std::env::set_var("XDG_CONFIG_HOME", cfg_dir.path()) };
        unsafe { std::env::set_var("HOME", home_dir_tmp.path()) };
        warn_if_not_initialized(); // exercises the "config found, no clients" branch
        unsafe { std::env::remove_var("XDG_CONFIG_HOME") };
        unsafe { std::env::remove_var("HOME") };
    }
}
