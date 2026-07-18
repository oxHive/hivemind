<script setup>
import { ref, onMounted, onBeforeUnmount } from 'vue'
import { useMemoriesStore } from '../../stores/memories.js'
import MemoryCard from './MemoryCard.vue'
import FilterChip from '../shared/FilterChip.vue'
import SkeletonCard from '../shared/SkeletonCard.vue'

const memories = useMemoriesStore()
const searchEl = ref(null)

function handleSlash(e) {
  if (e.key !== '/' || e.ctrlKey || e.metaKey || e.altKey) return
  const tag = document.activeElement?.tagName
  if (tag === 'INPUT' || tag === 'TEXTAREA') return
  e.preventDefault()
  searchEl.value?.focus()
}

onMounted(() => window.addEventListener('keydown', handleSlash))
onBeforeUnmount(() => window.removeEventListener('keydown', handleSlash))

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
    <div class="px-4 pt-4 pb-3" style="border-bottom:0.5px solid var(--hm-border-subtle)">
      <button class="hm-btn hm-btn-primary hm-btn-sm mb-3 w-full justify-center gap-1.5"
        @click="memories.startNew()">
        + New memory
        <span v-if="memories.hasNewDraft" class="font-mono rounded-sm px-1"
          style="font-size:9px; background:var(--hm-warning-bg); color:var(--hm-warning)">DRAFT</span>
      </button>
      <div class="relative mb-3">
        <span class="absolute left-2.5 top-1/2 -translate-y-1/2" style="color:var(--hm-text-tertiary); font-size:13px">⌕</span>
        <input
          ref="searchEl"
          class="hm-input pl-7"
          placeholder="Search memories…  ( / )"
          :value="memories.searchQuery"
          @input="memories.searchQuery = $event.target.value"
        />
      </div>
      <div class="flex gap-1.5">
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
    <div class="px-4 flex items-center" style="height:40px; border-top:0.5px solid var(--hm-border-subtle)">
      <span class="font-mono" style="font-size:11px; color:var(--hm-text-tertiary)">
        <template v-if="memories.searchQuery || memories.layerFilter !== 'all'">
          {{ memories.filtered.length }} of {{ memories.all.length }} memories
        </template>
        <template v-else>{{ memories.all.length }} memories</template>
      </span>
    </div>
  </div>
</template>
