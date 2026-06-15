<script setup>
import LayerBadge from '../shared/LayerBadge.vue'
defineProps({ node: Object, x: Number, y: Number })
</script>

<template>
  <div class="absolute pointer-events-none rounded-lg p-3"
    style="width:180px; background:var(--hm-bg-overlay); border:0.5px solid var(--hm-border-default); z-index:10"
    :style="{ left: x + 'px', top: y + 'px' }">
    <div class="flex items-start justify-between gap-2 mb-1">
      <span style="font-size:12px; font-weight:500; color:var(--hm-text-primary)">{{ node.title }}</span>
      <LayerBadge :layer="node.layer" />
    </div>
    <div class="flex flex-wrap gap-1 mt-1">
      <span v-for="tag in (node.tags||[]).slice(0,3)" :key="tag"
        class="font-mono rounded-sm px-1 py-0.5"
        style="font-size:9px; background:var(--hm-bg-elevated); color:var(--hm-text-tertiary)">{{ tag }}</span>
    </div>
  </div>
</template>
