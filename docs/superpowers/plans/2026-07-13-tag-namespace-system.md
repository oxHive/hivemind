# Tag Namespace System Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a namespace convention for tags (`namespace:value`, e.g. `project:hivemind`, `lang:rust`) with per-namespace colors and predefined value lists managed from a new Settings section, plus a hardcoded single-value rule for `project:*` that also drives a new Graph-page node label format (`"<project>: <title>"`).

**Architecture:** Tags remain plain free-text strings in `memory_tags` — namespace is a client/server-parsed convention (split on the first `:`), not a schema change. The namespace registry (`{name: {color, values[]}}`) persists server-side via the existing generic `_meta` key/value table under a new key, `tag_namespaces`, exposed through new `GET/POST /api/v1/settings/tags` endpoints mirroring the existing `/api/v1/settings/sync` pattern. Two rules — lowercase normalization and "at most one `project:*` tag" — are centralized in `SqliteStore::store()`/`update()` so every entry point (REST, MCP, import) gets them for free, avoiding the kind of per-handler duplication that caused a bug earlier in this project's history (the `accepted` vs `active` edge-status mismatch). On the frontend, a new shared `TagInput.vue` component (chip list + namespace-aware autocomplete) replaces the ad-hoc tag-entry markup in both `NewMemoryModal.vue` and `MemoryDetail.vue`.

**Tech Stack:** Rust/axum/libsql backend (existing), Vue 3 + Pinia dashboard (existing), no new dependencies on either side.

## Global Constraints

- Spec: `docs/superpowers/specs/2026-07-13-tag-namespace-system-design.md` — every requirement below traces to it.
- Seeded namespaces at launch: `project`, `lang`, `area`, `status`. Avoid `type` as a namespace name — it collides with the existing `memory_type` field (`preference`/`project`/`history`), which is unrelated to this feature.
- Only `project` is capped at one tag per memory (hardcoded to that literal namespace name, not a configurable per-namespace toggle). All other namespaces (seeded or custom) allow multiple values per memory.
- Every tag is lowercased on write (the whole string, not just the namespace prefix), enforced in `SqliteStore::store()`/`update()`, not per-handler.
- No backend validation of predefined *values* — autocomplete is soft/suggestion-only; any free-text value is always accepted for any namespace.
- Namespace registry storage: `_meta` table, key `tag_namespaces`, JSON blob shaped `{"<namespace>": {"color": "#rrggbb", "values": ["v1", "v2"]}}`.
- New REST routes: `GET/POST /api/v1/settings/tags`, added to `src/api.rs`'s existing `router()` alongside `/api/v1/settings/sync`.
- Settings UI lives in a new section (`TagsSection.vue`) inside the existing `SettingsView.vue` stack — not a new top-level nav page.
- Graph label: a node with a `project:*` tag renders `"<project-value>: <title>"`; nodes without one render as today (title only). Uses the raw tag value, no separate display-name field.
- `dashboard/` has no test runner and none is introduced here — frontend verification is manual/static review, explicitly disclosed as such, not claimed as tested. Backend changes DO have `cargo test` — use it for TDD as normal.
- Out of scope (do not implement): boolean AND tag search, strict predefined-value enforcement, per-project-value display names, making the single-value rule configurable per namespace.

---

### Task 1: Backend — tag normalization and single-`project:*` validation

**Files:**
- Modify: `src/store.rs`

**Interfaces:**
- Produces: `SqliteStore::store()` and `SqliteStore::update()` now (a) lowercase every tag before persisting and (b) return an `Err` if the resulting tag set for a memory would contain more than one tag matching `^project:` (case-insensitive). No new public function signatures — this is internal behavior of the two existing methods. Later tasks do not depend on any new symbol from this task.

- [ ] **Step 1: Write failing tests for lowercase normalization and single-project-tag rejection**

In `src/store.rs`, find the test module's existing tag-related tests (near `store_persists_row_and_tags`, around line 747) and add these three tests immediately after `store_deduplicates_tags` (find that test, add after its closing `}`):

```rust
    #[tokio::test]
    async fn store_lowercases_tags() {
        let (s, _dir) = make_store().await;
        s.store(&test_row(
            "mem_upper",
            "Title",
            "content",
            &["Lang:Rust".into(), "PROJECT:HiveMind".into()],
        ))
        .await
        .unwrap();
        let entry = s.recall_by_id("mem_upper").await.unwrap().unwrap();
        assert!(entry.tags.contains(&"lang:rust".to_string()));
        assert!(entry.tags.contains(&"project:hivemind".to_string()));
    }

    #[tokio::test]
    async fn store_rejects_more_than_one_project_tag() {
        let (s, _dir) = make_store().await;
        let result = s
            .store(&test_row(
                "mem_multi_project",
                "Title",
                "content",
                &["project:hivemind".into(), "project:oxhive".into()],
            ))
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn update_rejects_more_than_one_project_tag() {
        let (s, _dir) = make_store().await;
        let tags: Vec<String> = vec![];
        s.store(&test_row("mem_up", "Title", "content", &tags))
            .await
            .unwrap();
        let result = s
            .update(
                "mem_up",
                "Title",
                "content",
                &["project:a".into(), "project:b".into()],
            )
            .await;
        assert!(result.is_err());
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --lib store_lowercases_tags store_rejects_more_than_one_project_tag update_rejects_more_than_one_project_tag`
Expected: `store_lowercases_tags` FAILS (tags are stored as-typed, not lowercased); the two rejection tests FAIL (`result.is_err()` is false — `store()`/`update()` currently accept any number of `project:*` tags).

