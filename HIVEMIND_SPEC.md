# HiveMind — Complete Product & Technical Spec

## Claude Code Handover Document

_Generated from product brainstorm session — June 2026_

---

## 1. What Is HiveMind

HiveMind is a **persistent, cross-project memory layer for AI coding agents**, implemented
as an MCP (Model Context Protocol) server. It solves the context amnesia problem — Claude
Code starts every session with zero memory of the developer's preferences, past projects,
and cross-project context.

### Core Pain Points It Solves

| Pain                      | Example                                                                                          |
| ------------------------- | ------------------------------------------------------------------------------------------------ |
| Preference re-explanation | User has to re-tell Claude their Golang stack (clean arch, uber/zap, sqlc, pgx v5) every session |
| Device gap                | Preferences stored on PC are unknown to Claude Code on laptop                                    |
| Cross-project blindness   | Working on API A, Claude doesn't know API B it connects to                                       |
| Professional knowledge    | Past projects, CV content, career history not available to Claude                                |

### Product Name

- Product: **HiveMind** (name WIP, open to change)
- Company: **Oxhive**

---

## 2. Memory Model — Two Layers

### Layer 1 — Personal Memory _(travels with the developer)_

Scoped to the individual. Synced across all their devices.

- Coding preferences, stack choices, architecture patterns
- Library preferences (uber/zap, sqlc, pgx v5, chi router, etc.)
- Past projects, career history, CV content
- Communication style, debugging workflow, personal conventions

### Layer 2 — Workspace Memory _(lives with the project/team)_

Scoped to a project or repository.

- Cross-repo awareness ("auth service lives at /projects/auth-api")
- Project-specific context and architectural decisions
- Team conventions and shared patterns (paid tier: shared across teammates)

### Layer 3 — Org Memory _(paid tier only, not v1)_

- Company docs, onboarding, FAQs, runbooks
- Decision logs ("PIC confirmed project is on schedule on [date]")
- Access-controlled team knowledge base

---

## 3. Technical Architecture

### Stack

- **Language**: Rust
- **MCP SDK**: Official `modelcontextprotocol/rust-sdk` (crate: `rmcp`)
- **Database**: SQLite with FTS5 full-text search (`rusqlite`)
- **Dashboard**: Web UI served by the same binary (lightweight, SvelteKit or plain HTML)
- **Transport**: stdio (local Claude Code) and HTTP/SSE (self-hosted/team)

### Single Binary, Multiple Modes

```
hivemind                      # stdio mode — Claude Code spawns this automatically
hivemind up                   # HTTP mode + dashboard (self-hosted convenience)
hivemind up --headless        # HTTP mode only, no dashboard (headless server)
hivemind dashboard            # dashboard only, attaches to running server
hivemind dashboard --open     # same + opens browser
hivemind init                 # scaffold project config + CLAUDE.md
hivemind status               # show config, loaded memories, token budget preview
hivemind memory list
hivemind memory search <query>
hivemind memory export
hivemind memory import
hivemind db vacuum
hivemind db migrate
```

### Deployment Topology

```
Local only (default open source):
  Claude Code → spawns hivemind (stdio) → SQLite on local machine

Self-hosted sync (power user):
  All machines point to hivemind up running on Raspberry Pi / VPS
  Same binary, HTTP transport

Paid hosted (Oxhive cloud):
  Same binary on Oxhive infrastructure
  Multi-tenant, backups, uptime SLA
  Auth via API key
  Dashboard at app.oxhive.io
```

---

## 4. Data Model

### SQLite Schema

