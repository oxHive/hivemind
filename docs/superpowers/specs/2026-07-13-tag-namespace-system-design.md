---
title: Tag Namespace System — Colored Namespaces, Predefined Values, Project Grouping
date: 2026-07-13
status: approved
---

## Overview

Introduces a namespace convention for tags (`namespace:value`, e.g. `project:hivemind`, `lang:rust`) with per-namespace colors and predefined value lists, managed from a new Settings section. `project:*` is special: at most one per memory, and it drives a new Graph-page node label format. This directly enables using tags to group memories by project (a workspace-organization idea discussed earlier), generalized into a reusable namespace system rather than a one-off "project" field.

Tags remain plain free-text strings in storage — namespace is purely a client/server-parsed convention (split on the first `:`), not a schema change to the tag itself.

## Seeded Namespaces

Four namespaces are seeded on first use, each with a default color (editable afterward) and an empty predefined-value list (user fills in via Settings):

- `project` — single-value (see Multiplicity below)
- `lang` — multi-value
- `area` — multi-value (e.g. `area:dashboard`, `area:backend`, `area:cli`)
- `status` — multi-value (e.g. `status:idea`, `status:in-progress`, `status:done`)

Users can add further custom namespaces beyond these 4 from the same Settings UI. `memory_type` (the existing `preference`/`project`/`history` enum field) is unrelated to this system and untouched — hence avoiding `type` as a namespace name, to prevent confusion with that existing field.

## Multiplicity Rule

Only `project` is capped at one tag per memory. This is a hardcoded rule tied to the literal namespace name `project`, not a per-namespace toggle exposed in Settings. All other namespaces (seeded or custom) allow multiple values per memory (e.g. a memory can be tagged both `lang:rust` and `lang:vue`).

## Normalization & Validation (backend, centralized)

Both rules are enforced in `SqliteStore::store()` and `SqliteStore::update()` in `src/store.rs` — not duplicated across REST handlers, MCP tools, and import, so every entry point gets them automatically:

1. **Lowercase normalization**: every tag is lowercased before insertion (applies to the whole tag string, not just the namespace prefix — `Project:HiveMind` becomes `project:hivemind`).
2. **Single `project:*` tag**: if the resulting tag set for a memory would contain more than one tag matching `^project:`, the write is rejected with a validation error (same error-handling convention as the existing `Layer`/`MemoryType` parse validation already in these methods).

No backend validation exists for predefined *values* — value lists are autocomplete-only (soft), never enforced. Any free-text value is always accepted for any namespace.

## Storage

The namespace registry (`{name, color, values[]}[]`) persists server-side via the existing generic `_meta` key/value table (`Store::get_meta`/`set_meta`, already used for `last_synced_at`) under a new key, `tag_namespaces`, as a single JSON blob. No new SQL table or migration needed.

New REST endpoints, following the exact pattern of the existing `/api/v1/settings/sync`:
- `GET /api/v1/settings/tags` — returns the current registry (seeding defaults on first read if the meta key is absent).
- `POST /api/v1/settings/tags` — replaces the registry wholesale.

## Settings UI

New `TagsSection.vue`, added to `SettingsView.vue`'s existing stack (alongside `ServerSection`, `SyncSection`, `DataSection`, `DangerSection`) — not a new top-level nav page. Tag-namespace administration is occasional configuration, matching Settings' existing pattern; the app's top-level pages (Memories/Graph/Feedback/Settings) are reserved for frequently-browsed content.

Per namespace, the section shows:
- The namespace name.
- A row of ~8 curated preset color swatches (small clickable circles, palette consistent with the app's existing honeycomb color family — e.g. the same family as `GraphCanvas.vue`'s `personal`/`workspace`/`pending` colors) plus a small hex text input as an advanced escape hatch for a custom color.
- A live preview: the namespace rendered as an actual `TagChip` next to the picker, updating as you pick.
- An editable list of predefined values (add/remove), used for autocomplete.
- An "add namespace" affordance for custom namespaces beyond the seeded 4.

No two namespaces are prevented from sharing a color — that's the user's call, not validated.

## Tag Display

`TagChip.vue` parses the tag's namespace prefix (text before the first `:`) and, if it matches a known namespace in the registry, colors itself using that namespace's color. Tags with no `:` or an unrecognized namespace keep today's neutral gray styling — no behavior change for existing freeform tags (e.g. `architecture`, `dashboard` tags already in use).

## Tag Input & Autocomplete

`NewMemoryModal.vue` and `MemoryDetail.vue`'s tag-add flow gain namespace-aware autocomplete: typing a recognized namespace prefix (e.g. `lang:`) surfaces that namespace's predefined values as suggestions. This is soft/non-blocking — any typed value, predefined or not, is accepted, matching the "no backend value validation" decision above.

**Single-project UX**: adding a new `project:*` tag while the memory already has one **replaces** the existing one client-side (radio-button-style single-select), rather than erroring. This mirrors the backend's rejection rule without ever surfacing a validation error to the user in the normal UI flow — the reject path only fires for entry points that bypass this client logic (raw REST/MCP calls).

## Graph Label Format

In `GraphCanvas.vue`'s node label draw (inside `draw()`), a node whose tags include a `project:*` entry renders its label as `"<project-value>: <title>"` (e.g. `"hivemind: HiveMind product features"`) instead of just the title. Nodes without a `project:*` tag render exactly as today (title only). Uses the raw tag value, not a separately-configured display name — no additional Settings field for this.

## Explicitly Out of Scope

- Boolean AND search across tags (`tag:"lang:rust" & tag:"project:hivemind"`) — a related, previously-recorded idea (`mem_13cd73bf5bfe42ffb720732fea17b030`), but a separate feature not folded into this pass.
- Enforcing predefined values strictly (rejecting non-predefined values) — explicitly rejected in favor of soft autocomplete.
- Per-project-value display names (e.g. `hivemind` → `HiveMind`) for the graph label — explicitly rejected in favor of the raw tag value.
- Making the single-value rule configurable per namespace — hardcoded to `project` only.

## Testing

Backend: the normalization and single-project-tag validation are pure logic additions to already-tested `SqliteStore::store()`/`update()` — extend the existing unit test suite in `src/store.rs` (follow the existing test patterns there, e.g. `store_persists_row_and_tags`, `update_changes_title_and_recounts_tokens`) with cases for lowercase normalization and the multi-`project:*` rejection. REST integration tests in `tests/api_integration.rs` get a case for the new `/api/v1/settings/tags` endpoints, following the existing test style there.

Frontend: as established in the prior Graph Canvas Interactivity work, `dashboard/` has no test runner and none is being introduced here — verification is manual/visual (drive it in a browser if tooling is available in the implementation session; otherwise static review with that limitation explicitly disclosed, not claimed as tested).
