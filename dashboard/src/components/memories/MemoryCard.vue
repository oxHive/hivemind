<script setup>
import LayerBadge from '../shared/LayerBadge.vue'
import TagChip from '../shared/TagChip.vue'
import { fmtDate } from '../../lib/format.js'

const props = defineProps({ mem: Object, selected: Boolean })
defineEmits(['select'])
</script>

<template>
  <div
    @click="$emit('select', mem)"
    tabindex="0"
    role="button"
    class="memory-card"
    :class="{
      'memory-card--selected-personal': selected && mem.layer === 'personal',
      'memory-card--selected-workspace': selected && mem.layer !== 'personal',
    }"
    @keydown.enter.space.prevent="$emit('select', mem)"
  >
    <!-- Row 1: title + layer badge -->
    <div class="flex items-start justify-between gap-2 mb-1">
      <span class="font-medium leading-snug"
        style="font-size:13px; color:var(--hm-text-primary); overflow:hidden; display:-webkit-box; -webkit-line-clamp:1; -webkit-box-orient:vertical">
        {{ mem.title }}
      </span>
      <LayerBadge :layer="mem.layer" class="shrink-0 mt-0.5" />
    </div>
    <!-- Row 2: snippet -->
    <p class="mb-1.5"
      style="font-size:12px; color:var(--hm-text-secondary); overflow:hidden; display:-webkit-box; -webkit-line-clamp:1; -webkit-box-orient:vertical">
      {{ mem.content }}
    </p>
    <!-- Row 3: tags + date -->
    <div class="flex items-center gap-1 justify-between">
      <div class="flex items-center gap-1 overflow-hidden">
        <TagChip v-for="tag in (mem.tags || []).slice(0,3)" :key="tag" :tag="tag" />
      </div>
      <span class="font-mono shrink-0" style="font-size:11px; color:var(--hm-text-tertiary)">
        {{ fmtDate(mem.updated_at || mem.created_at) }}
      </span>
    </div>
  </div>
</template>

<style scoped>
.memory-card {
  padding: 12px 14px;
  cursor: pointer;
  border-bottom: 0.5px solid var(--hm-border-subtle);
  background: transparent;
  transition: background 0.1s;
}

.memory-card:hover,
.memory-card:focus-visible {
  background: var(--hm-bg-elevated);
  outline: none;
}

.memory-card:focus-visible {
  outline: 2px solid var(--hm-personal);
  outline-offset: -2px;
}

.memory-card--selected-personal {
  background: var(--hm-personal-bg);
}

.memory-card--selected-workspace {
  background: var(--hm-workspace-bg);
}

.memory-card--selected-personal:hover,
.memory-card--selected-personal:focus-visible {
  background: var(--hm-personal-bg);
}

.memory-card--selected-workspace:hover,
.memory-card--selected-workspace:focus-visible {
  background: var(--hm-workspace-bg);
}
</style>
