---
name: memory-list
description: List all stored memories with titles, tags, and IDs. Use to browse what HiveMind knows before searching or editing.
---

List all memories stored in HiveMind.

## Process

### 1. Fetch

Call `memory_search` with an empty or wildcard query to retrieve all memories, or use the MCP prompt `memory-list` if available.

### 2. Present

Group memories by tag if there are more than 10. For each memory show:
- ID (short form)
- Title
- Tags

### 3. Offer next steps

After the list, offer:
- "Search for something specific?" — use `/memory-search`
- "Edit a memory?" — use `/memory-edit [id]`
- "Delete a memory?" — use `memory_delete` after confirmation

## Rules

- If the store is empty, say so and suggest using `/memory-store` to add the first entry.