```sql
CREATE TABLE memories (
    id          TEXT PRIMARY KEY,         -- e.g. mem_a1b2c3
    layer       TEXT NOT NULL,            -- 'personal' | 'workspace'
    type        TEXT NOT NULL,            -- 'preference' | 'project' | 'history'
    title       TEXT NOT NULL,
    content     TEXT NOT NULL,
    source      TEXT,                     -- how it was created
    project     TEXT,                     -- workspace scoping (nullable)
    created_at  INTEGER NOT NULL,
    updated_at  INTEGER NOT NULL
);

CREATE TABLE tags (
    memory_id   TEXT REFERENCES memories(id) ON DELETE CASCADE,
    tag         TEXT NOT NULL
);

CREATE VIRTUAL TABLE memories_fts USING fts5(
    title, content,
    content='memories'
);

CREATE TABLE edges (
    id              TEXT PRIMARY KEY,
    source_id       TEXT NOT NULL REFERENCES memories(id),
    target_id       TEXT NOT NULL REFERENCES memories(id),
    relationship    TEXT NOT NULL,
    -- 'shares_tag'|'applies_to'|'pairs_with'|'used_in'|'related_to'|'custom'
    weight          REAL DEFAULT 1.0,
    inferred_by     TEXT NOT NULL,        -- 'auto' | 'ai' | 'manual'
    status          TEXT DEFAULT 'accepted',
    -- 'accepted' | 'pending' | 'rejected'
    confidence      REAL,                 -- internal only, never shown in UI
    reason          TEXT,
    created_at      INTEGER NOT NULL,
    updated_at      INTEGER NOT NULL,
    UNIQUE(source_id, target_id, relationship)
);

CREATE TABLE feedback (
    id          TEXT PRIMARY KEY,
    memory_id   TEXT REFERENCES memories(id),
    edge_id     TEXT REFERENCES edges(id),
    type        TEXT NOT NULL,
    -- 'incorrect'|'outdated'|'duplicate'|'wrong_connection'|'missing_connection'|'other'
    note        TEXT,
    status      TEXT DEFAULT 'open',      -- 'open' | 'resolved' | 'dismissed'
    created_at  INTEGER NOT NULL
);

CREATE TABLE conflicts (
    id           TEXT PRIMARY KEY,
    memory_id    TEXT REFERENCES memories(id),
    winner       TEXT NOT NULL,           -- JSON snapshot
    loser        TEXT NOT NULL,           -- JSON snapshot
    winner_src   TEXT NOT NULL,           -- device hostname
    loser_src    TEXT NOT NULL,
    detected_at  INTEGER NOT NULL,
    status       TEXT DEFAULT 'open'
    -- 'open'|'kept_current'|'restored'|'merged'|'dismissed'
);

CREATE TABLE pending_actions (
    id          TEXT PRIMARY KEY,
    action      TEXT NOT NULL,            -- 'edit' | 'flag' | etc.
    memory_id   TEXT,
    payload     TEXT,                     -- JSON
    created_at  INTEGER NOT NULL,
    status      TEXT DEFAULT 'pending'
);
```

### Retrieval Abstraction (design for v2 upgrade)

```rust
trait MemoryStore {
    fn store(&self, entry: MemoryEntry) -> Result<String>;
    fn recall(&self, query: &str, layer: Layer) -> Result<Vec<MemoryEntry>>;
    fn search(&self, query: &str) -> Result<Vec<MemoryEntry>>;
    fn delete(&self, id: &str) -> Result<()>;
}

// v1: SQLite + FTS5
struct SqliteStore { ... }

// v2: augment with embeddings (Ollama local or OpenAI-compatible API)
// SQLite remains source of truth, vector index is supplementary
// Enable via config: [embedding] enabled = true
```

---

## 5. MCP Tool Surface

### Tools (exposed to Claude Code)

#### `memory_store`

```json
{
  "name": "memory_store",
  "description": "Store a memory, preference, project context, or personal note for future recall across sessions and devices. Use when the user explicitly asks to remember something, or when important context should persist beyond this session.",
  "input_schema": {
    "type": "object",
    "properties": {
      "title": { "type": "string" },
      "content": { "type": "string" },
      "layer": { "type": "string", "enum": ["personal", "workspace"] },
      "tags": { "type": "array", "items": { "type": "string" } },
      "project": { "type": "string" }
    },
    "required": ["title", "content", "layer", "tags"]
  }
}
```

Response includes `auto_connected` count — number of edges auto-created via shared tags.

#### `memory_recall`

Direct lookup by title or id. Optional `include_connected: boolean` to return
neighborhood (use sparingly — token cost). Returns full content.

#### `memory_search`

FTS5 keyword search. Returns snippets (not full content) to preserve context budget.
Default limit 5, max 10. Use `memory_recall` with id for full content after finding
a candidate via search.

#### `memory_update`

Update title, content, or tags of existing memory. Supports `merge_content: boolean`
to append rather than replace (useful for evolving project context).

#### `memory_delete`

Requires `confirm: true` parameter. Forces Claude to explicitly confirm with user
before calling. Permanent deletion.

#### `memory_store_edge`

Store a confirmed connection between two memories. Only call after user explicitly
confirms. Used by `/suggest-connections` slash command flow.

### MCP Prompts (slash commands)

