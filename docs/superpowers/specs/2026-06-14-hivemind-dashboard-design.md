# HiveMind Dashboard — Implementation Design

**Date:** 2026-06-14  
**Status:** Approved  
**Scope:** Phase 4 — dashboard UI only (REST API is defined in the Phase 4 plan)

---

## 1. What We're Building

A four-view web dashboard for HiveMind, served at `localhost:3457`. It is a developer tool — desktop only, minimum 1100px. It talks to the HiveMind REST API at `localhost:3456` (defined in Phase 4 plan, Task 6).

This spec covers the frontend project structure, architecture, state design, and component breakdown. Visual design tokens and UX behavior are fully specified in `docs/HIVEMIND_DESIGN_SPEC.md` and `docs/HIVEMIND_DASHBOARD_SPEC.md` — this spec does not repeat them.

---

## 2. Project Structure

A proper Vite project lives at `dashboard/` in the repo root. The existing `static/index.html` is deleted.

```
hivemind/
  dashboard/
    package.json          ← bun + vite + vue + pinia + tailwindcss + d3
    vite.config.js        ← dev proxy /api → :3456; build output → dist/
    index.html            ← Vite entry; loads runtime /config.js
    src/
      main.js             ← createApp(App).use(pinia).mount('#app')
      App.vue             ← shell: sidebar + hash-routed active view + Toast
      style.css           ← all --hm-* CSS custom properties (from design spec)

      views/
        MemoriesView.vue
        GraphView.vue
        FeedbackView.vue
        SettingsView.vue

      components/
        sidebar/
          AppSidebar.vue
          StatusRow.vue
        memories/
          MemoryList.vue
          MemoryCard.vue
          MemoryDetail.vue
          DeleteConfirmModal.vue
        graph/
          GraphCanvas.vue
          GraphToolbar.vue
          PendingBar.vue
          MiniCard.vue
          EdgeCard.vue
          DetailPanel.vue
          RelationshipPicker.vue
          Legend.vue
        feedback/
          ConflictCard.vue
          FeedbackCard.vue
        settings/
          ServerSection.vue
          SyncSection.vue
          DataSection.vue
          DangerSection.vue
          DangerModal.vue
        shared/
          LayerBadge.vue
          TagChip.vue
          FilterChip.vue
          CopyButton.vue
          Toast.vue
          EmptyState.vue
          SkeletonCard.vue
          ConfirmModal.vue

      stores/
        memories.js
        graph.js
        feedback.js
        ui.js

      api/
        client.js         ← reads window.HIVEMIND_API from /config.js
        memories.js
        edges.js
        feedback.js
        settings.js
```

---

## 3. Tech Stack

| Concern | Choice | Notes |
|---|---|---|
| Framework | Vue 3 | `<script setup>` Composition API throughout |
| Build tool | Vite 6 | Dev proxy + HMR; `bun run dev` / `bun run build` |
| Package manager | Bun | Faster installs; `bun.lockb` committed |
| State | Pinia | 4 stores; no Vuex |
| Styling | Tailwind CSS v4 | CSS-native `@import "tailwindcss"`; `--hm-*` tokens in `style.css` |
| Graph | D3.js | Force simulation only; rendering on `<canvas>` (not SVG) |
| Routing | Hash routing | `window.location.hash`; no Vue Router |
| API base URL | `/config.js` | Rust injects `window.HIVEMIND_API` at runtime |

---

## 4. Routing

No Vue Router. `App.vue` maintains `activeView` in the `ui` store. Hash routing is wired in `App.vue` `onMounted`:

```js
// read hash on load
const applyHash = () => {
  const h = location.hash.replace('#/', '')
  if (['memories', 'graph', 'feedback', 'settings'].includes(h))
    ui.activeView = h
}
applyHash()
window.addEventListener('hashchange', applyHash)

// write hash on navigation
watch(() => ui.activeView, v => { location.hash = '#/' + v })
```

Default view on fresh load (no hash): `memories`.

---

## 5. Pinia Stores

### `ui` store
```
state:  activeView, serverStatus, serverInfo, syncInfo, toast
actions: showToast(message, duration=2200)
         copyToClipboard(text)      → clipboard + toast
         pollServerStatus()         → GET /api/v1/status every 30s
```

