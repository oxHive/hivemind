<script setup>
import { useGraphStore } from '../../stores/graph.js'
import { useSuggestStore } from '../../stores/suggest.js'
const graph = useGraphStore()
const suggest = useSuggestStore()
</script>

<template>
  <div v-if="graph.pendingEdges.length && !suggest.panelOpen"
    class="flex items-center justify-between px-3.5 py-1.5 shrink-0"
    style="background:var(--hm-warning-bg); border-bottom:0.5px solid var(--hm-warning-border)">
    <span style="font-size:12px; color:var(--hm-warning)">
      ✦ {{ graph.pendingEdges.length }} connection {{ graph.pendingEdges.length === 1 ? 'suggestion' : 'suggestions' }} pending
    </span>
    <button class="hm-btn hm-btn-sm hm-btn-primary" @click="suggest.openPanel()">Review</button>
  </div>
</template>