- [ ] **Step 3: Implement the validation helper and wire it into `store()`/`update()`**

In `src/store.rs`, find the `fts_quote` free function (around line 82, just above `impl SqliteStore`). Add a new free function right after it, before `impl SqliteStore {`:

```rust
/// A memory may have at most one tag in the `project` namespace — this is
/// the only namespace with this restriction (see the tag-namespace-system
/// design spec); all others allow multiple values per memory.
fn validate_single_project_tag(tags: &[String]) -> Result<()> {
    let project_tag_count = tags
        .iter()
        .filter(|t| t.to_lowercase().starts_with("project:"))
        .count();
    if project_tag_count > 1 {
        return Err(anyhow!("a memory can have at most one project:* tag"));
    }
    Ok(())
}
```

Now find `store()` (around line 95):

```rust
    pub async fn store(&self, m: &NewMemoryRow<'_>) -> Result<()> {
        let now = chrono_now();
        let token_count = m
            .token_count
            .unwrap_or_else(|| crate::budget::count_entry_tokens(m.title, m.content) as i64);

        let tx = self.conn.transaction().await?;
```

Change it to validate before opening the transaction:

```rust
    pub async fn store(&self, m: &NewMemoryRow<'_>) -> Result<()> {
        validate_single_project_tag(m.tags)?;
        let now = chrono_now();
        let token_count = m
            .token_count
            .unwrap_or_else(|| crate::budget::count_entry_tokens(m.title, m.content) as i64);

        let tx = self.conn.transaction().await?;
```

Then find the tag-insert loop inside `store()`:

```rust
        for tag in m.tags {
            tx.execute(
                "INSERT OR IGNORE INTO memory_tags (memory_id, tag) VALUES (?1, ?2)",
                params![m.id, tag.as_str()],
            )
            .await?;
        }

        // Auto-connect memories sharing a tag: one statement per tag, skipping
        // pairs already linked in either direction.
        for tag in m.tags {
            tx.execute(
                "INSERT INTO edges (id, source_id, target_id, relationship, status, created_at)
                 SELECT 'edge_' || lower(hex(randomblob(16))), ?1, mt.memory_id, 'shares_tag', 'active', ?2
                 FROM memory_tags mt
                 WHERE mt.tag = ?3 AND mt.memory_id != ?1
                   AND NOT EXISTS (
                       SELECT 1 FROM edges e
                       WHERE e.relationship = 'shares_tag'
                         AND ((e.source_id = ?1 AND e.target_id = mt.memory_id)
                           OR (e.source_id = mt.memory_id AND e.target_id = ?1)))",
                params![m.id, now, tag.as_str()],
            )
            .await?;
        }
```

Change both loops to lowercase each tag before use (the auto-connect query must use the same lowercased form, since that's what's now persisted):

```rust
        for tag in m.tags {
            let tag_lower = tag.to_lowercase();
            tx.execute(
                "INSERT OR IGNORE INTO memory_tags (memory_id, tag) VALUES (?1, ?2)",
                params![m.id, tag_lower.as_str()],
            )
            .await?;
        }

        // Auto-connect memories sharing a tag: one statement per tag, skipping
        // pairs already linked in either direction.
        for tag in m.tags {
            let tag_lower = tag.to_lowercase();
            tx.execute(
                "INSERT INTO edges (id, source_id, target_id, relationship, status, created_at)
                 SELECT 'edge_' || lower(hex(randomblob(16))), ?1, mt.memory_id, 'shares_tag', 'active', ?2
                 FROM memory_tags mt
                 WHERE mt.tag = ?3 AND mt.memory_id != ?1
                   AND NOT EXISTS (
                       SELECT 1 FROM edges e
                       WHERE e.relationship = 'shares_tag'
                         AND ((e.source_id = ?1 AND e.target_id = mt.memory_id)
                           OR (e.source_id = mt.memory_id AND e.target_id = ?1)))",
                params![m.id, now, tag_lower.as_str()],
            )
            .await?;
        }
```

Now find `update()` (around line 221):

```rust
    pub async fn update(
        &self,
        id: &str,
        title: &str,
        content: &str,
        tags: &[String],
    ) -> Result<bool> {
        let now = chrono_now();
        let token_count = crate::budget::count_entry_tokens(title, content) as i64;
        let tx = self.conn.transaction().await?;
```

Change it to validate first:

```rust
    pub async fn update(
        &self,
        id: &str,
        title: &str,
        content: &str,
        tags: &[String],
    ) -> Result<bool> {
        validate_single_project_tag(tags)?;
        let now = chrono_now();
        let token_count = crate::budget::count_entry_tokens(title, content) as i64;
        let tx = self.conn.transaction().await?;
```

Then find `update()`'s tag-insert loop:

```rust
        for tag in tags {
            tx.execute(
                "INSERT OR IGNORE INTO memory_tags (memory_id, tag) VALUES (?1, ?2)",
                params![id, tag.as_str()],
            )
            .await?;
        }
```

Change it to lowercase:

