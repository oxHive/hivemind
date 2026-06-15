<script setup>
import { ref } from 'vue'
import { useGraphStore } from '../../stores/graph.js'
import { useUiStore } from '../../stores/ui.js'
import FilterChip from '../shared/FilterChip.vue'

const graph = useGraphStore()
const ui = useUiStore()

const searchQuery = ref('')
const layerFilter = ref('all')
const emit = defineEmits(['filter-change', 'search-change'])
</script>

<template>
  <div class="flex items-center gap-2 px-3 py-2 shrink-0"
    style="border-bottom:0.5px solid var(--hm-border-subtle); background:var(--hm-bg-surface)">

    <input class="hm-input" style="width:180px" placeholder="Find node…"
      v-model="searchQuery" @input="emit('search-change', searchQuery)" />

    <div class="flex gap-1">
      <FilterChip
        v-for="f in [{label:'all',value:'all'},{label:'personal',value:'personal',layer:'personal'},{label:'workspace',value:'workspace',layer:'workspace'}]"
        :key="f.value" :label="f.label" :value="f.value"
        :active="layerFilter === f.value" :layer="f.layer"
        @select="layerFilter = $event; emit('filter-change', $event)"
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
