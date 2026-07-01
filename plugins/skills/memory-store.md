---
name: memory-store
description: Save something to HiveMind memory. Pass a title and content, or describe what to remember and Claude will extract and store it.
---

Save a new memory to HiveMind.

Input from $ARGUMENTS: a title and content to store, or a free-text description of what to remember.

## Process

### 1. Parse input

From $ARGUMENTS:
- If the input includes a clear title and body, use them directly.
- Otherwise, extract a short descriptive title (5-10 words) and the full content to store.

Ask the user to confirm the title before storing if it is ambiguous.

### 2. Identify tags

Extract 1-3 relevant tags from the content (e.g. "golang", "architecture", "preferences"). Use lowercase, single words or short phrases.

### 3. Store

Call `memory_store` with:
- `title`: the extracted or provided title
- `content`: the full content
- `tags`: the tag list

### 4. Confirm

Report back: "Stored: [title] (ID: [id])". If the store fails, report the error.

## Rules

- Never auto-store without showing the user what will be saved.
- Keep titles short and specific enough to be recalled by keyword later.
