<script setup>
import { ref, watch, onMounted, onUnmounted, computed } from 'vue'
import * as d3 from 'd3'
import { useMemoriesStore } from '../../stores/memories.js'
import { useGraphStore } from '../../stores/graph.js'

const emit = defineEmits(['node-click', 'node-hover', 'edge-hover'])
const memories = useMemoriesStore()
const graph = useGraphStore()

const canvasEl = ref(null)
let sim = null
let rafId = null
let nodes = []
let links = []

const nodeData = computed(() =>
  memories.all.map(m => ({ id: m.id, title: m.title, layer: m.layer, tags: m.tags || [] }))
)

const linkData = computed(() =>
  graph.edges
    .filter(e => e.status === 'accepted' || e.status === 'pending')
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

function draw() {
  const canvas = canvasEl.value
  if (!canvas) return
  const ctx = canvas.getContext('2d')
  const w = canvas.width
  const h = canvas.height
  ctx.clearRect(0, 0, w, h)

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
      ctx.beginPath()
      ctx.arc(node.x, node.y, r + 3.5, 0, Math.PI * 2)
      ctx.strokeStyle = color
      ctx.globalAlpha = 0.4
      ctx.lineWidth = 1.5
      ctx.stroke()
      ctx.globalAlpha = 1
    }

    ctx.beginPath()
    ctx.arc(node.x, node.y, r, 0, Math.PI * 2)
    ctx.fillStyle = color
    ctx.globalAlpha = isSelected ? 1 : 0.72
    ctx.fill()
    ctx.globalAlpha = 1

    // Label at zoom >= 2
    if (graph.zoom >= 2) {
      ctx.fillStyle = '#f0f0f0'
      ctx.font = '10px "JetBrains Mono", monospace'
      ctx.textAlign = 'center'
      ctx.fillText(node.title.slice(0, 20), node.x, node.y + r + 13)
    }
  }
}

function startSimulation() {
  if (sim) sim.stop()
  const canvas = canvasEl.value
  const w = canvas?.width || 800
  const h = canvas?.height || 600

  nodes = nodeData.value.map(n => {
    const existing = nodes.find(x => x.id === n.id)
    return existing ? { ...n, x: existing.x, y: existing.y, vx: existing.vx, vy: existing.vy } : { ...n }
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
  const { offsetX: mx, offsetY: my } = e
  for (const node of nodes) {
    const r = nodeRadius(node)
    const dx = mx - node.x, dy = my - node.y
    if (dx * dx + dy * dy <= (r + 4) * (r + 4)) {
      if (graph.connectMode && graph.connectSourceId && graph.connectSourceId !== node.id) {
        graph.pendingConnect = { sourceId: graph.connectSourceId, targetId: node.id }
        return
      }
      graph.selectedNodeId = node.id
      emit('node-click', node.id)
      return
    }
  }
  graph.selectedNodeId = null
}

function handleMouseMove(e) {
  const { offsetX: mx, offsetY: my } = e
  let found = null
  for (const node of nodes) {
    const r = nodeRadius(node)
    const dx = mx - node.x, dy = my - node.y
    if (dx * dx + dy * dy <= (r + 4) * (r + 4)) { found = node; break }
  }
  emit('node-hover', found)
  canvasEl.value.style.cursor = found
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
})

onUnmounted(() => {
  ro?.disconnect()
  sim?.stop()
  if (rafId) cancelAnimationFrame(rafId)
})

watch([() => memories.all.length, () => graph.edges.length, () => graph.zoom], startSimulation)
</script>

<template>
  <canvas ref="canvasEl" class="w-full h-full block"
    style="background:var(--hm-bg-base)"
    @click="handleClick"
    @mousemove="handleMouseMove"
  ></canvas>
</template>
