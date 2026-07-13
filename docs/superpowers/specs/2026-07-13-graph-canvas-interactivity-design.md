---
title: Graph Canvas Interactivity — Pan, Zoom, Node Drag
date: 2026-07-13
status: approved
---

## Overview

`dashboard/src/components/graph/GraphCanvas.vue` renders the force-directed memory graph on a raw `<canvas>` with manual hit-testing (no SVG, no existing pan/zoom/drag library usage beyond d3-force). This adds three interactions to that canvas:

1. Drag a node to reposition it — pins permanently (`fx`/`fy`), survives refresh.
2. Pan the canvas — toggled by spacebar (Pan Mode), also exits on Escape.
3. Zoom the camera — mouse wheel, always active regardless of Pan Mode.

Camera position/scale and pinned node positions persist in `localStorage`, per-browser.

## Approach

Use d3's own `d3.zoom` and `d3.drag` behaviors — d3 is already a dependency (`d3-force` is already used) and these modules are purpose-built for exactly this. Since rendering is canvas (not SVG), the zoom transform `{x, y, k}` is tracked as plain reactive state, applied manually via `ctx.translate`/`ctx.scale` inside `draw()`, and inverted to map screen coordinates → world coordinates for hit-testing and dragging (node `x`/`y` in the simulation stay in world space, unaffected by camera state).

## Camera (pan/zoom)

- `d3.zoom()` bound to the canvas selection via `.call()`.
- `.filter()`: wheel events always pass (zoom always active); mousedown-drag only passes when `panMode` is true (so zoom's built-in drag-to-pan doesn't fight node dragging).
- `.scaleExtent([0.2, 5])`.
- `on('zoom', ...)`: update `transform` ref, redraw via existing `requestAnimationFrame` pattern.
- `on('end', ...)`: persist `transform` to `localStorage` (avoids writing on every intermediate frame).
- `draw()`: `ctx.clearRect` happens in identity space first (untransformed), then `ctx.save()` / apply `transform` / existing draw logic (unchanged, already in world coordinates) / `ctx.restore()`.

## Pan Mode toggle

- New `panMode` ref (local component state, not persisted — always starts off).
- `keydown` listener on `window` (added in `onMounted`, removed in `onUnmounted`): spacebar toggles `panMode`, Escape forces it off. Ignored when `document.activeElement` is an `input`/`textarea` (so it doesn't fight the toolbar's "Find node…" search box) and calls `preventDefault()` on spacebar to stop page scroll.
- Cursor: `grab` while `panMode` is true and idle, `grabbing` while a pan drag is in progress; existing pointer/default/crosshair logic unchanged otherwise.
- While `panMode` is true, node dragging is disabled entirely (see below) — the two never compete for the same mousedown.

## Node dragging

- `d3.drag()` bound to the same canvas selection.
- `.filter()`: returns `false` whenever `panMode` is true (so `d3.zoom`'s pan owns the gesture instead).
- `.subject(event)`: converts the pointer position to world space via the inverse of the current `transform`, reuses the existing radius hit-test to find a node there; returns `undefined` if none (no drag starts, click/select behavior unaffected).
- `on('start')`: seed `fx`/`fy` from current `x`/`y`.
- `on('drag')`: update `fx`/`fy` from the (world-space-converted) pointer position; redraw via the existing `requestAnimationFrame` pattern.
- `on('end')`: leave `fx`/`fy` set (pinned permanently, per decision) and write the pinned position to `localStorage`.
- `startSimulation()`'s existing id-based merge (which already carries over `x`/`y`/`vx`/`vy` across data refreshes) is extended to also carry over `fx`/`fy` for nodes already in memory, and to seed `fx`/`fy` from the `localStorage` pinned-position map for nodes not yet seen this session.

## Persistence

Two `localStorage` keys, both loaded once on mount:

- `hivemind.graph.camera` — JSON `{x, y, k}`.
- `hivemind.graph.pinned` — JSON map `{[nodeId]: {x, y}}`.

Both are per-browser only (no backend/API involvement, no sync). No unpin affordance is included — out of scope for this pass.

## Explicitly out of scope

- The existing L1/L2/L3 label-detail stepper is untouched and stays independent of camera zoom (confirmed with the user).
- No inertia/momentum on pan or zoom beyond d3-zoom's defaults.
- No touch/pinch-zoom support beyond whatever `d3.zoom` provides for free.

## Testing

This feature is inherently interactive (mouse drag, wheel, keyboard) with no meaningful unit-testable logic beyond coordinate math. If a screen/coordinate ↔ world conversion helper ends up factored out as a pure function, it gets a couple of unit tests; otherwise verification is manual/visual — drive it in a browser if tooling is available in the session, and say so explicitly if it isn't rather than claiming untested behavior works.
