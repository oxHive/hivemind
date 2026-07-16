<script setup>
import { ref, watch, onMounted, onUnmounted, computed } from 'vue'
import * as d3 from 'd3'
import { useMemoriesStore } from '../../stores/memories.js'
import { useGraphStore } from '../../stores/graph.js'

const emit = defineEmits(['node-click', 'node-hover', 'edge-hover'])
const memories = useMemoriesStore()
const graph = useGraphStore()

const canvasEl = ref(null)
const panMode = ref(false)
let sim = null
let rafId = null
let nodes = []
let links = []
let panning = false

const CAMERA_KEY = 'hivemind.graph.camera'

function loadCamera() {
  try {
    const raw = localStorage.getItem(CAMERA_KEY)
    if (raw) {
      const p = JSON.parse(raw)
      if (typeof p.x === 'number' && typeof p.y === 'number' && typeof p.k === 'number') return p
    }
  } catch { /* malformed storage, fall through to default */ }
  return { x: 0, y: 0, k: 1 }
}

function saveCamera() {
  localStorage.setItem(CAMERA_KEY, JSON.stringify(transform))
}

const PINNED_KEY = 'hivemind.graph.pinned'

function loadPinned() {
  try {
    const raw = localStorage.getItem(PINNED_KEY)
    return raw ? JSON.parse(raw) : {}
  } catch {
    return {}
  }
}

function savePinnedPosition(id, x, y) {
  const all = loadPinned()
  all[id] = { x, y }
  localStorage.setItem(PINNED_KEY, JSON.stringify(all))
}

let transform = loadCamera()
let zoomBehavior = null

// Single rAF gate for all redraw triggers (sim ticks, zoom, drag, filters).
function scheduleDraw() {
  if (rafId) cancelAnimationFrame(rafId)
  rafId = requestAnimationFrame(draw)
}

// getComputedStyle can force a style recalc; calling it every frame during a
// drag adds measurable jank. Cache the two theme colors and invalidate only
// when the theme actually flips (theme store stamps data-theme on <html>).
let themeColors = null
function getThemeColors() {
  if (!themeColors) {
    const cs = getComputedStyle(document.documentElement)
    themeColors = {
      text: cs.getPropertyValue('--hm-text-primary').trim(),
      draft: cs.getPropertyValue('--hm-warning').trim(),
    }
  }
  return themeColors
}

function toWorld(px, py) {
  return [(px - transform.x) / transform.k, (py - transform.y) / transform.k]
}

function isEditableTarget(el) {
  return !!el && (el.tagName === 'INPUT' || el.tagName === 'TEXTAREA' || el.isContentEditable)
}

function handleKeydown(e) {
  if (isEditableTarget(document.activeElement)) return
  if (e.code === 'Space') {
    e.preventDefault()
    panMode.value = !panMode.value
  } else if (e.key === 'Escape') {
    panMode.value = false
  }
}

const nodeData = computed(() =>
  memories.all.map(m => ({ id: m.id, title: m.title, layer: m.layer, tags: m.tags || [] }))
)

const linkData = computed(() =>
  graph.edges
    .filter(e => e.status === 'active' || e.status === 'pending')
    .map(e => ({ id: e.id, source: e.source_id, target: e.target_id, status: e.status, relationship: e.relationship }))
)

// Camera scale (d3 zoom `transform.k`) below which per-node text is too
// small to read on screen — swap to a single cluster label instead.
const LABEL_MIN_SCALE = 0.55

const COLORS = {
  personal: '#1d9e75',
  workspace: '#7f77dd',
  pending: '#ba7517',
}

// Brighter, higher-contrast than the original muted set — at low alpha and
// 1px width the old colors (#5b8fd9/#c2634a/#9a63d6) were nearly indistinguishable
// from each other and from the background.
const RELATIONSHIP_COLORS = {
  parent: '#4d9bff',
  child: '#ff7a45',
  sibling: '#c084fc',
}
// Pre-existing edges whose relationship predates this taxonomy (shares_tag,
// applies_to, etc.) are left in the DB unmigrated — this is their fallback color.
const DEFAULT_EDGE_COLOR = '#9a9488'

