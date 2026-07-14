<script setup>
import { computed } from 'vue'
import { useMemoriesStore } from '../stores/memories.js'
import { useAnalyticsStore } from '../stores/analytics.js'

const memories = useMemoriesStore()
const analytics = useAnalyticsStore()

const totalMemories = computed(() => memories.all.length)
const totalTags = computed(() => analytics.tagCounts.length)
const totalProjects = computed(() => analytics.projectCounts.length)
const addedThisWeek = computed(() => {
  const weekAgo = Date.now() / 1000 - 7 * 24 * 60 * 60
  return memories.all.filter(m => m.created_at >= weekAgo).length
})
</script>

<template>
  <div class="flex-1 overflow-y-auto px-8 py-8">
    <h2 class="mb-8 font-medium" style="font-size:16px; color:var(--hm-text-primary)">Analytics</h2>

    <div class="grid grid-cols-4 gap-4 mb-10">
      <div class="hm-card px-5 py-4" style="border:0.5px solid var(--hm-border-subtle); border-radius:8px">
        <div style="font-size:11px; color:var(--hm-text-tertiary)">Total memories</div>
        <div style="font-size:22px; font-weight:600; color:var(--hm-text-primary)">{{ totalMemories }}</div>
      </div>
      <div class="hm-card px-5 py-4" style="border:0.5px solid var(--hm-border-subtle); border-radius:8px">
        <div style="font-size:11px; color:var(--hm-text-tertiary)">Distinct tags</div>
        <div style="font-size:22px; font-weight:600; color:var(--hm-text-primary)">{{ totalTags }}</div>
      </div>
      <div class="hm-card px-5 py-4" style="border:0.5px solid var(--hm-border-subtle); border-radius:8px">
        <div style="font-size:11px; color:var(--hm-text-tertiary)">Projects</div>
        <div style="font-size:22px; font-weight:600; color:var(--hm-text-primary)">{{ totalProjects }}</div>
      </div>
      <div class="hm-card px-5 py-4" style="border:0.5px solid var(--hm-border-subtle); border-radius:8px">
        <div style="font-size:11px; color:var(--hm-text-tertiary)">Added last 7 days</div>
        <div style="font-size:22px; font-weight:600; color:var(--hm-text-primary)">{{ addedThisWeek }}</div>
      </div>
    </div>
  </div>
</template>
