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
    Status,
    /// Start the HTTP server: MCP at /mcp, REST at /api/v1, plus the dashboard
    Up {
        /// Serve only MCP + REST API — no dashboard
        #[arg(long)]
        headless: bool,
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
}

#[derive(Subcommand)]
pub enum ServiceAction {
    /// Install and enable HiveMind as a user-level background service
    Install,
    /// Stop and remove the HiveMind background service
    Uninstall,
    /// Show the status of the HiveMind background service
    Status,
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
        // OAuth-connected MCP servers (claude.ai web registration)
        claude_dot.join(".credentials.json"),
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

## Suggest storing — never auto-store

When the user shares something worth persisting (preferences, project context,
design decisions), suggest: \"That seems worth remembering — should I store it?\"
Wait for explicit confirmation before calling memory_store.
";

// ── service management ────────────────────────────────────────────────────────

pub fn cmd_service_install() -> Result<()> {
    #[cfg(target_os = "macos")]
    return service_install_macos();
    #[cfg(target_os = "linux")]
    return service_install_linux();
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
fn systemd_unit_path() -> PathBuf {
    systemd_unit_dir().join("hivemind.service")
}

#[cfg(target_os = "linux")]
fn service_install_linux() -> Result<()> {
    let exe = std::env::current_exe()?;
    let unit = format!(
        "[Unit]\n\
         Description=HiveMind MCP memory server\n\
         After=network.target\n\
         \n\
         [Service]\n\
         Type=simple\n\
         ExecStart={exe}\n\
         Restart=on-failure\n\
         RestartSec=5\n\
         \n\
         [Install]\n\
         WantedBy=default.target\n",
        exe = exe.display()
    );

    let unit_path = systemd_unit_path();
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
        .args(["--user", "enable", "--now", "hivemind"])
        .status();
    match enable {
        Ok(s) if s.success() => {
            println!("HiveMind service enabled and started.");
        }
        _ => {
            println!("Warning: could not enable/start the service automatically.");
            println!("Run: systemctl --user enable --now hivemind");
        }
    }

    println!();
    println!("HiveMind will now start automatically on login.");
    println!("Check status: hivemind service status");
    Ok(())
}

#[cfg(target_os = "linux")]
fn service_uninstall_linux() -> Result<()> {
    let _ = std::process::Command::new("systemctl")
        .args(["--user", "disable", "--now", "hivemind"])
        .status();

    let unit_path = systemd_unit_path();
    if unit_path.exists() {
        std::fs::remove_file(&unit_path)?;
        println!("Removed: {}", unit_path.display());
    } else {
        println!("Unit file not found — nothing to remove.");
    }

    let _ = std::process::Command::new("systemctl")
        .args(["--user", "daemon-reload"])
        .status();

    println!("HiveMind service uninstalled.");
    Ok(())
}

#[cfg(target_os = "linux")]
fn service_status_linux() -> Result<()> {
    let status = std::process::Command::new("systemctl")
        .args(["--user", "status", "hivemind"])
        .status()?;
    if !status.success() {
        anyhow::bail!("service is not running or not installed");
    }
    Ok(())
}

// ── macOS / launchd ───────────────────────────────────────────────────────────

#[cfg(target_os = "macos")]
const LAUNCH_AGENT_LABEL: &str = "com.oxhive.hivemind";

#[cfg(target_os = "macos")]
fn launch_agent_path() -> PathBuf {
    home_dir()
        .join("Library")
        .join("LaunchAgents")
        .join(format!("{LAUNCH_AGENT_LABEL}.plist"))
}

#[cfg(target_os = "macos")]
fn service_install_macos() -> Result<()> {
    let exe = std::env::current_exe()?;
    let plist_path = launch_agent_path();
    let plist = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \
         \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n\
         <plist version=\"1.0\">\n\
         <dict>\n\
           <key>Label</key>\n\
           <string>{label}</string>\n\
           <key>ProgramArguments</key>\n\
           <array>\n\
             <string>{exe}</string>\n\
             <string>up</string>\n\
             <string>--headless</string>\n\
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
        label = LAUNCH_AGENT_LABEL,
        exe = exe.display(),
        log_dir = home_dir().join("Library").join("Logs").display(),
    );

    std::fs::create_dir_all(plist_path.parent().unwrap())?;
    std::fs::write(&plist_path, &plist)?;
    println!("Plist written: {}", plist_path.display());

    let load = std::process::Command::new("launchctl")
        .args(["load", "-w", plist_path.to_str().unwrap()])
        .status();
    match load {
        Ok(s) if s.success() => {
            println!("HiveMind service loaded and started.");
        }
        _ => {
            println!("Warning: launchctl load failed — run manually:");
            println!("  launchctl load -w {}", plist_path.display());
        }
    }

    println!();
    println!("HiveMind will now start automatically on login.");
    println!("Logs: ~/Library/Logs/hivemind.log");
    println!("Check status: hivemind service status");
    Ok(())
}

#[cfg(target_os = "macos")]
fn service_uninstall_macos() -> Result<()> {
    let plist_path = launch_agent_path();

    let _ = std::process::Command::new("launchctl")
        .args(["unload", "-w", plist_path.to_str().unwrap()])
        .status();

    if plist_path.exists() {
        std::fs::remove_file(&plist_path)?;
        println!("Removed: {}", plist_path.display());
    } else {
        println!("Plist not found — nothing to remove.");
    }

    println!("HiveMind service uninstalled.");
    Ok(())
}

#[cfg(target_os = "macos")]
fn service_status_macos() -> Result<()> {
    let output = std::process::Command::new("launchctl")
        .args(["list", LAUNCH_AGENT_LABEL])
        .output()?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    if output.status.success() && !stdout.trim().is_empty() {
        print!("{stdout}");
    } else {
        println!("HiveMind service is not loaded.");
        println!("Run: hivemind service install");
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
    // regardless of whether ~/.cargo/bin is in its subprocess PATH.
    let exe = exe_path();
    let status = std::process::Command::new("claude")
        .args(["mcp", "add", "hivemind", "--", &exe])
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

fn install_codex() -> Result<()> {
    let config_path = home_dir().join(".codex").join("config.toml");
    std::fs::create_dir_all(config_path.parent().unwrap())?;

    let existing = std::fs::read_to_string(&config_path).unwrap_or_default();
    if existing.contains("[mcp_servers.hivemind]") {
        println!("HiveMind is already registered with Codex CLI.");
        println!("Open a new Codex session to use it.");
        return Ok(());
    }

    let block = format!("\n[mcp_servers.hivemind]\ncommand = \"{}\"\nargs = []\n", exe_path());
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

pub fn cmd_status() -> Result<()> {
    let home = home_dir();
    let cwd = std::env::current_dir()?;
    let db_path = crate::db::resolve_db_path();
    let (out, clients) = with_spinner("checking status...", || {
        let clients = detect_registered_clients(&home);
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

/// Build the `hivemind status` report. `global_path` is injectable for testing.
pub async fn render_status(
    cwd: &Path,
    global_path: &Path,
    store: &crate::store::SqliteStore,
    db_path: &str,
    registered_clients: &[&str],
) -> Result<String> {
    use std::fmt::Write as _;

    let version = env!("CARGO_PKG_VERSION");
    let count = store.count().await?;
    let mut out = String::new();

    // Load the config once (if a project root is found) and reuse it for both
    // the header label and the injection preview below.
    let root = crate::config::discover_project_root(cwd);
    let config = match &root {
        Some(r) => Some(crate::config::load_config_with_global(r, global_path)?),
        None => None,
    };
    let project_label = config.as_ref().map(|c| c.project_name.as_str());

    match project_label {
        Some(label) => writeln!(out, "HiveMind v{version} — {label}")?,
        None => writeln!(out, "HiveMind v{version}")?,
    }
    writeln!(out, "─────────────────────────────────────────────────────")?;
    writeln!(out, "Server:     stdio (spawned by Claude Code)")?;
    writeln!(out, "Storage:    {db_path} ({count} memories)")?;
    writeln!(out, "Sync:       disabled (local only)")?;
    if registered_clients.is_empty() {
        writeln!(out, "AI clients: none registered")?;
    } else {
        writeln!(out, "AI clients: {}", registered_clients.join(", "))?;
    }
    writeln!(out)?;

    let (Some(root), Some(config)) = (root, config) else {
        writeln!(out, "No .hivemind.toml found in this directory tree.")?;
        writeln!(
            out,
            "Run `hivemind init` to set up memory hooks for this project."
        )?;
        return Ok(out);
    };

    let result = crate::session::execute_session_start(&config, store).await?;

    writeln!(out, "Project:    {}", config.project_name)?;
    writeln!(
        out,
        "Config:     .hivemind.toml{}",
        if root.join(".hivemind.local.toml").is_file() {
            " + .hivemind.local.toml"
        } else {
            ""
        }
    )?;
    writeln!(out)?;
    writeln!(out, "On session start will inject:")?;
    if result.loaded.is_empty() {
        writeln!(out, "  (nothing — no recalls configured or none resolved)")?;
    }
    for entry in &result.loaded {
        let local = if matches!(entry.source, crate::config::RecallSource::Local) {
            "  (local)"
        } else {
            ""
        };
        writeln!(
            out,
            "  {:<40} ~{} tokens{}",
            entry.entry.title, entry.tokens, local
        )?;
    }
    for skip in &result.skipped {
        writeln!(
            out,
            "  [skipped]   {:<40} ({})",
            skip.query,
            skip.reason.as_str()
        )?;
    }
    writeln!(
        out,
        "  ──────────────────────────────────────────────────────────"
    )?;
    writeln!(out, "  Total:      ~{} tokens", result.used_tokens)?;
    writeln!(out, "  Budget:     {} tokens", result.max_tokens)?;
    let headroom = if result.truncated() { "⚠" } else { "✓" };
    writeln!(
        out,
        "  Remaining:  ~{} tokens  {headroom}",
        result.remaining()
    )?;
    writeln!(out)?;
    writeln!(
        out,
        "On file open rules:    {} active",
        config.file_open_rule_count
    )?;
    writeln!(
        out,
        "On mention triggers:   {} (reserved, not yet active)",
        config.mention_trigger_count
    )?;

    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

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
            .store(
                &id,
                "golang preferences",
                "uber/zap, sqlc, pgx v5",
                &["golang".to_string()],
                None,
            )
            .await
            .unwrap();

        let proj = tempfile::tempdir().unwrap();
        std::fs::write(
            proj.path().join(".hivemind.toml"),
            "[project]\nname=\"demo\"\n[hooks.on_session_start]\nmax_tokens=2000\nrecalls=[\"golang preferences\"]\n",
        ).unwrap();
        let missing_global = proj.path().join("no-global.toml");

        let out = render_status(proj.path(), &missing_global, &store, "/tmp/x.db", &[])
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
        let out = render_status(proj.path(), &missing_global, &store, "/tmp/x.db", &[])
            .await
            .unwrap();
        assert!(
            out.contains("hivemind init"),
            "suggests init when no config"
        );
    }
}