// Degree is cached on each node (set in startSimulation whenever links
// change) instead of scanning `links` on every call — nodeRadius runs once
// per node per animation frame plus once per node per collision-force tick,
// so an O(links) scan there was the main cause of drag-time jank.
// Anchors every node — including edgeless ones — toward the live centroid
// of the *connected* nodes (the visual cluster), instead of the fixed
// canvas center. A fixed-center anchor pulls edgeless nodes toward canvas
// middle even when the cluster itself has drifted elsewhere, which reads as
// the edgeless nodes "escaping" the cluster.
function forceClusterAnchor(strength) {
  let nodesRef = []
  function force(alpha) {
    let cx = 0, cy = 0, count = 0
    for (const n of nodesRef) {
      if (n.__degree > 0) { cx += n.x; cy += n.y; count++ }
    }
    if (count === 0) return
    cx /= count
    cy /= count
    for (const n of nodesRef) {
      if (n.fx != null && n.fy != null) continue
      n.vx += (cx - n.x) * strength * alpha
      n.vy += (cy - n.y) * strength * alpha
    }
  }
  force.initialize = (ns) => { nodesRef = ns }
  return force
}

function nodeRadius(n) {
  return Math.max(10, 10 + (n.__degree || 0) * 1.5)
}

function hitTestNode(wx, wy) {
  // Reverse order: nodes later in the array draw on top, so when hexes
  // overlap the click should land on the visible (topmost) one.
  for (let i = nodes.length - 1; i >= 0; i--) {
    const node = nodes[i]
    const r = nodeRadius(node)
    const dx = wx - node.x, dy = wy - node.y
    if (dx * dx + dy * dy <= (r + 4) * (r + 4)) return node
  }
  return null
}

function projectOf(node) {
  const tag = (node.tags || []).find(t => t.toLowerCase().startsWith('project:'))
  return tag ? tag.slice(tag.indexOf(':') + 1) : null
}

// The project cluster blob + label already identify which project a node
// belongs to — no need to repeat it as a per-node title prefix.
function nodeLabel(node) {
  return { isDraft: memories.isDraft(node.id), text: node.title }
}

// Stable pastel color per project name — same string always hashes to the
// same hue so a project's cluster color doesn't shuffle between renders.
const projectColorCache = new Map()
function projectColor(name) {
  if (projectColorCache.has(name)) return projectColorCache.get(name)
  let hash = 0
  for (let i = 0; i < name.length; i++) hash = (hash * 31 + name.charCodeAt(i)) >>> 0
  const hue = hash % 360
  projectColorCache.set(name, hue)
  return hue
}

// Groups nodes by their precomputed __project (set when nodes are built or
// patched) — this runs every frame and every sim tick, so it must not redo
// the per-tag string scanning projectOf does.
function groupByProject(list) {
  const groups = new Map()
  for (const n of list) {
    if (!n.__project) continue
    if (!groups.has(n.__project)) groups.set(n.__project, [])
    groups.get(n.__project).push(n)
  }
  return groups
}

// Pulls nodes sharing the same project: tag toward their group's centroid,
// so project clusters visually separate from each other on the canvas.
function forceProjectCluster(strength) {
  let nodesRef = []
  function force(alpha) {
    const groups = groupByProject(nodesRef)
    for (const members of groups.values()) {
      if (members.length < 2) continue
      let cx = 0, cy = 0
      for (const n of members) { cx += n.x; cy += n.y }
      cx /= members.length
      cy /= members.length
      for (const n of members) {
        if (n.fx != null && n.fy != null) continue
        n.vx += (cx - n.x) * strength * alpha
        n.vy += (cy - n.y) * strength * alpha
      }
    }
  }
  force.initialize = (ns) => { nodesRef = ns }
  return force
}

