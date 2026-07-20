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

### 2. Default the project tag

If `.hivemind.toml` exists in the project root, read it and check for a `[project]` table with a `name` key. If present, default the memory's `project:*` tag to `project:<name>` (lowercased). This is a default only — an explicit user instruction about which project to tag (a different name, or no project tag at all) always overrides it. Skip this step if `.hivemind.toml` doesn't exist or has no `project.name`.

### 3. Identify tags

Call `tag_namespaces_list` to see the project's registered namespaces (e.g. `project`, `topic`, `status`, `lang`), their allowed values, and their descriptions. Prefer `namespace:value` tags that already fit — reuse an existing value before adding a new one to a namespace, and reuse an existing namespace before reaching for a bare tag. Only use a bare (non-namespaced) tag for something that's genuinely a one-off; if you're about to reuse the same bare tag a second time, that's a sign it belongs in a namespace instead (mention this to the user rather than silently inventing a new namespace).

Check each namespace's `values_mode`: `"fixed"` means `values` is an enforced allow-list — `memory_store` rejects any tag in that namespace using a value not on the list, so pick from the listed values only. `"suggestion"` (or absent) means the list is just a hint — any value is accepted.

Extract 1-3 relevant tags this way from the content, in addition to the project tag from step 2.

### 4. Store

Call `memory_store` with:
- `title`: the extracted or provided title
- `content`: the full content
- `tags`: the tag list

### 5. Confirm

Report back: "Stored: [title] (ID: [id])". If the store fails, report the error.

## Rules

- Never auto-store without showing the user what will be saved.
- Keep titles short and specific enough to be recalled by keyword later.
- The `.hivemind.toml`-derived project tag is a default, not a mandate — always defer to what the user explicitly says about tagging.
