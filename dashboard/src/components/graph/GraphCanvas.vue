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

const COLORS = {
  personal: '#1d9e75',
  workspace: '#7f77dd',
  pending: '#ba7517',
}

function nodeRadius(n) {
  const connections = links.filter(l => l.source?.id === n.id || l.target?.id === n.id).length
  return Math.max(10, 10 + connections * 1.5)
}

function hitTestNode(wx, wy) {
  for (const node of nodes) {
    const r = nodeRadius(node)
    const dx = wx - node.x, dy = wy - node.y
    if (dx * dx + dy * dy <= (r + 4) * (r + 4)) return node
  }
  return null
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

  // Draw edges
  for (const link of links) {
    const sx = link.source?.x, sy = link.source?.y
    const tx = link.target?.x, ty = link.target?.y
    if (sx == null || tx == null) continue

    ctx.beginPath()
    if (link.status === 'pending') {
      ctx.setLineDash([4, 4])
      ctx.strokeStyle = COLORS.pending
      ctx.globalAlpha = 0.5
      ctx.lineWidth = 1.2
    } else {
      ctx.setLineDash([])
      ctx.strokeStyle = COLORS.personal
      ctx.globalAlpha = 0.25
      ctx.lineWidth = 1
    }
    ctx.moveTo(sx, sy)
    ctx.lineTo(tx, ty)
    ctx.stroke()
    ctx.globalAlpha = 1
    ctx.setLineDash([])
  }

  // Draw nodes
  for (const node of nodes) {
    const r = nodeRadius(node)
    const isSelected = graph.selectedNodeId === node.id
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
    ctx.globalAlpha = isSelected ? 1 : 0.72
    ctx.fill()
    ctx.globalAlpha = 1

    // Label at zoom >= 2, always for the selected node
    if (graph.zoom >= 2 || isSelected) {
      ctx.fillStyle = '#f2f0ec'
      ctx.font = '10px "IBM Plex Mono", monospace'
      ctx.textAlign = 'center'
      ctx.fillText(node.title.slice(0, 20), node.x, node.y + r + 13)
    }
  }

  ctx.restore()
}

function startSimulation() {
  if (sim) sim.stop()
  const canvas = canvasEl.value
  const w = canvas?.width || 800
  const h = canvas?.height || 600
  const pinned = loadPinned()

  nodes = nodeData.value.map(n => {
    const existing = nodes.find(x => x.id === n.id)
    if (existing) {
      return { ...n, x: existing.x, y: existing.y, vx: existing.vx, vy: existing.vy, fx: existing.fx, fy: existing.fy }
    }
    const p = pinned[n.id]
    return p ? { ...n, x: p.x, y: p.y, fx: p.x, fy: p.y } : { ...n }
  })

  links = linkData.value.map(l => ({ ...l,
    source: nodes.find(n => n.id === l.source) || l.source,
    target: nodes.find(n => n.id === l.target) || l.target,
  }))

  sim = d3.forceSimulation(nodes)
    .force('link', d3.forceLink(links).id(d => d.id).distance(80).strength(0.3))
    .force('charge', d3.forceManyBody().strength(-200))
    .force('center', d3.forceCenter(w / 2, h / 2))
    .force('collision', d3.forceCollide().radius(d => nodeRadius(d) + 8))
    .on('tick', () => { if (rafId) cancelAnimationFrame(rafId); rafId = requestAnimationFrame(draw) })
    .on('end', draw)
}

function handleClick(e) {
  if (panMode.value) return
  const [mx, my] = toWorld(e.offsetX, e.offsetY)
  const node = hitTestNode(mx, my)
  if (node) {
    if (graph.connectMode && graph.connectSourceId && graph.connectSourceId !== node.id) {
      graph.pendingConnect = { sourceId: graph.connectSourceId, targetId: node.id }
      return
    }
    graph.selectedNodeId = node.id
    emit('node-click', node.id)
    return
  }
  graph.selectedNodeId = null
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

  canvasEl.value.style.cursor = foundNode
    ? (graph.connectMode ? 'crosshair' : 'pointer')
    : graph.connectMode ? 'crosshair' : 'default'
}

let ro = null
onMounted(() => {
  ro = new ResizeObserver(() => {
    const canvas = canvasEl.value
    if (!canvas) return
    canvas.width = canvas.offsetWidth
    canvas.height = canvas.offsetHeight
    startSimulation()
  })
  ro.observe(canvasEl.value.parentElement)
  window.addEventListener('keydown', handleKeydown)

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
      if (rafId) cancelAnimationFrame(rafId)
      rafId = requestAnimationFrame(draw)
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
      sim?.alphaTarget(0.3).restart()
    })
    .on('drag', event => {
      if (!event.subject) return
      const [wx, wy] = toWorld(event.sourceEvent.offsetX, event.sourceEvent.offsetY)
      event.subject.fx = wx
      event.subject.fy = wy
      if (rafId) cancelAnimationFrame(rafId)
      rafId = requestAnimationFrame(draw)
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
  sim?.stop()
  if (rafId) cancelAnimationFrame(rafId)
  window.removeEventListener('keydown', handleKeydown)
})

watch([nodeData, linkData, () => graph.zoom], startSimulation)
</script>

<template>
  <canvas ref="canvasEl" class="w-full h-full block"
    style="background:var(--hm-bg-base)"
    @click="handleClick"
    @mousemove="handleMouseMove"
  ></canvas>
</template>
