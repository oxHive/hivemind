<script setup>
import { computed } from 'vue'
import { useUiStore } from '../../stores/ui.js'
const ui = useUiStore()

const rows = computed(() => [
  ['Status', ui.serverStatus],
  ['Version', ui.serverInfo?.version || '—'],
  ['Memories', ui.serverInfo?.memory_count ?? ui.serverInfo?.memoryCount ?? '—'],
  ['Storage', ui.serverInfo?.storage_path || ui.serverInfo?.storagePath || '—'],
])
</script>

<template>
  <div>
    <p class="hm-label mb-4">SERVER</p>
    <div class="flex flex-col">
      <div v-for="[label, value] in rows" :key="label" class="info-row">
        <span style="color:var(--hm-text-secondary)">{{ label }}</span>
        <span class="font-mono text-right" style="color:var(--hm-text-primary); overflow-wrap:anywhere">{{ value }}</span>
      </div>
    </div>
  </div>
</template>

<style scoped>
.info-row {
  display: flex;
  justify-content: space-between;
  gap: 24px;
  padding: 10px 0;
  font-size: 12px;
  border-bottom: 0.5px solid var(--hm-border-subtle);
}

.info-row:first-child {
  padding-top: 0;
}

.info-row:last-child {
  border-bottom: none;
}
</style>
