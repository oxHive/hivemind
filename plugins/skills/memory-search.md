---
name: memory-search
description: Search HiveMind memories by keyword. Returns matching memories with snippets. Use when you need to find something stored previously.
---

Search HiveMind for memories matching the query in $ARGUMENTS.

## Process

### 1. Search

Call `memory_search` with the query from $ARGUMENTS.

### 2. Present results

For each result, show:
- Title and ID
- A short content snippet
- Tags

If no results are found, say so and suggest broadening the search term.

### 3. Offer next steps

After showing results, offer:
- "Recall the full content of [title]?" — call `memory_recall` with the ID
- "Edit [title]?" — use `/memory-edit`

## Rules

- Show at most 10 results; if more exist, note the count and suggest a narrower query.
- Never fabricate results. Only show what `memory_search` returned.