### `memories` store
```
state:  all[], selected, draft, searchQuery, layerFilter, loading, saving
getter: filtered                   → applies searchQuery + layerFilter
actions: fetchAll()                → GET /api/v1/memories
         select(entry)             → sets selected + clones draft
         save()                    → PATCH /api/v1/memories/:id from draft
         remove(id)                → DELETE /api/v1/memories/:id
```
`draft` is a shallow clone of the selected memory's editable fields (title, content, tags). Dirtiness is computed by comparing draft vs selected. No auto-save in v1.

### `graph` store
```
state:  edges[], zoom (1|2|3), selectedNodeId, connectMode, connectSourceId, pendingConnect
getter: pendingEdges               → edges where status='pending'
actions: fetchEdges()              → GET /api/v1/edges
         storeEdge(src, tgt, rel)  → POST /api/v1/edges
         resolveEdge(id, status)   → PATCH /api/v1/edges/:id
         acceptAllPending()
         rejectAllPending()
```
Nodes are derived from `memories.all` — graph store does not own memory data.

### `feedback` store
```
state:  conflicts[], feedbackItems[], activeTab ('conflicts'|'feedback'), loading
actions: fetchConflicts()          → GET /api/v1/conflicts?status=open
         fetchFeedback()           → GET /api/v1/feedback?status=open
         resolveConflict(id, action)  → POST /api/v1/conflicts/:id/resolve
         dismissFeedback(id)          → PATCH /api/v1/feedback/:id { status: dismissed }
```

---

## 6. API Layer

`src/api/client.js` is the only place that touches `window.HIVEMIND_API` or `fetch`. All other API modules call through it.

```js
const BASE = window.HIVEMIND_API || 'http://localhost:3456'

export async function request(method, path, body) {
  const res = await fetch(BASE + path, {
    method,
    headers: body !== undefined ? { 'Content-Type': 'application/json' } : {},
    body: body !== undefined ? JSON.stringify(body) : undefined,
  })
  if (!res.ok) {
    const err = new Error(res.status + ' ' + res.statusText)
    err.status = res.status
    throw err
  }
  return res.status === 204 ? null : res.json()
}
```

Each resource module (`memories.js`, `edges.js`, etc.) exports named functions like `listMemories(params)`, `patchMemory(id, body)`, etc. Stores import from these modules, not from `client.js` directly.

---

## 7. App Initialization

`App.vue` `onMounted` sequence:

1. `ui.pollStatus()` → if unreachable, render a full-screen error with Retry button
2. If reachable → parallel: `memories.fetchAll()`, `graph.fetchEdges()`, `feedback.fetchConflicts()`, `feedback.fetchFeedback()`
3. Start 30s interval: `setInterval(() => ui.pollStatus(), 30_000)`
4. Wire hash routing (see §4)

---

## 8. Component Contracts

### Shared

**`LayerBadge.vue`** — props: `layer: 'personal'|'workspace'`. Pure display.

**`TagChip.vue`** — props: `tag: string`, `removable: boolean`. Emits `remove`.

**`FilterChip.vue`** — props: `label`, `value`, `active`, `layer?`. Emits `select`.

**`CopyButton.vue`** — props: `command: string`, `label?: string`. On click: `ui.copyToClipboard(command)`.

**`Toast.vue`** — singleton, reads `ui.toast`, bottom-center of viewport, auto-dismisses.

**`EmptyState.vue`** — props: `message: string`, `icon?: string`. Centered in container.

**`SkeletonCard.vue`** — shimmer placeholder, same height as MemoryCard.

**`ConfirmModal.vue`** — props: `title`, `body`, `confirmLabel`, `dangerous`. Emits `confirm`, `cancel`.

### Memories view

**`MemoryList.vue`** — owns search input + filter chips. Renders `memories.filtered` as `MemoryCard` list. Scroll within panel. Footer shows count.

**`MemoryCard.vue`** — props: `mem`, `selected`. Emits `select`. Shows title, 1-line snippet (truncated), up to 3 tag chips, layer badge, date. Selected: left border colored by layer.

**`MemoryDetail.vue`** — reads `memories.selected` + `memories.draft`. Inline-editable title, content, tags. Connections list (from `graph.edgesFor(id)`). Save button active when `memories.dirty`. Delete triggers `DeleteConfirmModal`.

**`DeleteConfirmModal.vue`** — wraps `ConfirmModal` with memory title in body text.

### Graph view

