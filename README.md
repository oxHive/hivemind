# HiveMind

Persistent memory for AI coding agents. HiveMind runs a local MCP server that gives Claude Code (and other AI agents) access to a SQLite-backed memory store — so context, preferences, and project knowledge survive across sessions.

## How it works

1. You run `hivemind up` once to start the local server.
2. Claude Code connects to it via MCP.
3. At the start of every session, Claude automatically recalls the memories configured for your project.
4. You ask Claude to store anything worth keeping — it never auto-stores.

---

## Installation

```sh
cargo install oxhivemind
```

Then register it with Claude Code:

```sh
hivemind mcp install claude
```

This runs `claude mcp add hivemind --transport http http://127.0.0.1:3456/mcp` for you.

---

## Quick start

```sh
# 1. Start the server (keep this running in a terminal)
hivemind up

# 2. In a new terminal, go to your project and initialise it
cd ~/projects/myapp
hivemind init

# 3. Open a new Claude Code session — memory hooks are now active
```

`hivemind init` creates:

| File | Description |
|------|-------------|
| `.hivemind.toml` | Project config (commit this) |
| `.hivemind.local.toml` | Personal recalls, gitignored |
| `CLAUDE.md` | Instructs Claude to call `hivemind_session_start` |
| `.gitignore` | Adds `.hivemind.local.toml` entry |

It also appends a HiveMind block to `~/.claude/CLAUDE.md` (preserving any existing content) so Claude knows how to use the MCP tools globally.

> **If you already have a project `CLAUDE.md`**, init will not modify it. Add this line manually:
> ```
> At the start of every session, call `hivemind_session_start` if .hivemind.toml exists in the project root.
> ```

---

## Commands

```
hivemind up                  Start the server (MCP + REST API + dashboard)
hivemind up --headless       Start without the dashboard UI
hivemind init                Scaffold config files for the current project
hivemind status              Show config, memory count, and session-start preview
hivemind mcp install claude  Register with Claude Code
hivemind dashboard --open    Open the dashboard (requires server running)
```

---

## Configuration

### Project config — `.hivemind.toml`

Committed to the repo. Shared across the team.

```toml
[project]
name = "myapp"
layer = "workspace"
description = "Short project description"

[hooks.on_session_start]
max_tokens = 2000
recalls = [
  "golang preferences",
  "project/myapp",
]
```

`recalls` is a list of memory titles to auto-inject at session start. Each entry is looked up by exact title. The combined size is capped at `max_tokens`.

### Personal config — `.hivemind.local.toml`

Gitignored. Your own additions on top of the team config.

```toml
[hooks.on_session_start]
recalls = ["my personal style notes"]
max_tokens = 500   # added to the team budget
```

### Global config — `~/.config/hivemind/config.toml`

Created by `hivemind init`. Applies to all projects.

```toml
[defaults]
max_inject_tokens = 2000   # default token budget when project doesn't set one

[server]
host = "127.0.0.1"
port = 3456

[dashboard]
port = 3457

[sync]
enabled = false
remote_url = ""
api_key = ""
interval_seconds = 300
sync_on_store = true
sync_on_startup = true
```

`$XDG_CONFIG_HOME/hivemind/config.toml` is used instead if `XDG_CONFIG_HOME` is set.

### Environment variables

| Variable | Default | Description |
|----------|---------|-------------|
| `HIVEMIND_DB_PATH` | `~/.local/share/hivemind/memory.db` | Path to the SQLite database |

---

## Sync (optional)

HiveMind can sync memories to a second instance — useful for sharing across machines or teammates.

```toml
[sync]
enabled = true
remote_url = "http://pi.local:3456"
api_key = "your-api-key"
interval_seconds = 300
sync_on_store = true     # push immediately when a memory is stored
sync_on_startup = true   # pull on server start
```

---

## Checking your setup

```sh
hivemind status
```

Shows the active config, memory count, database path, and a preview of exactly what will be injected at the next session start — including token usage vs budget.

---

## License

AGPL-3.0-only