// Expands a convex hull outward by `pad` along each vertex's normal
// (relative to the hull centroid) so the outline clears the node hexes
// instead of clipping through their centers.
function padHull(hull, pad) {
  let cx = 0, cy = 0
  for (const [x, y] of hull) { cx += x; cy += y }
  cx /= hull.length
  cy /= hull.length
  return hull.map(([x, y]) => {
    const dx = x - cx, dy = y - cy
    const d = Math.hypot(dx, dy) || 1
    return [x + (dx / d) * pad, y + (dy / d) * pad]
  })
}

// Draws a soft, rounded blob behind a project's nodes — a closed Catmull-Rom-
// style spline through the padded hull so corners look organic rather than
// polygonal.
function drawClusterBlob(ctx, members, hue) {
  const pts = members.map(n => [n.x, n.y])
  let hull = pts.length >= 3 ? d3.polygonHull(pts) : pts
  if (!hull || hull.length === 0) return
  if (hull.length < 3) {
    // 1-2 nodes: no real hull: draw a soft circle/capsule around them instead.
    const pad = 34
    ctx.beginPath()
    if (hull.length === 1) {
      ctx.arc(hull[0][0], hull[0][1], nodeRadius(members[0]) + pad, 0, Math.PI * 2)
    } else {
      const [[ax, ay], [bx, by]] = hull
      const r = Math.max(nodeRadius(members[0]), nodeRadius(members[1])) + pad
      const dx = bx - ax, dy = by - ay
      const len = Math.hypot(dx, dy) || 1
      const nx = -dy / len, ny = dx / len
      ctx.moveTo(ax + nx * r, ay + ny * r)
      ctx.lineTo(bx + nx * r, by + ny * r)
      ctx.arc(bx, by, r, Math.atan2(ny, nx), Math.atan2(-ny, -nx), true)
      ctx.lineTo(ax - nx * r, ay - ny * r)
      ctx.arc(ax, ay, r, Math.atan2(-ny, -nx), Math.atan2(ny, nx), true)
    }
    ctx.closePath()
  } else {
    const padded = padHull(hull, 30)
    const n = padded.length
    ctx.beginPath()
    for (let i = 0; i < n; i++) {
      const p0 = padded[(i - 1 + n) % n]
      const p1 = padded[i]
      const p2 = padded[(i + 1) % n]
      const p3 = padded[(i + 2) % n]
      const cp1x = p1[0] + (p2[0] - p0[0]) / 6
      const cp1y = p1[1] + (p2[1] - p0[1]) / 6
      const cp2x = p2[0] - (p3[0] - p1[0]) / 6
      const cp2y = p2[1] - (p3[1] - p1[1]) / 6
      if (i === 0) ctx.moveTo(p1[0], p1[1])
      ctx.bezierCurveTo(cp1x, cp1y, cp2x, cp2y, p2[0], p2[1])
    }
    ctx.closePath()
  }
  ctx.fillStyle = `hsla(${hue}, 65%, 55%, 0.12)`
  ctx.strokeStyle = `hsla(${hue}, 65%, 55%, 0.4)`
  ctx.lineWidth = 1.5
  ctx.fill()
  ctx.stroke()
}

// Labels the cluster with its project name, sitting just above the blob so
// it doesn't collide with node hexes/titles at any zoom level. Drawn at
// every zoom (not just zoomed-out), since node titles no longer repeat the
// project name — this is the only place it appears now.
function drawClusterLabel(ctx, name, members, hue, scale) {
  let cx = 0, top = Infinity
  for (const n of members) {
    cx += n.x
    top = Math.min(top, n.y - nodeRadius(n))
  }
  cx /= members.length
  // Counter-scale the font against the camera zoom so the label stays a
  // roughly constant, readable size on screen as the user scrolls in/out,
  // instead of shrinking or growing along with everything else in world space.
  const screenPx = 17
  const worldPx = Math.min(48, screenPx / Math.max(scale, 0.05))
  ctx.font = `bold ${worldPx}px "IBM Plex Mono", monospace`
  ctx.textAlign = 'center'
  ctx.textBaseline = 'bottom'
  ctx.fillStyle = `hsla(${hue}, 70%, 40%, 0.9)`
  // 30 clears the blob's hull padding (see drawClusterBlob), 14 is a small gap above it.
  ctx.fillText(name, cx, top - 30 - 14)
  ctx.textAlign = 'left'
  ctx.textBaseline = 'alphabetic'
}

