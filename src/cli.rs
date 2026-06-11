use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::{Path, PathBuf};

#[derive(Parser)]
#[command(name = "hivemind", version, about = "HiveMind — persistent memory for AI coding agents")]
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
    std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."))
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
        write_if_absent(
            &project_root.join(".hivemind.local.toml"),
            LOCAL_TOML,
        )?,
        ensure_line(
            &project_root.join(".gitignore"),
            ".hivemind.local.toml",
        )?,
        write_if_absent(
            &project_root.join("CLAUDE.md"),
            &project_claude_md(&project_name),
        )?,
        append_block_if_absent(
            &home.join(".claude").join("CLAUDE.md"),
            GLOBAL_CLAUDE_MARKER,
            GLOBAL_CLAUDE_BLOCK,
        )?,
        write_if_absent(
            &config_dir.join("config.toml"),
            GLOBAL_CONFIG,
        )?,
    ];

    Ok(report)
}

fn write_if_absent(path: &Path, contents: &str) -> Result<(PathBuf, &'static str)> {
    if path.exists() {
        return Ok((path.to_path_buf(), "exists"));
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, contents)?;
    Ok((path.to_path_buf(), "created"))
}

/// Ensure `line` is present in the file (create the file if needed).
fn ensure_line(path: &Path, line: &str) -> Result<(PathBuf, &'static str)> {
    let existing = std::fs::read_to_string(path).unwrap_or_default();
    if existing.lines().any(|l| l.trim() == line) {
        return Ok((path.to_path_buf(), "exists"));
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut body = existing;
    if !body.is_empty() && !body.ends_with('\n') {
        body.push('\n');
    }
    body.push_str(line);
    body.push('\n');
    std::fs::write(path, body)?;
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
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut body = existing;
    if !body.is_empty() && !body.ends_with('\n') {
        body.push('\n');
    }
    if !body.is_empty() {
        body.push('\n');
    }
    body.push_str(block);
    std::fs::write(path, body)?;
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

    let root = crate::config::discover_project_root(cwd);
    let project_label = match &root {
        Some(r) => crate::config::load_config_with_global(r, global_path)
            .map(|c| c.project_name)
            .unwrap_or_else(|_| "—".to_string()),
        None => "—".to_string(),
    };

    writeln!(out, "HiveMind v{version} — {project_label}")?;
    writeln!(out, "─────────────────────────────────────────────────────")?;
    writeln!(out, "Server:     stdio (spawned by Claude Code)")?;
    writeln!(out, "Storage:    {db_path} ({count} memories)")?;
    writeln!(out, "Sync:       disabled (local only)")?;
    writeln!(out)?;

    let Some(root) = root else {
        writeln!(out, "No .hivemind.toml found in this directory tree.")?;
        writeln!(out, "Run `hivemind init` to set up memory hooks for this project.")?;
        return Ok(out);
    };

    let config = crate::config::load_config_with_global(&root, global_path)?;
    let result = crate::session::execute_session_start(&config, store)?;

    writeln!(out, "Project:    {}", config.project_name)?;
    writeln!(out, "Config:     .hivemind.toml{}", if root.join(".hivemind.local.toml").is_file() { " + .hivemind.local.toml" } else { "" })?;
    writeln!(out)?;
    writeln!(out, "On session start will inject:")?;
    if result.loaded.is_empty() {
        writeln!(out, "  (nothing — no recalls configured or none resolved)")?;
    }
    for entry in &result.loaded {
        let layer = format!("[{}]", entry.entry.layer);
        let local = if matches!(entry.source, crate::config::RecallSource::Local) { "  (local)" } else { "" };
        writeln!(out, "  {:<11} {:<40} ~{} tokens{}", layer, entry.entry.title, entry.tokens, local)?;
    }
    for skip in &result.skipped {
        writeln!(out, "  [skipped]   {:<40} ({})", skip.query, skip.reason.as_str())?;
    }
    writeln!(out, "  ──────────────────────────────────────────────────────────")?;
    writeln!(out, "  Total:      ~{} tokens", result.used_tokens)?;
    writeln!(out, "  Budget:     {} tokens", result.max_tokens)?;
    let headroom = if result.truncated() { "⚠" } else { "✓" };
    writeln!(out, "  Remaining:  ~{} tokens  {headroom}", result.remaining())?;
    writeln!(out)?;
    writeln!(out, "On file open rules:    {} active", config.file_open_rule_count)?;
    writeln!(out, "On mention triggers:   {} (reserved, not yet active)", config.mention_trigger_count)?;

    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

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
    fn render_status_previews_injection() {
        use crate::db;
        use crate::model::{Layer, MemoryType, NewMemory};
        use crate::store::SqliteStore;

        let conn = rusqlite::Connection::open_in_memory().unwrap();
        db::create_schema(&conn).unwrap();
        let store = SqliteStore::new(conn);
        store.store(NewMemory {
            title: "golang preferences".to_string(),
            content: "uber/zap, sqlc, pgx v5".to_string(),
            layer: Layer::Personal,
            memory_type: MemoryType::Preference,
            tags: vec!["golang".to_string()],
            project: None,
            source: None,
        }).unwrap();

        let proj = tempfile::tempdir().unwrap();
        std::fs::write(
            proj.path().join(".hivemind.toml"),
            "[project]\nname=\"demo\"\n[hooks.on_session_start]\nmax_tokens=2000\nrecalls=[\"golang preferences\"]\n",
        ).unwrap();
        let missing_global = proj.path().join("no-global.toml");

        let out = render_status(proj.path(), &missing_global, &store, "/tmp/x.db").unwrap();
        assert!(out.contains("demo"), "shows project name");
        assert!(out.contains("golang preferences"), "lists the injected memory");
        assert!(out.contains("Budget:"), "shows the budget line");
        assert!(out.contains("1 memories") || out.contains("1 memorie"), "shows memory count");
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
        assert!(out.contains("hivemind init"), "suggests init when no config");
    }
}