**`GraphCanvas.vue`** — owns `<canvas>` + D3 force simulation. Accepts `nodes` (derived from `memories.all`) + `edges` (from `graph.edges`) as props. Emits: `node-click(id)`, `node-hover(node|null)`, `edge-hover(edge|null)`. Handles connect-mode internally when `graph.connectMode` is true. Reacts to `graph.zoom` changes. `ResizeObserver` keeps canvas sized.

**`GraphToolbar.vue`** — search input (highlights node), filter chips (shows/hides nodes), zoom badge + controls, fit/reset, suggest button.

**`PendingBar.vue`** — `v-if="graph.pendingEdges.length"`. Accept-all / Reject-all.

**`MiniCard.vue`** — props: `node`, `x`, `y`. `pointer-events: none`. Flip logic to avoid viewport overflow.

**`EdgeCard.vue`** — props: `edge`, `x`, `y`. `pointer-events: all`. Accept/Reject buttons call `graph.resolveEdge`.

**`DetailPanel.vue`** — slide-in, `v-show="graph.selectedNodeId"`. Read-only. Connect button sets `graph.connectMode`. Copy-edit and flag buttons use `CopyButton`.

**`RelationshipPicker.vue`** — centered overlay, `v-if="graph.pendingConnect"`. Relationship dropdown + custom input. Calls `graph.storeEdge` on confirm.

**`Legend.vue`** — bottom bar, display-only legend items.

### Feedback view

**`ConflictCard.vue`** — props: `conflict`. Two-column diff. Keep / Restore / Copy merge command actions.

**`FeedbackCard.vue`** — props: `item`. Type badge + title (click navigates to Memories) + note. Copy-edit + Dismiss actions.

### Settings view

**`ServerSection.vue`** — reads `ui.serverInfo` + `ui.serverStatus`. Read-only rows.

**`SyncSection.vue`** — local form state; toggle + conditional fields; Save calls `POST /api/v1/settings/sync`.

**`DataSection.vue`** — Export: `GET /api/v1/export` → file download. Import: hidden file input → `POST /api/v1/import`.

**`DangerSection.vue`** — triggers `DangerModal`.

**`DangerModal.vue`** — typed "DELETE" confirmation gate. `DELETE /api/v1/memories/all` on confirm.

---

## 9. Graph Canvas — D3 Implementation Notes

- Simulation: `d3.forceSimulation` with `forceManyBody` (repel), `forceLink` (spring on accepted edges), `forceCenter`.
- Nodes derived reactively from `memories.all`; simulation restarts on memory count change, edge count change, or zoom change.
- Rendering: `requestAnimationFrame` loop during simulation ticks; stops after ~300 ticks. Single `canvas` element, `getContext('2d')`.
- Hit testing on `mousemove`: circle collision for nodes (+4px tolerance), point-to-segment distance for pending edges (8px threshold).
- Zoom levels (L1/L2/L3) control what gets drawn — cluster labels at L1, node labels at L2+, tag hints at L3.
- Connect mode: crosshair cursor, pulsing source ring via `sin(Date.now())`, dashed preview edge to cursor.

---

## 10. Vite Config

```js
// vite.config.js
import { defineConfig } from 'vite'
import vue from '@vitejs/plugin-vue'
import tailwindcss from '@tailwindcss/vite'

export default defineConfig({
  plugins: [vue(), tailwindcss()],
  server: {
    port: 5173,
    proxy: { '/api': 'http://localhost:3456' }
  },
  build: {
    outDir: 'dist',
    emptyOutDir: true,
  }
})
```

---

## 11. Build Integration with Rust

`Cargo.toml` gains `include_dir`:

```toml
include_dir = "0.7"
```

`src/http.rs` (Phase 4 Task 8) serves the dashboard:

```rust
static DASHBOARD: Dir = include_dir!("$CARGO_MANIFEST_DIR/../dashboard/dist");
```

CI/build order: `cd dashboard && bun install && bun run build` must run before `cargo build --release`.

The `static/index.html` created earlier in this session is deleted as part of the first task in the implementation plan.

---

## 12. Out of Scope (v1)

- TypeScript — plain JS with JSDoc comments where helpful
- Vue Router — hash routing is sufficient
- SSR / server-side rendering
- Mobile / responsive layout
- Light mode
- Export/import UI (DataSection renders the buttons; the server endpoints are Phase 4 scope, UI calls them but graceful-degrades with a toast if not yet implemented)
- Org memory layer (Phase 6+)