// Memories render as hexagonal cells — the honeycomb is the graph.
function traceHex(ctx, x, y, r) {
  ctx.beginPath()
  for (let i = 0; i < 6; i++) {
    const a = (Math.PI / 180) * (60 * i - 90)
    const px = x + r * Math.cos(a)
    const py = y + r * Math.sin(a)
    if (i === 0) ctx.moveTo(px, py)
    else ctx.lineTo(px, py)
  }
  ctx.closePath()
}

function draw() {
  const canvas = canvasEl.value
  if (!canvas) return
  const ctx = canvas.getContext('2d')
  const w = canvas.width
  const h = canvas.height
  ctx.clearRect(0, 0, w, h)

  ctx.save()
  ctx.translate(transform.x, transform.y)
  ctx.scale(transform.k, transform.k)

  // Below this camera scale, a 10px node label renders under ~5 screen
  // pixels — illegible. Swap per-node titles for one label per cluster.
  const showNodeLabels = transform.k >= LABEL_MIN_SCALE

  // Draw project clusters — soft outline behind everything else.
  const projectGroups = groupByProject(nodes)
  for (const [name, members] of projectGroups) {
    drawClusterBlob(ctx, members, projectColor(name))
    drawClusterLabel(ctx, name, members, projectColor(name), transform.k)
  }

  // Draw edges — anchored to each node's boundary (not center) along the
  // line between the two centers, so they track node radius/position live.
  for (const link of links) {
    let sx = link.source?.x, sy = link.source?.y
    let tx = link.target?.x, ty = link.target?.y
    if (sx == null || tx == null) continue

    const dx = tx - sx, dy = ty - sy
    const dist = Math.hypot(dx, dy)
    if (dist > 0) {
      const ux = dx / dist, uy = dy / dist
      const sr = nodeRadius(link.source)
      const tr = nodeRadius(link.target)
      sx += ux * sr
      sy += uy * sr
      tx -= ux * tr
      ty -= uy * tr
    }

    ctx.beginPath()
    if (link.status === 'pending') {
      ctx.setLineDash([4, 4])
      ctx.strokeStyle = COLORS.pending
      ctx.globalAlpha = 0.75
      ctx.lineWidth = 2
    } else {
      ctx.setLineDash([])
      ctx.strokeStyle = RELATIONSHIP_COLORS[link.relationship] || DEFAULT_EDGE_COLOR
      ctx.globalAlpha = 0.6
      ctx.lineWidth = 2
    }
    if (graph.selectedEdgeId && link.id === graph.selectedEdgeId) {
      ctx.strokeStyle = COLORS.pending
      ctx.globalAlpha = 1
      ctx.lineWidth = 3.5
    }
    ctx.moveTo(sx, sy)
    ctx.lineTo(tx, ty)
    ctx.stroke()
    ctx.globalAlpha = 1
    ctx.setLineDash([])
  }

  // Draw nodes
  const query = graph.searchQuery.trim().toLowerCase()
  const { text: textColor, draft: draftColor } = getThemeColors()
  for (const node of nodes) {
    const r = nodeRadius(node)
    const isSelected = graph.selectedNodeId === node.id
    const matchesSearch = !query || node.title.toLowerCase().includes(query)
    const matchesLayer = graph.layerFilter === 'all' || node.layer === graph.layerFilter
    const isMatch = matchesSearch && matchesLayer
    const color = COLORS[node.layer] || COLORS.personal

    // Ring for selected
    if (isSelected) {
      traceHex(ctx, node.x, node.y, r + 3.5)
      ctx.strokeStyle = color
      ctx.globalAlpha = 0.4
      ctx.lineWidth = 1.5
      ctx.stroke()
      ctx.globalAlpha = 1
    }

    traceHex(ctx, node.x, node.y, r)
    ctx.fillStyle = color
    ctx.globalAlpha = !isMatch ? 0.15 : isSelected ? 1 : 0.72
    ctx.fill()
    ctx.globalAlpha = 1

    // Label at zoom >= 2, always for the selected node — suppressed once
    // the camera is zoomed out far enough that text would be illegible.
    if (showNodeLabels && ((graph.zoom >= 2 && isMatch) || isSelected)) {
      const label = nodeLabel(node)
      const text = label.text.slice(0, 20)
      const draftPrefix = label.isDraft ? '[DRAFT] ' : ''
      ctx.font = '10px "IBM Plex Mono", monospace'
      ctx.textAlign = 'left'
      const prefixWidth = draftPrefix ? ctx.measureText(draftPrefix).width : 0
      const textWidth = ctx.measureText(text).width
      const startX = node.x - (prefixWidth + textWidth) / 2
      const y = node.y + r + 13
      if (draftPrefix) {
        ctx.fillStyle = draftColor
        ctx.fillText(draftPrefix, startX, y)
      }
      ctx.fillStyle = textColor
      ctx.fillText(text, startX + prefixWidth, y)
    }
  }

  ctx.restore()
}