```rust
        for tag in tags {
            let tag_lower = tag.to_lowercase();
            tx.execute(
                "INSERT OR IGNORE INTO memory_tags (memory_id, tag) VALUES (?1, ?2)",
                params![id, tag_lower.as_str()],
            )
            .await?;
        }
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --lib store_lowercases_tags store_rejects_more_than_one_project_tag update_rejects_more_than_one_project_tag`
Expected: all 3 PASS.

- [ ] **Step 5: Run the full test suite to check for regressions**

Run: `cargo test --lib`
Expected: all tests pass (178 previously, now 181).

- [ ] **Step 6: Commit**

```bash
git add src/store.rs
git commit -m "feat: lowercase tags on write, reject more than one project:* tag per memory"
```

---

### Task 2: Backend — tag namespace settings REST endpoints

**Files:**
- Modify: `src/api.rs`

**Interfaces:**
- Consumes: `Store::get_meta`/`set_meta` (existing, `src/store.rs:581-601`).
- Produces: `GET /api/v1/settings/tags` (returns the registry, seeding the 4 default namespaces if none is stored yet) and `POST /api/v1/settings/tags` (replaces it wholesale) — routes added to the `router()` function. Later frontend tasks (3+) consume this exact JSON shape: `{"<namespace>": {"color": "#rrggbb", "values": ["v1", ...]}}`.

- [ ] **Step 1: Write failing tests for the new endpoints**

In `src/api.rs`, find the existing `save_sync_settings_returns_not_saved` test (around line 783) and add these two tests immediately after it:

```rust
    #[tokio::test]
    async fn get_tag_settings_returns_seeded_defaults_when_unset() {
        let (app, _dir) = test_router().await;
        let (status, body) = req(app, "GET", "/api/v1/settings/tags", None).await;
        assert_eq!(status, StatusCode::OK);
        assert!(body["project"]["color"].is_string());
        assert!(body["lang"]["color"].is_string());
        assert!(body["area"]["color"].is_string());
        assert!(body["status"]["color"].is_string());
        assert_eq!(body["project"]["values"], json!([]));
    }

    #[tokio::test]
    async fn save_tag_settings_persists_and_get_returns_it() {
        let (app, _dir) = test_router().await;
        let custom = json!({
            "project": { "color": "#4a9eff", "values": ["hivemind", "oxhive"] },
            "lang": { "color": "#e0607e", "values": ["rust"] },
        });
        let (status, saved) = req(
            app.clone(),
            "POST",
            "/api/v1/settings/tags",
            Some(custom.clone()),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(saved["saved"], true);

        let (status, body) = req(app, "GET", "/api/v1/settings/tags", None).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body, custom);
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test get_tag_settings_returns_seeded_defaults_when_unset save_tag_settings_persists_and_get_returns_it`
Expected: FAIL with a 404 (no such route exists yet).

- [ ] **Step 3: Add the default-registry helper and the two handlers**

In `src/api.rs`, find `save_sync_settings`:

```rust
async fn save_sync_settings(Json(_): Json<Value>) -> Json<Value> {
    Json(
        json!({ "saved": false, "message": "Sync settings are managed via config.toml — restart hivemind after editing." }),
    )
}
```

Add the following immediately after it:

```rust
fn default_tag_namespaces() -> Value {
    json!({
        "project": { "color": "#4a9eff", "values": [] },
        "lang": { "color": "#e0607e", "values": [] },
        "area": { "color": "#5fb8b0", "values": [] },
        "status": { "color": "#a875d1", "values": [] },
    })
}

async fn get_tag_settings(State(store): State<Store>) -> Result<Json<Value>, ApiError> {
    let raw = store.get_meta("tag_namespaces").await?;
    let registry = match raw {
        Some(s) => serde_json::from_str(&s).unwrap_or_else(|_| default_tag_namespaces()),
        None => default_tag_namespaces(),
    };
    Ok(Json(registry))
}

async fn save_tag_settings(
    State(store): State<Store>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    store.set_meta("tag_namespaces", &body.to_string()).await?;
    Ok(Json(json!({ "saved": true })))
}
```

- [ ] **Step 4: Register the route**

Find, in `router()`:

```rust
        .route(
            "/api/v1/settings/sync",
            get(get_sync_settings).post(save_sync_settings),
        )
        .route("/api/v1/status", get(server_status))
```

Change it to add the new route right after:

```rust
        .route(
            "/api/v1/settings/sync",
            get(get_sync_settings).post(save_sync_settings),
        )
        .route(
            "/api/v1/settings/tags",
            get(get_tag_settings).post(save_tag_settings),
        )
        .route("/api/v1/status", get(server_status))
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test get_tag_settings_returns_seeded_defaults_when_unset save_tag_settings_persists_and_get_returns_it`
Expected: both PASS.

- [ ] **Step 6: Run the full test suite and the integration suite**

Run: `cargo test --lib && cargo test --test api_integration`
Expected: all pass (181 lib tests from Task 1, plus these 2 new ones = 183; 14 integration tests unaffected).

- [ ] **Step 7: Commit**

```bash
git add src/api.rs
git commit -m "feat: add GET/POST /api/v1/settings/tags for the tag namespace registry"
```

---

### Task 3: Frontend — tag settings store, API client, and namespace-colored `TagChip`

**Files:**
- Modify: `dashboard/src/api/settings.js`
- Create: `dashboard/src/stores/tagSettings.js`
- Modify: `dashboard/src/App.vue`
- Modify: `dashboard/src/components/shared/TagChip.vue`

