# HiveMind

> 🚧 **Under active development** — APIs and config formats may change between releases.

Persistent memory for AI coding agents. HiveMind runs a local MCP server that gives Claude Code (and other AI agents) access to a SQLite-backed memory store — so context, preferences, and project knowledge survive across sessions.

## How it works

1. You run `hivemind up` once to start the local server.
2. Claude Code connects to it via MCP.
3. At the start of every session, Claude automatically recalls the memories configured for your project.
4. You ask Claude to store anything worth keeping — it never auto-stores.

### The session start flow in detail

When you open a new Claude Code session in a project that has `.hivemind.toml`:

1. Claude reads CLAUDE.md, which instructs it to call `hivemind_session_start` once.
2. Claude calls the MCP tool with the current project path.
3. HiveMind reads `.hivemind.toml` (and `.hivemind.local.toml` if present), resolves each `recalls` entry against the SQLite database (by exact title, then FTS), and returns the results as structured JSON — staying within `max_tokens`.
4. Claude incorporates the returned memories silently and proceeds with your request.

That's it. One tool call, one round-trip to the database, zero per-prompt overhead after that.

### Context budget

The `max_tokens` cap prevents session start from consuming too much of Claude's context window. Memories are loaded in order; if an entry would push past the budget, it is skipped (but later, smaller entries still get a chance). The `hivemind status` command shows a preview of exactly what would be injected and how many tokens it costs.

---

## Fetching memories during a session

The `recalls` list in `.hivemind.toml` is only for **automatic injection at session start**. You can always fetch any memory on demand during a session:

- **By title or ID** — ask Claude: *"recall the memory titled 'golang preferences'"* → Claude calls `memory_recall`
- **By keyword** — ask Claude: *"search my memories for postgres"* → Claude calls `memory_search` (FTS, returns snippets)
- **Browse all** — use the `/memory-list` prompt

Memories not listed in `recalls` are not gone — they're just not auto-loaded. They live in the database and are available any time you ask.

---

## How HiveMind differs from Claude Code's built-in hooks

Claude Code has its own hook system in `.claude/settings.json`:

```json
{
  "hooks": {
    "UserPromptSubmit": [
      { "hooks": [{ "type": "command", "command": "my-memory-tool search" }] }
    ]
  }
}
```

This runs a shell command and injects its stdout into the conversation. It works, but the trade-offs differ:

| | Claude Code `UserPromptSubmit` hook | HiveMind `[hooks.on_session_start]` |
|---|---|---|
| **When it runs** | On **every message** you send | **Once** per session |
| **Context overhead** | Added to every prompt, every time | Injected once; zero cost after that |
| **Token budget** | None — dumps all output unconditionally | `max_tokens` cap with per-entry skipping |
| **Data source** | Anything a shell command outputs | SQLite FTS store, queryable by title/ID/keyword |
| **Selectivity** | Whatever the command returns | You specify exactly which memories per project |
| **Persistence** | Stateless — reruns the command fresh each call | Stateful — memories survive machines and reinstalls |
| **On-demand access** | Only what the hook returns | Full MCP tools (`memory_recall`, `memory_search`, etc.) |

**The short version:** the hook approach re-injects context on every single message, which burns tokens proportionally to how often you prompt. HiveMind injects once at session start and then stays out of the way — the rest of the session is just Claude using what it loaded, with on-demand tools if it needs more.

---

## Installation

```sh
cargo install oxhivemind
```

### Claude Code

```sh
hivemind mcp install claude
```

This runs `claude mcp add hivemind --transport http http://127.0.0.1:3456/mcp` for you.

### OpenCode

```sh
hivemind mcp install opencode
```

Writes to `~/.config/opencode/opencode.json` (uses the `opencode` CLI if available, otherwise writes the config file directly). Manual config:

```json
{
  "$schema": "https://opencode.ai/config.json",
  "mcp": {
    "hivemind": {
      "type": "remote",
      "url": "http://127.0.0.1:3456/mcp",
      "enabled": true
    }
  }
}
```