let lastTopologyKey = null

// A signature of node/link ids — cheap to compare so we can tell a real
// topology change (a memory or edge added/removed) apart from the SSE
// stream just handing us a same-content-but-new-reference array on every
// backend event. Recreating the simulation on every SSE tick reset alpha to
// 1 each time, which kept re-perturbing every unpinned node forever — most
// visible on edgeless nodes since they had only weak forces holding them,
// while linked nodes got pulled back into place by the link force.
function topologyKey() {
  const nodeIds = nodeData.value.map(n => n.id).sort().join(',')
  const linkIds = linkData.value.map(l => l.id).sort().join(',')
  return `${nodeIds}|${linkIds}`
}

// When node/link ids are unchanged, the sim keeps running on its existing
// arrays — but the store data may still have changed in place: a pending
// edge got accepted (same id, new status), or a memory's title/tags/layer
// got edited. Patch those fields onto the live sim objects so the render
// tracks the store without a full restart.
function syncData() {
  const nodeById = new Map(nodes.map(n => [n.id, n]))
  for (const nd of nodeData.value) {
    const n = nodeById.get(nd.id)
    if (!n) continue
    n.title = nd.title
    n.layer = nd.layer
    n.tags = nd.tags
    n.__project = projectOf(nd)
  }
  const linkById = new Map(links.map(l => [l.id, l]))
  for (const ld of linkData.value) {
    const l = linkById.get(ld.id)
    if (!l) continue
    l.status = ld.status
    l.relationship = ld.relationship
  }
  scheduleDraw()
}