**Interfaces:**
- Consumes: `GET/POST /api/v1/settings/tags` (Task 2).
- Produces: `useTagSettingsStore()` — Pinia store with `namespaces` (ref, object keyed by namespace name → `{color, values}`), `loaded` (ref bool), `fetchNamespaces()` (async), `save()` (async), `namespaceFor(tag)` (returns the namespace name if the tag's prefix is a known namespace, else `null`), `colorFor(tag)` (returns the namespace's color string if known, else `null`). Tasks 4 and 6 both consume this store.

- [ ] **Step 1: Add API client functions**

In `dashboard/src/api/settings.js`, the file currently reads:

```js
import { request } from './client.js'

export const getSyncSettings = () => request('GET', '/api/v1/settings/sync')
export const saveSyncSettings = (body) => request('POST', '/api/v1/settings/sync', body)
```

Change it to:

```js
import { request } from './client.js'

export const getSyncSettings = () => request('GET', '/api/v1/settings/sync')
export const saveSyncSettings = (body) => request('POST', '/api/v1/settings/sync', body)
export const getTagSettings = () => request('GET', '/api/v1/settings/tags')
export const saveTagSettings = (body) => request('POST', '/api/v1/settings/tags', body)
```

- [ ] **Step 2: Create the Pinia store**

Create `dashboard/src/stores/tagSettings.js`:

```js
import { defineStore } from 'pinia'
import { ref } from 'vue'
import { getTagSettings, saveTagSettings } from '../api/settings.js'

export const useTagSettingsStore = defineStore('tagSettings', () => {
  const namespaces = ref({})
  const loaded = ref(false)

  async function fetchNamespaces() {
    namespaces.value = await getTagSettings()
    loaded.value = true
  }

  async function save() {
    await saveTagSettings(namespaces.value)
  }

  function namespaceFor(tag) {
    const idx = tag.indexOf(':')
    if (idx === -1) return null
    const ns = tag.slice(0, idx)
    return namespaces.value[ns] ? ns : null
  }

  function colorFor(tag) {
    const ns = namespaceFor(tag)
    return ns ? namespaces.value[ns].color : null
  }

  return { namespaces, loaded, fetchNamespaces, save, namespaceFor, colorFor }
})
```

- [ ] **Step 3: Fetch namespaces on app mount**

In `dashboard/src/App.vue`, find:

```js
import { useUiStore } from './stores/ui.js'
import { useMemoriesStore } from './stores/memories.js'
import { useGraphStore } from './stores/graph.js'
import { useFeedbackStore } from './stores/feedback.js'
import { BASE } from './api/client.js'
```

Change it to:

```js
import { useUiStore } from './stores/ui.js'
import { useMemoriesStore } from './stores/memories.js'
import { useGraphStore } from './stores/graph.js'
import { useFeedbackStore } from './stores/feedback.js'
import { useTagSettingsStore } from './stores/tagSettings.js'
import { BASE } from './api/client.js'
```

Find:

```js
const ui = useUiStore()
const memories = useMemoriesStore()
const graph = useGraphStore()
const fb = useFeedbackStore()
```

Change it to:

```js
const ui = useUiStore()
const memories = useMemoriesStore()
const graph = useGraphStore()
const fb = useFeedbackStore()
const tagSettings = useTagSettingsStore()
```

Find, inside `onMounted`:

```js
  if (ui.serverStatus !== 'unreachable') {
    await Promise.all([
      memories.fetchAll(),
      graph.fetchEdges(),
      fb.fetchConflicts(),
      fb.fetchFeedback(),
    ])
  }
```

Change it to:

```js
  if (ui.serverStatus !== 'unreachable') {
    await Promise.all([
      memories.fetchAll(),
      graph.fetchEdges(),
      fb.fetchConflicts(),
      fb.fetchFeedback(),
      tagSettings.fetchNamespaces(),
    ])
  }
```

- [ ] **Step 4: Color `TagChip` by namespace**

Replace the full contents of `dashboard/src/components/shared/TagChip.vue` (currently 13 lines) with:

```vue
<script setup>
import { computed } from 'vue'
import { useTagSettingsStore } from '../../stores/tagSettings.js'

const props = defineProps({ tag: String, removable: Boolean })
defineEmits(['remove'])

const tagSettings = useTagSettingsStore()
const color = computed(() => tagSettings.colorFor(props.tag))
</script>

<template>
  <span class="inline-flex items-center gap-1 rounded-sm px-1.5 py-0.5 text-[10px] font-mono"
    :style="color
      ? `background:${color}22; color:${color}; border:0.5px solid ${color}55`
      : 'background:var(--hm-bg-elevated); color:var(--hm-text-tertiary); border:0.5px solid var(--hm-border-subtle)'">
    {{ tag }}
    <button v-if="removable" @click.stop="$emit('remove')"
      class="leading-none hover:text-white"
      :style="color ? `color:${color}` : 'color:var(--hm-text-tertiary)'">×</button>
  </span>
</template>
```

- [ ] **Step 5: Manually verify**

Since `dashboard/` has no test runner, verify by running the dev server:

```bash
cd dashboard && bun install && bun run dev
```

