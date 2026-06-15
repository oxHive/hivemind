<script setup>
import { useGraphStore } from '../../stores/graph.js'
const graph = useGraphStore()
</script>

<template>
  <div v-if="graph.pendingEdges.length" class="flex items-center justify-between px-3.5 py-1.5 shrink-0"
    style="background:var(--hm-warning-bg); border-bottom:0.5px solid var(--hm-warning-border)">
    <span style="font-size:12px; color:var(--hm-warning)">
      ✦ {{ graph.pendingEdges.length }} connection {{ graph.pendingEdges.length === 1 ? 'suggestion' : 'suggestions' }} pending review
    </span>
    <div class="flex gap-1.5">
      <button class="hm-btn hm-btn-sm hm-btn-primary" @click="graph.acceptAllPending()">Accept all</button>
      <button class="hm-btn hm-btn-sm hm-btn-default" @click="graph.rejectAllPending()">Reject all</button>
    </div>
  </div>
</template>