function startSimulation(force = false) {
  const key = topologyKey()
  if (!force && sim && key === lastTopologyKey) {
    syncData()
    return
  }
  lastTopologyKey = key

  if (sim) sim.stop()
  const canvas = canvasEl.value
  const w = canvas?.width || 800
  const h = canvas?.height || 600
  const pinned = loadPinned()

  const prevById = new Map(nodes.map(n => [n.id, n]))
  nodes = nodeData.value.map(n => {
    const existing = prevById.get(n.id)
    const base = existing
      ? { ...n, x: existing.x, y: existing.y, vx: existing.vx, vy: existing.vy, fx: existing.fx, fy: existing.fy }
      : (pinned[n.id] ? { ...n, x: pinned[n.id].x, y: pinned[n.id].y, fx: pinned[n.id].x, fy: pinned[n.id].y } : { ...n })
    base.__project = projectOf(n)
    return base
  })

  // Drop links whose endpoints aren't in the node set. The DB cascades edge
  // deletes, but the dashboard refreshes memories and edges independently,
  // so there's a window where an edge references a just-deleted memory —
  // and d3.forceLink throws ("missing: <id>") on any unresolvable link,
  // which would kill the whole simulation.
  const nodeById = new Map(nodes.map(n => [n.id, n]))
  links = linkData.value
    .map(l => ({ ...l, source: nodeById.get(l.source), target: nodeById.get(l.target) }))
    .filter(l => l.source && l.target)

  for (const n of nodes) n.__degree = 0
  for (const l of links) {
    l.source.__degree++
    l.target.__degree++
  }

  sim = d3.forceSimulation(nodes)
    .force('link', d3.forceLink(links).id(d => d.id).distance(80).strength(0.3))
    // distanceMax bounds charge to nearby nodes — without it, dragging one
    // node perturbs the pairwise repulsion balance for every node in the
    // graph, including ones with no edges, so edgeless nodes would drift
    // away from the cluster on every drag.
    .force('charge', d3.forceManyBody().strength(-200).distanceMax(300))
    // Weak forceX/forceY instead of forceCenter: forceCenter rigidly
    // translates ALL nodes (including pinned ones, which then snap back to
    // fx/fy) to keep the mean at canvas center — with any pinned node away
    // from center that tug-of-war repeats every tick and shows up as the
    // unpinned nodes slowly drifting. forceX/Y act through velocity, which
    // fixed nodes simply ignore, so pinning is conflict-free.
    .force('x', d3.forceX(w / 2).strength(0.02))
    .force('y', d3.forceY(h / 2).strength(0.02))
    .force('clusterAnchor', forceClusterAnchor(0.03))
    .force('projectCluster', forceProjectCluster(0.08))
    .force('collision', d3.forceCollide().radius(d => nodeRadius(d) + 8))
    .on('tick', scheduleDraw)
    .on('end', draw)
}

function handleClick(e) {
  if (panMode.value) return
  const [mx, my] = toWorld(e.offsetX, e.offsetY)
  const node = hitTestNode(mx, my)
  if (node) {
    graph.selectedNodeId = node.id
    emit('node-click', node.id)
    return
  }
  graph.selectedNodeId = null
  graph.selectedEdgeId = null
}

function ptSegDist(px, py, ax, ay, bx, by) {
  const dx = bx - ax, dy = by - ay
  if (dx === 0 && dy === 0) return Math.hypot(px - ax, py - ay)
  const t = Math.max(0, Math.min(1, ((px - ax) * dx + (py - ay) * dy) / (dx * dx + dy * dy)))
  return Math.hypot(px - ax - t * dx, py - ay - t * dy)
}

function handleMouseMove(e) {
  if (panMode.value) {
    canvasEl.value.style.cursor = panning ? 'grabbing' : 'grab'
    emit('node-hover', null)
    emit('edge-hover', null)
    return
  }
  const [mx, my] = toWorld(e.offsetX, e.offsetY)
  const foundNode = hitTestNode(mx, my)
  emit('node-hover', foundNode)

  let foundEdge = null
  if (!foundNode) {
    for (const link of links) {
      if (link.status !== 'pending') continue
      const sx = link.source?.x, sy = link.source?.y
      const tx = link.target?.x, ty = link.target?.y
      if (sx == null || tx == null) continue
      if (ptSegDist(mx, my, sx, sy, tx, ty) <= 8) { foundEdge = link; break }
    }
  }
  emit('edge-hover', foundEdge)

  canvasEl.value.style.cursor = foundNode ? 'pointer' : 'default'
}