With `hivemind up` running in another terminal:
1. Open the dashboard, open devtools console, run `fetch('/api/v1/settings/tags').then(r=>r.json()).then(console.log)` (or check the Network tab on page load) — confirm it returns the 4 seeded namespaces with colors and empty `values` arrays.
2. Open a memory that has any existing tag (e.g. one with a plain freeform tag like `architecture`) — confirm its `TagChip` still renders in the original neutral gray (unrecognized namespace, no regression).
3. In devtools console, temporarily run `useTagSettingsStore()` is not directly accessible from console, so instead: create a memory via `memory_store` MCP tool (or the dashboard's "+ New memory" if still using the old comma-separated input from before Task 5) with a tag like `lang:rust`, then view it in the Memories list — confirm the `lang:rust` chip now renders in the `lang` namespace's seeded color (`#e0607e`) instead of gray.

- [ ] **Step 6: Commit**

```bash
git add dashboard/src/api/settings.js dashboard/src/stores/tagSettings.js dashboard/src/App.vue dashboard/src/components/shared/TagChip.vue
git commit -m "feat(dashboard): fetch tag namespace registry, color TagChip by namespace"
```

---

### Task 4: Frontend — shared `TagInput` component (chip list + namespace-aware autocomplete)

**Files:**
- Create: `dashboard/src/components/shared/TagInput.vue`

**Interfaces:**
- Consumes: `useTagSettingsStore()` (Task 3) for namespace names/values; `TagChip.vue` (Task 3) for rendering.
- Produces: `TagInput` component with props `modelValue: Array` (current tags) and emits `update:modelValue` (new tags array) — a `v-model`-compatible component. Task 5 wires this into `NewMemoryModal.vue` and `MemoryDetail.vue`.

- [ ] **Step 1: Create the component**

Create `dashboard/src/components/shared/TagInput.vue`:

```vue
<script setup>
import { ref, computed } from 'vue'
import { useTagSettingsStore } from '../../stores/tagSettings.js'
import TagChip from './TagChip.vue'

const props = defineProps({ modelValue: { type: Array, default: () => [] } })
const emit = defineEmits(['update:modelValue'])

const tagSettings = useTagSettingsStore()
const inputValue = ref('')
const showSuggestions = ref(false)

const suggestions = computed(() => {
  const raw = inputValue.value.trim()
  if (!raw) return []
  const colonIdx = raw.indexOf(':')
  if (colonIdx === -1) {
    return Object.keys(tagSettings.namespaces)
      .filter(ns => ns.startsWith(raw.toLowerCase()))
      .map(ns => `${ns}:`)
  }
  const ns = raw.slice(0, colonIdx).toLowerCase()
  const partial = raw.slice(colonIdx + 1).toLowerCase()
  const entry = tagSettings.namespaces[ns]
  if (!entry) return []
  return entry.values
    .filter(v => v.startsWith(partial))
    .map(v => `${ns}:${v}`)
})

function commit(rawTag) {
  const tag = rawTag.trim().toLowerCase()
  if (!tag) return
  const isProjectTag = tag.startsWith('project:')
  let next = props.modelValue.filter(t => !(isProjectTag && t.toLowerCase().startsWith('project:')))
  if (!next.includes(tag)) next = [...next, tag]
  emit('update:modelValue', next)
  inputValue.value = ''
  showSuggestions.value = false
}

function selectSuggestion(s) {
  commit(s)
}

function removeTag(tag) {
  emit('update:modelValue', props.modelValue.filter(t => t !== tag))
}
</script>

<template>
  <div class="relative">
    <div class="flex flex-wrap gap-1.5 p-2.5 rounded-md" style="border:0.5px solid var(--hm-border-subtle); min-height:40px">
      <TagChip v-for="tag in modelValue" :key="tag" :tag="tag" removable @remove="removeTag(tag)" />
      <input class="hm-input" style="width:120px; height:22px; font-size:10px; padding:0 6px"
        v-model="inputValue"
        placeholder="add tag…"
        @focus="showSuggestions = true"
        @keydown.enter.prevent="commit(inputValue)"
        @keydown.esc="showSuggestions = false"
        @blur="setTimeout(() => showSuggestions = false, 150)" />
    </div>
    <div v-if="showSuggestions && suggestions.length"
      class="absolute left-0 mt-1 rounded-md py-1"
      style="background:var(--hm-bg-overlay); border:0.5px solid var(--hm-border-default); z-index:10; min-width:140px">
      <button v-for="s in suggestions" :key="s"
        class="block w-full text-left px-3 py-1.5 font-mono"
        style="font-size:11px; color:var(--hm-text-secondary)"
        @mousedown.prevent="selectSuggestion(s)">{{ s }}</button>
    </div>
  </div>
</template>
```

- [ ] **Step 2: Manually verify in isolation**

Since `dashboard/` has no test runner, this component has no consumer yet (Task 5 wires it in) — defer interactive verification to Task 5's manual check, which exercises this component directly. For this step, just confirm the file has no syntax errors by running the dev build:

```bash
cd dashboard && bun run build
```

Expected: builds successfully (this component isn't imported anywhere yet, so a build success here just confirms valid Vue SFC syntax, not runtime behavior).

- [ ] **Step 3: Commit**

```bash
git add dashboard/src/components/shared/TagInput.vue
git commit -m "feat(dashboard): add shared TagInput component with namespace-aware autocomplete"
```

---

### Task 5: Frontend — wire `TagInput` into `NewMemoryModal.vue` and `MemoryDetail.vue`

**Files:**
- Modify: `dashboard/src/components/memories/NewMemoryModal.vue`
- Modify: `dashboard/src/components/memories/MemoryDetail.vue`

**Interfaces:**
- Consumes: `TagInput.vue` (Task 4), which is `v-model`-compatible (prop `modelValue: Array`, emits `update:modelValue`).

- [ ] **Step 1: Replace `NewMemoryModal.vue`'s comma-separated tags field**

Replace the full contents of `dashboard/src/components/memories/NewMemoryModal.vue` with:

```vue
<script setup>
import { ref } from 'vue'
import { useMemoriesStore } from '../../stores/memories.js'
import { useUiStore } from '../../stores/ui.js'
import TagInput from '../shared/TagInput.vue'

const emit = defineEmits(['close'])
const memories = useMemoriesStore()
const ui = useUiStore()

const title = ref('')
const content = ref('')
const tags = ref([])
const layer = ref('workspace')
const saving = ref(false)

async function submit() {
  if (!title.value.trim() || !content.value.trim()) return
  saving.value = true
  try {
    await memories.create({
      title: title.value.trim(),
      content: content.value,
      tags: tags.value,
      layer: layer.value,
    })
    ui.showToast('Memory created')
    emit('close')
  } finally {
    saving.value = false
  }
}
</script>

<template>
  <div class="fixed inset-0 flex items-center justify-center" style="background:#000a; z-index:50"
    @click.self="emit('close')" @keydown.esc="emit('close')">
    <div class="rounded-lg p-6" role="dialog" aria-label="New memory"
      style="width:460px; background:var(--hm-bg-elevated); border:0.5px solid var(--hm-border-default)">
      <p class="mb-5" style="font-size:14px; font-weight:500; color:var(--hm-text-primary)">New memory</p>
      <label class="hm-label" for="nm-title">TITLE</label>
      <input id="nm-title" class="hm-input mb-4" v-model="title" autofocus />
      <label class="hm-label" for="nm-content">CONTENT</label>
      <textarea id="nm-content" class="hm-input mb-4 resize-none"
        style="height:120px; padding:10px 12px; font-family:var(--hm-font-mono); font-size:12px"
        v-model="content"></textarea>
      <label class="hm-label">TAGS</label>
      <div class="mb-4">
        <TagInput v-model="tags" />
      </div>
      <label class="hm-label">LAYER</label>
      <div class="flex gap-1.5 mb-6">
        <button class="hm-btn hm-btn-sm"
          :style="layer==='workspace' ? 'background:var(--hm-workspace-bg); border-color:var(--hm-workspace); color:var(--hm-workspace)' : 'border-color:var(--hm-border-subtle); color:var(--hm-text-secondary)'"
          @click="layer='workspace'">workspace</button>
        <button class="hm-btn hm-btn-sm"
          :style="layer==='personal' ? 'background:var(--hm-personal-bg); border-color:var(--hm-personal); color:var(--hm-personal)' : 'border-color:var(--hm-border-subtle); color:var(--hm-text-secondary)'"
          @click="layer='personal'">personal</button>
      </div>
      <div class="flex justify-end gap-2">
        <button class="hm-btn hm-btn-default hm-btn-sm" @click="emit('close')">Cancel</button>
        <button class="hm-btn hm-btn-primary hm-btn-sm"
          :disabled="saving || !title.trim() || !content.trim()" @click="submit">
          {{ saving ? 'Creating…' : 'Create' }}
        </button>
      </div>
    </div>
  </div>
</template>
```

- [ ] **Step 2: Replace `MemoryDetail.vue`'s ad-hoc tag entry**

In `dashboard/src/components/memories/MemoryDetail.vue`, find the imports:

```js
import LayerBadge from '../shared/LayerBadge.vue'
import TagChip from '../shared/TagChip.vue'
import EmptyState from '../shared/EmptyState.vue'
```

Change to:

```js
import LayerBadge from '../shared/LayerBadge.vue'
import TagInput from '../shared/TagInput.vue'
import EmptyState from '../shared/EmptyState.vue'
```

Find:

```js
const showDeleteModal = ref(false)
const newTagInput = ref('')
const addingTag = ref(false)
const flagOpen = ref(false)
```

Change to (drop the now-unused `newTagInput`/`addingTag`):

```js
const showDeleteModal = ref(false)
const flagOpen = ref(false)
```

Find:

```js
function removeTag(tag) {
  memories.draft.tags = memories.draft.tags.filter(t => t !== tag)
}

function addTag() {
  const t = newTagInput.value.trim()
  if (t && !memories.draft.tags.includes(t)) memories.draft.tags.push(t)
  newTagInput.value = ''
  addingTag.value = false
}
```

Delete both functions entirely (no replacement needed — `TagInput` handles this internally via `v-model`).

Find the tags block in the template:

```html
        <!-- Tags -->
        <label class="hm-label" id="mem-tags-label">TAGS</label>
        <div class="flex flex-wrap gap-1.5 p-2.5 mb-6 rounded-md"
          aria-labelledby="mem-tags-label"
          style="border:0.5px solid var(--hm-border-subtle); min-height:40px">
          <TagChip
            v-for="tag in memories.draft?.tags" :key="tag"
            :tag="tag" :removable="true"
            @remove="removeTag(tag)" />
          <template v-if="addingTag">
            <input class="hm-input" style="width:100px; height:22px; font-size:10px; padding:0 6px"
              v-model="newTagInput" @keydown.enter="addTag" @keydown.esc="addingTag = false" @blur="addTag" />
          </template>
          <button v-else class="font-mono" style="font-size:10px; color:var(--hm-text-tertiary)"
            @click="addingTag = true">+ add tag</button>
        </div>
```

Replace it with:

```html
        <!-- Tags -->
        <label class="hm-label" id="mem-tags-label">TAGS</label>
        <div class="mb-6" aria-labelledby="mem-tags-label">
          <TagInput
            :model-value="memories.draft?.tags ?? []"
            @update:model-value="memories.draft.tags = $event" />
        </div>
```

- [ ] **Step 3: Manually verify**

```bash
cd dashboard && bun install && bun run dev
```

With `hivemind up` running:
1. Open "+ New memory" — confirm the TAGS field now shows a chip-input box instead of a comma-separated text field. Type `lang:` — confirm a suggestion dropdown appears (even if empty until Task 6 seeds values). Type `lang:rust` and press Enter — confirm a `lang:rust` chip appears, colored per Task 3.
2. In the same new-memory form, add `project:foo`, then add `project:bar` — confirm `project:foo` is automatically replaced by `project:bar` (only one project chip remains), not both.
3. Create the memory, then open it in `MemoryDetail` — confirm the same `TagInput` behavior works there too (add/remove tags, single-project replacement), and that existing tags from before this change still display correctly as chips.

- [ ] **Step 4: Commit**

```bash
git add dashboard/src/components/memories/NewMemoryModal.vue dashboard/src/components/memories/MemoryDetail.vue
git commit -m "feat(dashboard): use TagInput for tag entry in NewMemoryModal and MemoryDetail"
```

---

### Task 6: Frontend — `TagsSection.vue` Settings UI

**Files:**
- Create: `dashboard/src/components/settings/TagsSection.vue`
- Modify: `dashboard/src/views/SettingsView.vue`

**Interfaces:**
- Consumes: `useTagSettingsStore()` (Task 3) — reads/mutates `namespaces`, calls `save()`.

- [ ] **Step 1: Create the settings section**

Create `dashboard/src/components/settings/TagsSection.vue`:

```vue
<script setup>
import { ref, onMounted } from 'vue'
import { useTagSettingsStore } from '../../stores/tagSettings.js'
import { useUiStore } from '../../stores/ui.js'
import TagChip from '../shared/TagChip.vue'

const tagSettings = useTagSettingsStore()
const ui = useUiStore()
const newNamespaceName = ref('')
const newValueInput = ref({})

const SWATCHES = ['#4a9eff', '#e0607e', '#5fb8b0', '#a875d1', '#1d9e75', '#7f77dd', '#ba7517', '#d9534f']

onMounted(() => {
  if (!tagSettings.loaded) tagSettings.fetchNamespaces()
})

function setColor(ns, color) {
  tagSettings.namespaces[ns].color = color
}

function addValue(ns) {
  const v = (newValueInput.value[ns] || '').trim().toLowerCase()
  if (v && !tagSettings.namespaces[ns].values.includes(v)) {
    tagSettings.namespaces[ns].values.push(v)
  }
  newValueInput.value[ns] = ''
}

function removeValue(ns, v) {
  tagSettings.namespaces[ns].values = tagSettings.namespaces[ns].values.filter(x => x !== v)
}

function addNamespace() {
  const name = newNamespaceName.value.trim().toLowerCase()
  if (name && !tagSettings.namespaces[name]) {
    tagSettings.namespaces[name] = { color: SWATCHES[0], values: [] }
  }
  newNamespaceName.value = ''
}

async function save() {
  await tagSettings.save()
  ui.showToast('Tag namespaces saved')
}
</script>

<template>
  <div>
    <p class="hm-label mb-4">TAG NAMESPACES</p>
    <p v-if="!tagSettings.loaded" style="font-size:12px; color:var(--hm-text-tertiary)">Loading…</p>
    <template v-else>
      <div v-for="(ns, name) in tagSettings.namespaces" :key="name" class="mb-6">
        <div class="flex items-center gap-2 mb-2">
          <TagChip :tag="`${name}:example`" />
          <span class="font-mono" style="font-size:11px; color:var(--hm-text-secondary)">{{ name }}</span>
        </div>
        <div class="flex items-center gap-1.5 mb-2">
          <button v-for="c in SWATCHES" :key="c"
            class="rounded-full"
            style="width:16px; height:16px; border:1px solid var(--hm-border-subtle)"
            :style="{ background: c }"
            @click="setColor(name, c)"></button>
          <input class="hm-input" style="width:80px; height:20px; font-size:10px" v-model="ns.color" />
        </div>
        <div class="flex flex-wrap gap-1.5 mb-2">
          <TagChip v-for="v in ns.values" :key="v" :tag="`${name}:${v}`" removable @remove="removeValue(name, v)" />
          <input class="hm-input" style="width:100px; height:22px; font-size:10px"
            v-model="newValueInput[name]" placeholder="add value"
            @keydown.enter="addValue(name)" />
        </div>
      </div>
      <div class="flex items-center gap-2 mb-4">
        <input class="hm-input" style="width:140px" v-model="newNamespaceName" placeholder="new namespace" />
        <button class="hm-btn hm-btn-default hm-btn-sm" @click="addNamespace">+ Add namespace</button>
      </div>
      <button class="hm-btn hm-btn-primary" @click="save">Save tag namespaces</button>
    </template>
  </div>
</template>
```

- [ ] **Step 2: Wire it into the Settings page**

In `dashboard/src/views/SettingsView.vue`, find:

```vue
<script setup>
import ServerSection from '../components/settings/ServerSection.vue'
import SyncSection from '../components/settings/SyncSection.vue'
import DataSection from '../components/settings/DataSection.vue'
import DangerSection from '../components/settings/DangerSection.vue'
</script>

<template>
  <div class="flex-1 overflow-y-auto px-8 py-8 max-w-xl">
    <h2 class="mb-8 font-medium" style="font-size:16px; color:var(--hm-text-primary)">Settings</h2>
    <div class="flex flex-col gap-10">
      <ServerSection />
      <SyncSection />
      <DataSection />
      <DangerSection />
    </div>
  </div>
</template>
```

Change it to:

```vue
<script setup>
import ServerSection from '../components/settings/ServerSection.vue'
import SyncSection from '../components/settings/SyncSection.vue'
import TagsSection from '../components/settings/TagsSection.vue'
import DataSection from '../components/settings/DataSection.vue'
import DangerSection from '../components/settings/DangerSection.vue'
</script>

<template>
  <div class="flex-1 overflow-y-auto px-8 py-8 max-w-xl">
    <h2 class="mb-8 font-medium" style="font-size:16px; color:var(--hm-text-primary)">Settings</h2>
    <div class="flex flex-col gap-10">
      <ServerSection />
      <SyncSection />
      <TagsSection />
      <DataSection />
      <DangerSection />
    </div>
  </div>
</template>
```

- [ ] **Step 3: Manually verify**

```bash
cd dashboard && bun install && bun run dev
```

With `hivemind up` running:
1. Open Settings — confirm a "TAG NAMESPACES" section appears between Sync and Data, showing the 4 seeded namespaces (`project`, `lang`, `area`, `status`), each with a color swatch row and an empty value list.
2. Click a different swatch for `lang` — confirm the `TagChip` preview next to the namespace name updates to the new color immediately.
3. Type a value (e.g. `rust`) into `lang`'s "add value" input and press Enter — confirm a `lang:rust` chip appears in that namespace's value list.
4. Click "Save tag namespaces" — confirm a toast appears. Reload the page, reopen Settings — confirm the color and value changes persisted (fetched fresh from the server).
5. Go create/edit a memory and start typing `lang:` in the tag input (from Task 4/5) — confirm `rust` now appears as an autocomplete suggestion, proving the two features are connected end-to-end.

- [ ] **Step 4: Commit**

```bash
git add dashboard/src/components/settings/TagsSection.vue dashboard/src/views/SettingsView.vue
git commit -m "feat(dashboard): add Tag Namespaces settings section (colors, predefined values)"
```

---

### Task 7: Frontend — project-prefixed Graph node labels

**Files:**
- Modify: `dashboard/src/components/graph/GraphCanvas.vue`

**Interfaces:**
- Consumes: `nodeData` (existing computed, already includes each node's `tags` array).

- [ ] **Step 1: Add a label-formatting helper and use it in `draw()`**

In `dashboard/src/components/graph/GraphCanvas.vue`, find the `hitTestNode` helper (added by the Graph Canvas Interactivity feature) and add a new helper right after it:

```js
function nodeLabel(node) {
  const projectTag = (node.tags || []).find(t => t.toLowerCase().startsWith('project:'))
  if (!projectTag) return node.title
  const projectValue = projectTag.slice(projectTag.indexOf(':') + 1)
  return `${projectValue}: ${node.title}`
}
```

Find, inside `draw()`:

```js
    // Label at zoom >= 2, always for the selected node
    if (graph.zoom >= 2 || isSelected) {
      ctx.fillStyle = '#f2f0ec'
      ctx.font = '10px "IBM Plex Mono", monospace'
      ctx.textAlign = 'center'
      ctx.fillText(node.title.slice(0, 20), node.x, node.y + r + 13)
    }
```

Change the `fillText` call to use the new helper (keep the same 20-character truncation, applied to the combined label):

```js
    // Label at zoom >= 2, always for the selected node
    if (graph.zoom >= 2 || isSelected) {
      ctx.fillStyle = '#f2f0ec'
      ctx.font = '10px "IBM Plex Mono", monospace'
      ctx.textAlign = 'center'
      ctx.fillText(nodeLabel(node).slice(0, 20), node.x, node.y + r + 13)
    }
```

Also find the `nodeData` computed to confirm `tags` is included (it already is — this is a read-only check, no edit needed if it matches):

```js
const nodeData = computed(() =>
  memories.all.map(m => ({ id: m.id, title: m.title, layer: m.layer, tags: m.tags || [] }))
)
```

If this doesn't already include `tags: m.tags || []`, add it — but per the current codebase state (post Graph Canvas Interactivity feature), it already does.

- [ ] **Step 2: Manually verify**

```bash
cd dashboard && bun install && bun run dev
```

With `hivemind up` running and at least one memory tagged `project:something`:
1. Open the Graph page, zoom in enough to show labels (or select the node) — confirm a node with a `project:*` tag shows `"<project-value>: <title>"` instead of just the title.
2. Confirm a node WITHOUT a `project:*` tag still shows just its title, unchanged.

- [ ] **Step 3: Commit**

```bash
git add dashboard/src/components/graph/GraphCanvas.vue
git commit -m "feat(dashboard): prefix Graph node labels with project tag when present"
```
