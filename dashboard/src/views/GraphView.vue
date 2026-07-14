<script setup>
import { ref, computed } from 'vue'
import { useMemoriesStore } from '../stores/memories.js'
import GraphCanvas from '../components/graph/GraphCanvas.vue'
import GraphToolbar from '../components/graph/GraphToolbar.vue'
import PendingBar from '../components/graph/PendingBar.vue'
import DetailPanel from '../components/graph/DetailPanel.vue'
import Legend from '../components/graph/Legend.vue'
import EmptyState from '../components/shared/EmptyState.vue'
import Tooltip from '../components/shared/Tooltip.vue'

const memories = useMemoriesStore()

const hoveredNode = ref(null)
const hoveredEdge = ref(null)
const mouseX = ref(0)
const mouseY = ref(0)

function onCanvasMouseMove(e) {
  mouseX.value = e.clientX
  mouseY.value = e.clientY
}

const tooltipText = computed(() => {
  if (hoveredNode.value) return hoveredNode.value.title
  if (hoveredEdge.value) return hoveredEdge.value.relationship || 'pending link'
  return ''
})
</script>

<template>
  <div class="flex flex-1 overflow-hidden">
    <!-- Left: canvas area -->
    <div class="flex flex-col flex-1 overflow-hidden">
      <GraphToolbar />
      <PendingBar />
      <div class="flex-1 relative overflow-hidden" @mousemove="onCanvasMouseMove">
        <EmptyState
          v-if="!memories.all.length"
          message="No connections to display."
          hint="Store some memories first; edges appear as they share tags or get linked."
        />
        <GraphCanvas v-else @node-hover="hoveredNode = $event" @edge-hover="hoveredEdge = $event" />
      </div>
      <Legend />
    </div>

    <!-- Right: detail panel (slides in) -->
    <DetailPanel />

    <Tooltip :visible="!!(hoveredNode || hoveredEdge)" :text="tooltipText" :x="mouseX" :y="mouseY" />
  </div>
</template>