let ro = null
let themeObserver = null
onMounted(() => {
  ro = new ResizeObserver(() => {
    const canvas = canvasEl.value
    if (!canvas) return
    canvas.width = canvas.offsetWidth
    canvas.height = canvas.offsetHeight
    // A resize doesn't change the graph — rebuilding the simulation here
    // reset alpha to 1 on every window resize and made the layout churn.
    // Just retarget the centering forces at the new canvas center and repaint.
    if (!sim) {
      startSimulation(true)
      return
    }
    sim.force('x')?.x(canvas.width / 2)
    sim.force('y')?.y(canvas.height / 2)
    scheduleDraw()
  })
  ro.observe(canvasEl.value.parentElement)
  window.addEventListener('keydown', handleKeydown)

  // Theme flips restyle CSS variables — drop the cached canvas colors and
  // repaint (the canvas doesn't react to CSS changes on its own).
  themeObserver = new MutationObserver(() => { themeColors = null; scheduleDraw() })
  themeObserver.observe(document.documentElement, { attributes: true, attributeFilter: ['data-theme'] })

  zoomBehavior = d3.zoom()
    .scaleExtent([0.2, 5])
    .filter(event => event.type === 'wheel' || panMode.value)
    .on('start', event => {
      if (panMode.value && event.sourceEvent?.type !== 'wheel') {
        panning = true
        canvasEl.value.style.cursor = 'grabbing'
      }
    })
    .on('zoom', event => {
      transform.x = event.transform.x
      transform.y = event.transform.y
      transform.k = event.transform.k
      scheduleDraw()
    })
    .on('end', event => {
      saveCamera()
      if (event.sourceEvent?.type !== 'wheel') {
        panning = false
        if (panMode.value) canvasEl.value.style.cursor = 'grab'
      }
    })
  const sel = d3.select(canvasEl.value)
  sel.call(zoomBehavior)
  sel.call(zoomBehavior.transform, d3.zoomIdentity.translate(transform.x, transform.y).scale(transform.k))

  const dragBehavior = d3.drag()
    .filter(() => !panMode.value)
    .subject(event => {
      const [mx, my] = toWorld(event.x, event.y)
      return hitTestNode(mx, my)
    })
    .on('start', event => {
      if (!event.subject) return
      event.subject.__wasPinned = event.subject.fx != null
      event.subject.fx = event.subject.x
      event.subject.fy = event.subject.y
      event.subject.__dragStartScreen = [event.x, event.y]
      // Lower alphaTarget than d3's usual 0.3 default — 0.3 keeps the whole
      // simulation "hot" enough that neighboring nodes visibly jostle around
      // the dragged node every tick, which reads as clunky. 0.08 still lets
      // linked neighbors ease into the new position without the jitter.
      sim?.alphaTarget(0.08).restart()
    })
    .on('drag', event => {
      if (!event.subject) return
      const [wx, wy] = toWorld(event.sourceEvent.offsetX, event.sourceEvent.offsetY)
      event.subject.fx = wx
      event.subject.fy = wy
      scheduleDraw()
    })
    .on('end', event => {
      if (!event.subject) return
      sim?.alphaTarget(0)
      const [sx, sy] = event.subject.__dragStartScreen || [event.x, event.y]
      const moved = Math.hypot(event.x - sx, event.y - sy) > 3
      const wasPinned = event.subject.__wasPinned
      delete event.subject.__dragStartScreen
      delete event.subject.__wasPinned
      if (moved) {
        savePinnedPosition(event.subject.id, event.subject.fx, event.subject.fy)
      } else if (!wasPinned) {
        event.subject.fx = null
        event.subject.fy = null
      }
    })
  sel.call(dragBehavior)
})

onUnmounted(() => {
  ro?.disconnect()
  themeObserver?.disconnect()
  sim?.stop()
  if (rafId) cancelAnimationFrame(rafId)
  window.removeEventListener('keydown', handleKeydown)
})

watch([nodeData, linkData], () => startSimulation())
watch([() => graph.zoom, () => graph.searchQuery, () => graph.layerFilter, () => graph.selectedNodeId, () => graph.selectedEdgeId], scheduleDraw)
</script>

<template>
  <canvas ref="canvasEl" class="w-full h-full block"
    style="background:var(--hm-bg-base)"
    @click="handleClick"
    @mousemove="handleMouseMove"
  ></canvas>
</template>
