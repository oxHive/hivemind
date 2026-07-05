<script setup>
import { useMemoriesStore } from '../../stores/memories.js'
import MemoryCard from './MemoryCard.vue'
import FilterChip from '../shared/FilterChip.vue'
import SkeletonCard from '../shared/SkeletonCard.vue'

const memories = useMemoriesStore()

const filters = [
  { label: 'all', value: 'all' },
  { label: 'personal', value: 'personal', layer: 'personal' },
  { label: 'workspace', value: 'workspace', layer: 'workspace' },
]
</script>

<template>
  <div class="flex flex-col h-full shrink-0"
    style="width:clamp(240px, 26vw, 320px); border-right:0.5px solid var(--hm-border-subtle)">

    <!-- Header -->
    <div class="px-3 pt-3 pb-2" style="border-bottom:0.5px solid var(--hm-border-subtle)">
      <div class="relative mb-2">
        <span class="absolute left-2.5 top-1/2 -translate-y-1/2" style="color:var(--hm-text-tertiary); font-size:13px">⌕</span>
        <input
          class="hm-input pl-7"
          placeholder="Search memories…"
          :value="memories.searchQuery"
          @input="memories.searchQuery = $event.target.value"
        />
      </div>
      <div class="flex gap-1">
        <FilterChip
          v-for="f in filters" :key="f.value"
          :label="f.label" :value="f.value"
          :active="memories.layerFilter === f.value"
          :layer="f.layer"
          @select="memories.layerFilter = $event"
        />
      </div>
    </div>

    <!-- List -->
    <div class="flex-1 overflow-y-auto">
      <template v-if="memories.loading">
        <SkeletonCard v-for="i in 5" :key="i" />
      </template>
      <template v-else>
        <MemoryCard
          v-for="mem in memories.filtered"
          :key="mem.id"
          :mem="mem"
          :selected="memories.selected?.id === mem.id"
          @select="memories.select($event)"
        />
        <div v-if="!memories.filtered.length" class="p-6 text-center"
          style="font-size:12px; color:var(--hm-text-secondary)">
          No memories match your filter.
        </div>
      </template>
    </div>

    <!-- Footer -->
    <div class="px-3.5 py-2" style="border-top:0.5px solid var(--hm-border-subtle)">
      <span class="font-mono" style="font-size:11px; color:var(--hm-text-tertiary)">
        <template v-if="memories.searchQuery || memories.layerFilter !== 'all'">
          {{ memories.filtered.length }} of {{ memories.all.length }} memories
        </template>
        <template v-else>{{ memories.all.length }} memories</template>
      </span>
    </div>
  </div>
</template>