Exposed as MCP `prompts` primitive, not tools. Claude Code surfaces these as slash
commands. Server dynamically builds prompt content from SQLite data.

| Command                | Purpose                                                                                                                         |
| ---------------------- | ------------------------------------------------------------------------------------------------------------------------------- |
| `/suggest-connections` | Lazy AI inference — analyze memory graph, suggest edges. User accepts/rejects in dashboard. Stores as `status='pending'` edges. |
| `/memory-edit {id}`    | Fetch memory, present current content, ask user what to change, call memory_update                                              |
| `/memory-status`       | Show what memories are loaded this session, token budget used                                                                   |
| `/memory-list`         | List recent memories with titles and tags                                                                                       |
| `/memory-search`       | Interactive search flow                                                                                                         |
| `/review-feedback`     | Surface open feedback items, resolve interactively                                                                              |
| `/memory-flag {id}`    | Flag a memory for feedback                                                                                                      |

---

## 6. Config Architecture

### File Locations

```
~/.config/hivemind/config.toml          # global server + user defaults
{project_root}/.hivemind.toml           # committed — team/workspace hooks
{project_root}/.hivemind.local.toml     # gitignored — personal overrides
~/.local/share/hivemind/memory.db       # SQLite database
```

### Global Config `~/.config/hivemind/config.toml`

```toml
[server]
host = "127.0.0.1"
port = 3456
storage_path = "~/.local/share/hivemind/memory.db"

[sync]
enabled = false
remote_url = ""           # home Pi / VPS / app.oxhive.io
api_key = ""
interval_seconds = 300    # 5 minutes default
sync_on_store = true      # immediate sync after memory_store
sync_on_startup = true

[defaults]
max_inject_tokens = 2000  # hard ceiling for auto-injection per session
suggest_store = true      # Claude proactively suggests storing

[embedding]               # disabled in v1, ready for v2
enabled = false
provider = "ollama"       # "ollama" | "openai_compatible"
model = "nomic-embed-text"
endpoint = ""

[dashboard]
enabled = true
port = 3457
open_on_start = false
api_url = "http://localhost:3456"
```

### Project Config `{project_root}/.hivemind.toml` (committed)

```toml
[project]
name = "my-project"
layer = "workspace"
description = "Short project description"

[hooks.on_session_start]
max_tokens = 2000
recalls = [
  "golang preferences",
  "project/my-project",
]

[hooks.on_session_start.conditions]
paths = ["internal/", "cmd/", "pkg/"]

[hooks.on_file_open]
rules = [
  { pattern = "**/*_repository.go", recall = "sqlc pattern" },
  { pattern = "**/handler/*.go", recall = "golang http handler pattern" },
]

[hooks.on_mention]
triggers = [
  { keyword = "@arch", recall = "golang clean architecture" },
  { keyword = "@db", recall = "database patterns" },
]
```

### Personal Overrides `{project_root}/.hivemind.local.toml` (gitignored)

```toml
[hooks.on_session_start]
recalls = ["my debugging workflow"]
max_tokens = 500    # additive on top of team config
```

### Merge Rules

- `.hivemind.local.toml` is additive only — cannot suppress team-configured recalls
- Global config provides floor defaults
- Local override can only add recalls and extend token budget

### Config Discovery (walk up directory tree)

Same algorithm as Git `.git/` and ESLint `.eslintrc` — walk up from working
directory until `.hivemind.toml` is found. That directory is the project root.

---

## 7. Sync Mechanism

### How It Works

```
Local HiveMind                    Remote HiveMind server
    │── GET /api/sync/status ──▶│
    │◀─ { server_time, count } ──│
    │── POST /api/sync/push ───▶│  (records changed since last_synced_at)
    │◀─ { conflicts: [...] } ────│
    │── GET /api/sync/pull ────▶│  (?since=last_synced_at)
    │◀─ { updated records } ─────│
    │  apply pulled records      │
    │  write conflicts to table  │
    │  update last_synced_at     │
```

### Conflict Resolution

**Strategy: last-write-wins + conflict log**

- Newer `updated_at` timestamp wins automatically
- Losing version written to `conflicts` table, never discarded
- Dashboard surfaces conflicts with three actions:
  - **Keep current** — confirm winner, dismiss
  - **Restore overwritten** — replace with losing version
  - **Merge** — copy both versions to clipboard with `/memory-merge {id}` command
    for Claude to merge intelligently in Claude Code session