Docs: [opencode.ai/docs/mcp-servers](https://opencode.ai/docs/mcp-servers/)

### Kimi Code CLI

```sh
hivemind mcp install kimi
```

Uses the `kimi` CLI if available, otherwise writes to `~/.kimi/mcp.json` directly. Manual config:

```json
{
  "mcpServers": {
    "hivemind": {
      "url": "http://127.0.0.1:3456/mcp"
    }
  }
}
```

Docs: [moonshotai.github.io/kimi-cli/en/customization/mcp.html](https://moonshotai.github.io/kimi-cli/en/customization/mcp.html)

### OpenAI Codex CLI

```sh
hivemind mcp install codex
```

Appends to `~/.codex/config.toml`. Manual config:

```toml
[mcp_servers.hivemind]
url = "http://127.0.0.1:3456/mcp"
```

Docs: [developers.openai.com/codex/mcp](https://developers.openai.com/codex/mcp)

### Cursor

```sh
hivemind mcp install cursor
```

Writes to `~/.cursor/mcp.json`. Restart Cursor after running. Manual config:

```json
{
  "mcpServers": {
    "hivemind": {
      "url": "http://127.0.0.1:3456/mcp"
    }
  }
}
```

Docs: [cursor.com/docs/mcp](https://cursor.com/docs/mcp)

### Windsurf

```sh
hivemind mcp install windsurf
```

Writes to `~/.codeium/windsurf/mcp_config.json`. Restart Windsurf after running. Manual config:

```json
{
  "mcpServers": {
    "hivemind": {
      "serverUrl": "http://127.0.0.1:3456/mcp"
    }
  }
}
```

Note: Windsurf uses `serverUrl` where other clients use `url`.

Docs: [docs.windsurf.com/windsurf/cascade/mcp](https://docs.windsurf.com/windsurf/cascade/mcp)

### Other MCP-compatible clients

Any client that supports the MCP streamable HTTP transport can connect to:

```
http://127.0.0.1:3456/mcp
```

No authentication is required for local connections.

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

### Run as a background service (recommended)

Instead of keeping a terminal open, install HiveMind as a user-level service so it starts automatically on login:

```sh
hivemind service install
```

This writes a unit file (Linux) or launchd plist (macOS) and enables it immediately — no `sudo` required.

| Platform | Mechanism | Unit file location |
|----------|-----------|-------------------|
| Linux | systemd user unit | `~/.config/systemd/user/hivemind.service` |
| macOS | launchd LaunchAgent | `~/Library/LaunchAgents/com.oxhive.hivemind.plist` |

On macOS, logs are written to `~/Library/Logs/hivemind.log`.

```sh
hivemind service status    # check if running
hivemind service uninstall # stop and remove
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
hivemind up                      Start the server (MCP + REST API + dashboard)
hivemind up --headless           Start without the dashboard UI
hivemind init                    Scaffold config files for the current project
hivemind status                  Show config, memory count, and session-start preview
hivemind mcp install claude      Register with Claude Code
hivemind mcp install opencode    Register with OpenCode
hivemind mcp install kimi        Register with Kimi Code CLI
hivemind mcp install codex       Register with OpenAI Codex CLI
hivemind mcp install cursor      Register with Cursor
hivemind mcp install windsurf    Register with Windsurf
hivemind service install         Install and enable as a background service
hivemind service uninstall       Stop and remove the background service
hivemind service status          Show background service status
hivemind dashboard --open        Open the dashboard (requires server running)
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

`recalls` is a list of memory titles to auto-inject at session start. Each entry is looked up by exact title, then falls back to FTS. The combined size is capped at `max_tokens`.

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
remote_url = ""        # sqld server URL or Oxhive hosted endpoint
api_key = ""           # sqld auth token, or Oxhive account key
interval_seconds = 300
sync_on_store = true
sync_on_startup = true
```

`$XDG_CONFIG_HOME/hivemind/config.toml` is used instead if `XDG_CONFIG_HOME` is set.

### Environment variables

| Variable | Default | Description |
|----------|---------|-------------|
| `HIVEMIND_DB_PATH` | `~/.hivemind/memories.db` | Path to the SQLite database |

---

## Sync (optional)

HiveMind can replicate memories to a remote server — useful for sharing across machines or keeping a remote backup. Sync uses [libsql](https://github.com/tursodatabase/libsql) embedded replication: the local database stays fully functional offline, and `hivemind up` periodically replicates writes to the remote primary.

```toml
[sync]
enabled = true
remote_url = "http://pi.local:8080"   # see options below
api_key = "your-auth-token"           # see options below
interval_seconds = 300                # background sync every 5 minutes
sync_on_store = true                  # also sync immediately after each memory is stored
sync_on_startup = true                # sync once when the server starts
```

Two `remote_url` targets are supported:

| Setup | `remote_url` points to | `api_key` |
|-------|------------------------|-----------|
| **Self-hosted** | Your own [sqld](https://github.com/tursodatabase/libsql/tree/main/libsql-server) server | sqld auth token — leave empty if sqld has no auth configured |
| **Oxhive hosted** *(coming soon)* | `https://sync.oxhive.dev` | Your Oxhive account key |

`api_key` is never sent to Claude or the dashboard — it is only used during replication.

---

## Checking your setup

```sh
hivemind status
```

Shows the active config, memory count, database path, and a preview of exactly what will be injected at the next session start — including token usage vs budget.

---

## Troubleshooting

### "hint: looks like you haven't run `hivemind init` yet"

You ran `hivemind up` or `hivemind status` before initializing. Run `hivemind init` in your project directory first:

```sh
cd ~/projects/myapp
hivemind init
```

This creates `.hivemind.toml`, scaffolds CLAUDE.md, and writes the global config file that makes the hint go away.

### "hint: no AI client is registered with HiveMind yet"

You ran `hivemind init` but haven't told your AI client about the MCP server yet. The server will start, but your AI client won't connect to it. Run the install command for your client once:

```sh
hivemind mcp install claude      # Claude Code
hivemind mcp install cursor      # Cursor
hivemind mcp install windsurf    # Windsurf
hivemind mcp install opencode    # OpenCode
hivemind mcp install kimi        # Kimi Code CLI
hivemind mcp install codex       # OpenAI Codex CLI
```

This only needs to be done once per machine, not per project.

**How HiveMind detects whether a client is registered:**

| Client | Detection method |
|--------|-----------------|
| Claude Code | Runs `claude mcp list` and checks for "hivemind" |
| Cursor | Reads `~/.cursor/mcp.json`, checks for "hivemind" |
| Windsurf | Reads `~/.codeium/windsurf/mcp_config.json`, checks for "hivemind" |
| Kimi | Reads `~/.kimi/mcp.json`, checks for "hivemind" |
| OpenCode | Reads `~/.config/opencode/opencode.json` (or `$XDG_CONFIG_HOME`), checks for "hivemind" |
| Codex CLI | Reads `~/.codex/config.toml`, checks for `[mcp_servers.hivemind]` |

Detection failures are silent — a missing config file or unavailable CLI simply means "not registered." If you've registered a client manually and still see the hint, verify that "hivemind" appears in the config file at the path listed above.

### Claude connects but session start fails

If `hivemind_session_start` errors during a session, the most likely cause is that the server isn't running. Check:

```sh
hivemind service status    # if installed as a service
# or
hivemind up                # to start manually
```

### Session start succeeds but no memories are injected

`hivemind_session_start` loads only the entries listed in `[hooks.on_session_start].recalls` in `.hivemind.toml`. If that list is empty or no entries match titles in the database, nothing is injected. Check with:

```sh
hivemind status    # previews exactly what would be injected
```

---

## FAQ

**Does HiveMind inject memories into every prompt I send?**

No. Memories are injected once — when Claude calls `hivemind_session_start` at the start of the session. After that, the loaded memories are part of the conversation context, but nothing extra is added per prompt. Tools like `UserPromptSubmit` hooks in `.claude/settings.json` run on every message; HiveMind does not.

**What's the difference between HiveMind's session start and a Claude Code `UserPromptSubmit` hook?**

A `UserPromptSubmit` hook runs a shell command and appends its output to every message you send — unconditionally, on every prompt, with no token budget. HiveMind runs once per session, respects a `max_tokens` cap, and gives you per-project control over exactly which memories to load. See the [comparison table](#how-hivemind-differs-from-claude-codes-built-in-hooks) for the full breakdown.

**Can I fetch memories that aren't listed in `recalls`?**

Yes. `recalls` is only the auto-inject list for session start. Every memory in the database is available on demand at any time — ask Claude to recall it by title or ID (`memory_recall`), or search by keyword (`memory_search`). Nothing is hidden or inaccessible.

**Does Claude store memories automatically as we chat?**

No. HiveMind never auto-stores. Claude only writes a memory when you explicitly ask it to — *"remember this"*, *"store that preference"*. This keeps the store intentional and free of noise.

**What happens if a memory doesn't fit within `max_tokens`?**

It gets skipped. HiveMind loads recalls in order; if an entry would push past the budget, it skips that entry and continues with the next one — a later, smaller entry can still fit. Skipped entries are reported in the result. Use `hivemind status` to preview what would be loaded and how many tokens it costs before opening a session.

**Can I have different recalls per project?**

Yes. Each project has its own `.hivemind.toml` with its own `recalls` list and `max_tokens`. Your personal additions go in `.hivemind.local.toml` (gitignored), which stacks on top of the project config.

**Do my teammates see my personal memories?**

No. Memories stored with `layer = "personal"` follow you, not the repo. Only `layer = "workspace"` memories are project-scoped. The `.hivemind.local.toml` file is gitignored, and your personal layer is local to your machine unless you configure sync.

**Is the MCP connection authenticated?**

The MCP endpoint (`/mcp`) and the REST API (`/api/v1/*`) are unauthenticated — they bind to `127.0.0.1` by default, so only processes on your local machine can reach them. The `api_key` under `[sync]` is your auth token for the remote sync target (sqld token for self-hosted, account key for Oxhive hosted); it is used only during replication and has nothing to do with Claude's connection to HiveMind.

**The server isn't running — will Claude error out?**

If `hivemind up` isn't running when Claude calls `hivemind_session_start`, the MCP call will fail. Claude will typically note this and continue the session without memory context. Your work isn't blocked — you just won't have the injected memories for that session.

**Can I use HiveMind with agents other than Claude Code?**

Yes, as long as the agent supports MCP over HTTP (streamable HTTP transport). The REST API is also fully accessible for custom integrations.

**Where is the database stored?**

`~/.hivemind/memories.db` by default. Override with the `HIVEMIND_DB_PATH` environment variable. It's a plain SQLite file — you can back it up, copy it between machines, or inspect it directly.

---

## Integrating with HiveMind

Detailed docs for connecting your own app, script, or AI agent to HiveMind's MCP tools and REST API: [docs/INTEGRATING.md](docs/INTEGRATING.md)

---

## License

AGPL-3.0-only
