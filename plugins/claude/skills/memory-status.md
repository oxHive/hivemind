---
name: memory-status
description: Show HiveMind status: total memory count, what will be injected at session start for this project, and token budget usage.
---

Show the current HiveMind status for this session and project.

## Process

### 1. Check server

Call `hivemind_session_start` with the current working directory path to get the session start result. This returns what would be injected on a fresh session start.

### 2. Report

Show:
- Total memories in store (from the result or call `memory_search` with empty query for count)
- Project name (from .hivemind.toml if present)
- Memories that would be auto-injected at session start, with their token cost
- Token budget used vs total
- Whether the budget is exhausted (any entries were skipped)

### 3. Suggest next steps

If the budget is exhausted: "Some memories were skipped. Increase max_tokens in .hivemind.toml or remove low-value entries from recalls."

If no recalls are configured: "No recalls set up. Edit .hivemind.toml to add memory titles to auto-inject at session start."

## Rules

- Do not invent counts. Only report what the tool returns.
