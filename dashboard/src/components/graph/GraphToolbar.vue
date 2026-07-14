<script setup>
import { useGraphStore } from '../../stores/graph.js'
import { useUiStore } from '../../stores/ui.js'
import { useMemoriesStore } from '../../stores/memories.js'
import FilterChip from '../shared/FilterChip.vue'

const graph = useGraphStore()
const ui = useUiStore()
const memories = useMemoriesStore()

function jumpToMatch() {
  const q = graph.searchQuery.trim().toLowerCase()
  if (!q) return
  const match = memories.all.find(m => m.title.toLowerCase().includes(q))
  if (match) graph.selectedNodeId = match.id
}
</script>

<template>
  <div class="flex items-center gap-3 px-4 py-2.5 shrink-0"
    style="border-bottom:0.5px solid var(--hm-border-subtle); background:var(--hm-bg-surface)">

    <input class="hm-input" style="width:180px" placeholder="Find node…"
      v-model="graph.searchQuery" @keyup.enter="jumpToMatch" />

    <div class="flex gap-1.5">
      <FilterChip
        v-for="f in [{label:'all',value:'all'},{label:'personal',value:'personal',layer:'personal'},{label:'workspace',value:'workspace',layer:'workspace'}]"
        :key="f.value" :label="f.label" :value="f.value"
        :active="graph.layerFilter === f.value" :layer="f.layer"
        @select="graph.layerFilter = $event"
      />
    </div>

    <div class="flex items-center gap-1 ml-auto">
      <span class="font-mono" style="font-size:10px; color:var(--hm-text-tertiary)">L{{ graph.zoom }}</span>
      <button class="hm-btn hm-btn-ghost hm-btn-sm" @click="graph.zoom = Math.max(1, graph.zoom - 1)">−</button>
      <button class="hm-btn hm-btn-ghost hm-btn-sm" @click="graph.zoom = Math.min(3, graph.zoom + 1)">+</button>
      <button class="hm-btn hm-btn-default hm-btn-sm ml-1 font-mono"
        @click="ui.copyToClipboard('/suggest-connections')">
        ✦ suggest
      </button>
    </div>
  </div>
</template>
