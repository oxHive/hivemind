use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::{Path, PathBuf};

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
}

#[derive(Subcommand)]
pub enum McpAction {
    /// Register HiveMind as an MCP server in a supported AI coding client
    Install {
        /// Client to register with: claude
        client: String,
    },
}

pub fn cmd_init() -> Result<()> {
    let cwd = std::env::current_dir()?;
    let home = home_dir();
    let report = scaffold(&cwd, &home, &crate::config::global_config_dir())?;
    for (path, status) in &report {
        println!("  {status:7}  {}", path.display());
    }
    println!("\nHiveMind initialized. Restart your Claude Code session to load memory hooks.");
    Ok(())
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
         description = \"TODO: short project description\"\n\
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
remote_url = \"\"\n\
api_key = \"\"\n\
interval_seconds = 300\n\
sync_on_store = true\n\
sync_on_startup = true\n";

const GLOBAL_CLAUDE_MARKER: &str = "# HiveMind Memory System";

const GLOBAL_CLAUDE_BLOCK: &str = "# HiveMind Memory System

You have access to HiveMind via MCP tools: memory_store, memory_recall,
memory_search, memory_update, memory_delete, hivemind_session_start.

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

pub fn cmd_mcp_install(client: &str) -> Result<()> {
    match client {
        "claude" => install_claude(),
        other => anyhow::bail!("unknown client \"{other}\" — supported clients: claude"),
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
        println!("Run `hivemind up` to start the server, then open a new Claude Code session.");
        return Ok(());
    }

    // Register.
    let status = std::process::Command::new("claude")
        .args([
            "mcp",
            "add",
            "hivemind",
            "--transport",
            "http",
            "http://127.0.0.1:3456/mcp",
        ])
        .status()?;

    if !status.success() {
        anyhow::bail!("claude mcp add failed — run `claude mcp list` to inspect existing servers");
    }

    println!("HiveMind registered with Claude Code.");
    println!();
    println!("Next steps:");
    println!("  1. Run `hivemind up` to start the server");
    println!("  2. Open a new Claude Code session");
    println!("  3. Type /memory-status to verify");
    Ok(())
}

pub fn cmd_status() -> Result<()> {
    let cwd = std::env::current_dir()?;
    let db_path = crate::db::resolve_db_path();
    let conn = crate::db::open(&db_path)?;
    let store = crate::store::SqliteStore::new(conn);
    let out = render_status(&cwd, &crate::config::global_config_path(), &store, &db_path)?;
    println!("{out}");
    Ok(())
}

/// Build the `hivemind status` report. `global_path` is injectable for testing.
pub fn render_status(
    cwd: &Path,
    global_path: &Path,
    store: &crate::store::SqliteStore,
    db_path: &str,
) -> Result<String> {
    use std::fmt::Write as _;

    let version = env!("CARGO_PKG_VERSION");
    let count = store.count()?;
    let mut out = String::new();

    // Load the config once (if a project root is found) and reuse it for both
    // the header label and the injection preview below.
    let root = crate::config::discover_project_root(cwd);
    let config = match &root {
        Some(r) => Some(crate::config::load_config_with_global(r, global_path)?),
        None => None,
    };
    let project_label = config
        .as_ref()
        .map(|c| c.project_name.as_str())
        .unwrap_or("—");

    writeln!(out, "HiveMind v{version} — {project_label}")?;
    writeln!(out, "─────────────────────────────────────────────────────")?;
    writeln!(out, "Server:     stdio (spawned by Claude Code)")?;
    writeln!(out, "Storage:    {db_path} ({count} memories)")?;
    writeln!(out, "Sync:       disabled (local only)")?;
    writeln!(out)?;

    let (Some(root), Some(config)) = (root, config) else {
        writeln!(out, "No .hivemind.toml found in this directory tree.")?;
        writeln!(
            out,
            "Run `hivemind init` to set up memory hooks for this project."
        )?;
        return Ok(out);
    };

    let result = crate::session::execute_session_start(&config, store)?;

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
        let layer = format!("[{}]", entry.entry.layer);
        let local = if matches!(entry.source, crate::config::RecallSource::Local) {
            "  (local)"
        } else {
            ""
        };
        writeln!(
            out,
            "  {:<11} {:<40} ~{} tokens{}",
            layer, entry.entry.title, entry.tokens, local
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

    #[test]
    fn render_status_previews_injection() {
        use crate::db;
        use crate::model::{Layer, MemoryType, NewMemory};
        use crate::store::SqliteStore;

        let conn = rusqlite::Connection::open_in_memory().unwrap();
        db::create_schema(&conn).unwrap();
        let store = SqliteStore::new(conn);
        store
            .store(NewMemory {
                title: "golang preferences".to_string(),
                content: "uber/zap, sqlc, pgx v5".to_string(),
                layer: Layer::Personal,
                memory_type: MemoryType::Preference,
                tags: vec!["golang".to_string()],
                project: None,
                source: None,
            })
            .unwrap();

        let proj = tempfile::tempdir().unwrap();
        std::fs::write(
            proj.path().join(".hivemind.toml"),
            "[project]\nname=\"demo\"\n[hooks.on_session_start]\nmax_tokens=2000\nrecalls=[\"golang preferences\"]\n",
        ).unwrap();
        let missing_global = proj.path().join("no-global.toml");

        let out = render_status(proj.path(), &missing_global, &store, "/tmp/x.db").unwrap();
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
    }

    #[test]
    fn render_status_without_config_reports_missing() {
        use crate::db;
        use crate::store::SqliteStore;
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        db::create_schema(&conn).unwrap();
        let store = SqliteStore::new(conn);

        let proj = tempfile::tempdir().unwrap();
        let missing_global = proj.path().join("no-global.toml");
        let out = render_status(proj.path(), &missing_global, &store, "/tmp/x.db").unwrap();
        assert!(
            out.contains("hivemind init"),
            "suggests init when no config"
        );
    }
}
