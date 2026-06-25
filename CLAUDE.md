# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Commands

```sh
just build          # cargo build
just test           # cargo test (unit + integration)
just install        # cargo install --path . --force (installs hivemind binary locally)
just dashboard      # bun run build (builds the embedded dashboard UI)

# Run a single test by name
cargo test <test_name>

# Run integration tests only
cargo test --test api_integration

# Release (requires cargo-release)
just release-patch  # bumps patch version, tags, pushes
just release-minor
just release-major
```

The dashboard (`dashboard/`) is a Bun/Vite frontend embedded into the binary via `include_dir`. Run `just dashboard` after UI changes before building the binary.

## Architecture

The binary is `hivemind` (crate name `oxhivemind`). The library crate (`src/lib.rs`) exports the MCP server impl; the binary crate (`src/main.rs`) wires CLI commands.

**Request path for `hivemind up`:**
1. `cli.rs` parses args and calls `http::serve()`
2. `http.rs` builds an Axum router combining:
   - `/mcp` — MCP streamable HTTP transport (via `rmcp`), backed by `server::HiveMind`
   - `/api/v1/*` — REST API (`api.rs`)
   - `/` and static assets — embedded dashboard (`include_dir`)
3. Both surfaces share a single `Arc<SqliteStore>` (`store.rs`)

**Storage layer:**
- `db.rs` — opens a `libsql` database (local SQLite or remote replica for sync), runs migrations from `migrations/`
- `store.rs` — all SQL queries; `SqliteStore` wraps a `libsql::Connection`
- Conflict on `id` upserts (not title); titles are not unique

**Session start flow (`hivemind_session_start` MCP tool):**
- `session.rs` resolves each `recalls` entry from `.hivemind.toml` against the store (exact title → FTS fallback), respects `max_tokens` budget (`budget.rs` uses tiktoken), returns structured JSON

**Version stamping:**
- `build.rs` sets `HIVEMIND_GIT_SHA` and `HIVEMIND_IS_TAGGED` at compile time via `git describe --exact-match --tags HEAD`
- `cmd_version()` in `cli.rs` prints `hivemind <version>` when tagged, `hivemind <sha>-dev` otherwise

**Integration tests** (`tests/api_integration.rs`) spin up a real Axum router against a `tempfile` SQLite DB using tower's `oneshot` helper — no network required.

**Config files consumed at runtime:**
- `~/.config/hivemind/config.toml` — global (server port, sync settings)
- `.hivemind.toml` — per-project recalls list and token budget
- `.hivemind.local.toml` — personal additions, gitignored
