<script setup>
import { useGraphStore } from '../../stores/graph.js'
const props = defineProps({ edge: Object, x: Number, y: Number })
const graph = useGraphStore()
</script>

<template>
  <div class="absolute rounded-lg p-3"
    style="width:200px; background:var(--hm-bg-overlay); border:0.5px solid var(--hm-warning-border); z-index:11"
    :style="{ left: x + 'px', top: y + 'px' }">
    <p class="mb-2" style="font-size:11px; color:var(--hm-warning); font-family:monospace">{{ edge.relationship }}</p>
    <div class="flex gap-1.5">
      <button class="hm-btn hm-btn-primary hm-btn-sm" @click="graph.resolveEdge(edge.id,'accepted')">Accept</button>
      <button class="hm-btn hm-btn-default hm-btn-sm" @click="graph.resolveEdge(edge.id,'rejected')">Reject</button>
    </div>
  </div>
</template>