### Sync Status Display

```
● synced · just now
● synced · 3 min ago
⚠ synced · 6 min ago      (overdue warning)
✗ sync failed · retry      (clickable)
⚡ 1 conflict needs review  (in feedback view)
```

---

## 8. CLAUDE.md Integration

`hivemind init` writes to two locations:

### Global `~/.claude/CLAUDE.md` (appended, never overwritten)

```markdown
# HiveMind Memory System

You have access to HiveMind via MCP tools:
memory_store, memory_recall, memory_search, memory_update,
memory_delete, memory_store_edge

## Behavior Rules

### Auto-inject on session start

If .hivemind.toml exists in project root, read it and call configured
recalls before doing anything else. Do this silently.

### Suggest storing — never auto-store

When user shares something worth persisting (preferences, project
context, design decisions), suggest storing:
"That seems worth remembering — should I store this?"
Wait for explicit confirmation before calling memory_store.

### Recall before answering when relevant

If user asks about preferences, past projects, or known patterns,
call memory_search first. Don't ask permission — just search and
incorporate naturally.

### Respect token budget

Respect max_tokens in .hivemind.toml. Default 2000 tokens if no config.

### On memory not found

"I don't have a memory for that yet — want me to store something now?"
```

### Project `{project_root}/CLAUDE.md` (created by hivemind init)

```markdown
# HiveMind — {project_name}

Load project context on session start per .hivemind.toml.
Suggest storing any new architectural decisions made during session.
```

---

## 9. Dashboard UX

### Four Views

**Memories** (default landing — audit + manage)

- Three-panel layout: sidebar nav | filterable memory list | detail/edit panel
- List: search bar, layer filter chips (all/personal/workspace), tag filters
- Memory cards: title, 1-line snippet, tags, layer badge, date
- Detail panel: inline-editable title/content/tags, connections list
- Actions: Save, Delete (confirm required), Flag, Copy edit command

**Graph** (explore)

- Force-directed graph, tag-cluster grouping
- Three zoom levels:
  - L1: cluster boundaries only (golang, career, oxhive, frontend, etc.)
  - L2: individual nodes with labels
  - L3: nodes with tags, edge relationship labels on hover
- Sub-clusters emerge at L3 within each cluster (secondary tag grouping)
- Node size = connection count
- Node color: teal = personal, purple = workspace
- Edge styles: solid green = AI confirmed, dashed gray = auto-tag,
  dashed amber = pending suggestion
- Hover node: mini card + neighborhood highlight
- Click node: detail panel slides in from right (read-only)
- Hover pending edge: accept/reject card appears on edge midpoint
- Pending bar: bulk accept/reject when suggestions exist
- Connection mode: click Connect → crosshair cursor → click target node
  → relationship picker overlay → confirm → edge written live

**Feedback** (review flagged + conflicts)

- Two sections: Conflicts | Feedback items
- Conflict diff view: side-by-side current vs overwritten
- Actions: Keep current | Restore overwritten | Copy merge command

**Settings**

- Server status (read-only)
- Sync configuration
- Export / Import
- Danger zone (clear all memories)

### Interaction Patterns

- **Read-only in graph view** — detail panel shows content, no inline editing
- **Edit action** — copies `/memory-edit {id}` to clipboard with toast confirmation
- **Flag action** — copies `/memory-flag {id}` to clipboard
- **Suggest connections** — copies `/suggest-connections` to clipboard
  (or button in graph toolbar for discoverability)
- **Merge conflict** — copies `/memory-merge {id}` to clipboard
- Toast format: "Command copied: /memory-edit mem_a1b2c3" (2.2s duration)

---

## 10. Build Sequence — v1

### Phase 1: Core MCP Loop (weeks 1–2)

**Goal: `memory_store` and `memory_recall` working in a real Claude Code session**

1. Scaffold Rust project with `rmcp` SDK
2. Implement SQLite store with `rusqlite` — `memories` + `tags` tables
3. Implement `memory_store` tool — write to SQLite, return id + auto_connected count
4. Implement `memory_recall` tool — exact match by title or id
5. Wire MCP stdio transport
6. Test manually with Claude Code — store a preference, end session,
   new session, recall it. This is the core product validation.

### Phase 2: Search + Update (week 3–4)

1. Add FTS5 virtual table to schema
2. Implement `memory_search` — FTS5 keyword search, return snippets
3. Implement `memory_update` — partial update + merge_content flag
4. Implement `memory_delete` — with confirm:true guard
5. Add auto-tagging edge creation on store

### Phase 3: Hook System (week 5–6)

1. Config discovery — walk up directory tree for `.hivemind.toml`
2. Parse TOML config with `toml` crate
3. on_session_start hook — auto-recall on session init
4. Token budget enforcement — count tokens before injecting,
   respect max_inject_tokens ceiling
5. `hivemind init` command — scaffold configs + CLAUDE.md
6. `hivemind status` command

### Phase 4: REST API + Dashboard (week 7–9)

1. REST API layer — `/api/v1/memories`, `/api/v1/search`, `/api/v1/edges`
2. `hivemind up` — start HTTP MCP + REST API
3. `hivemind dashboard` — serve static dashboard files on :3457
4. Dashboard: Memories view (list + detail, edit)
5. Dashboard: Graph view (clusters, zoom, pending edges)
6. Dashboard: Feedback + Settings views

### Phase 5: Sync (week 10–11)

1. Sync protocol — push/pull endpoints on server
2. Client sync loop — interval timer + sync_on_store trigger
3. Conflict detection — compare updated_at on pull
4. Conflict log — write to conflicts table
5. Dashboard conflict UI

### Phase 6: Slash Commands (week 12)

1. MCP prompts primitive — `/memory-edit`, `/memory-status`, `/memory-list`
2. `/suggest-connections` — fetch memory graph, build dynamic prompt,
   store results as pending edges
3. `/review-feedback` — fetch open items, interactive resolution

---

## 11. Key Technical Constraints

**Context window budget**
Hard limit of 2000 tokens for auto-injected memories per session (configurable).
Search returns snippets not full content. `memory_recall` with `include_connected:true`
should be used sparingly.

**Single binary**
All functionality in one Rust binary. No separate processes, no Docker required.
User downloads one file. This is a hard requirement for self-host experience.

**Privacy by default**
Local mode: zero network calls, zero external dependencies.
Self-hosted: all data stays on user's infrastructure.
Paid: explicit opt-in via config.

**Config is additive**
`.hivemind.local.toml` can only add to team config, never suppress.
This enables team tier mandatory context without conflict.

**Vector DB migration path**
`MemoryStore` trait abstracts retrieval. SQLite FTS5 is v1 implementation.
Hybrid search (FTS5 + embeddings) is v2 — trigger when users have 200+ memories
and report semantic search misses. Use Ollama for local embeddings, OAI-compatible
API as alternative. SQLite remains source of truth; vectors are supplementary index.

---

## 12. Open Source vs Paid Boundary

| Feature                       | Open Source              | Paid (Oxhive Cloud) |
| ----------------------------- | ------------------------ | ------------------- |
| Local memory (single machine) | ✓                        | ✓                   |
| Self-hosted sync              | ✓ (user runs own server) | ✓                   |
| Managed sync across devices   | ✗                        | ✓                   |
| Personal memory (Layer 1)     | ✓                        | ✓                   |
| Workspace memory (Layer 2)    | ✓                        | ✓                   |
| Team shared workspace memory  | ✗                        | ✓                   |
| Org memory (Layer 3)          | ✗                        | ✓ (future)          |
| Dashboard                     | ✓                        | ✓                   |
| Conflict resolution           | ✓                        | ✓                   |
| Backups                       | User-managed             | Automated           |
| Access control / permissions  | ✗                        | ✓                   |
| Priority support              | ✗                        | ✓                   |

**Core value prop of paid tier**: Sync is the feature.
"Your memory, everywhere, zero setup."

---

## 13. Suggested First Session in Claude Code

When starting Claude Code on the HiveMind project, use this prompt:

```
Read HIVEMIND_SPEC.md in full before we start.

We are building HiveMind — a Rust MCP server for persistent developer memory.
Start with Phase 1 of the build sequence:

1. Scaffold a new Rust project
2. Add dependencies: rmcp (official MCP SDK), rusqlite, tokio, serde, toml
3. Implement the SQLite schema (memories + tags tables only for now)
4. Implement memory_store tool
5. Implement memory_recall tool
6. Wire stdio MCP transport
7. Test with a simple store + recall cycle

Ask me before making any architectural decisions not covered in the spec.
Flag any constraints in the Rust MCP SDK that affect our tool design.
```

---

_End of spec. Generated from product brainstorm session with Claude._
_All decisions in this document were explicitly validated by the product owner._
