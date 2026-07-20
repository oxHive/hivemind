<script setup>
import { computed } from 'vue'
import { useUiStore } from '../../stores/ui.js'
import { useUpdateStore } from '../../stores/update.js'
const ui = useUiStore()
const update = useUpdateStore()

const rowsBeforeUpdate = computed(() => [
  ['Status', ui.serverStatus],
  ['Version', ui.serverInfo?.version || '—'],
])

const rowsAfterUpdate = computed(() => [
  ['Memories', ui.serverInfo?.memory_count ?? ui.serverInfo?.memoryCount ?? '—'],
  ['Storage', ui.serverInfo?.storage_path || ui.serverInfo?.storagePath || '—'],
  ['Agent', ui.serverInfo?.agent?.kind || '—'],
])

const updateText = computed(() => {
  if (update.status === 'updating') return 'Updating…'
  if (update.status === 'failed') return 'Update failed'
  if (update.available) return `v${update.latestVersion} available`
  return 'Up to date'
})
</script>

<template>
  <div>
    <p class="hm-label mb-4">SERVER</p>
    <div class="flex flex-col">
      <div v-for="[label, value] in rowsBeforeUpdate" :key="label" class="info-row">
        <span style="color:var(--hm-text-secondary)">{{ label }}</span>
        <span class="font-mono text-right" style="color:var(--hm-text-primary); overflow-wrap:anywhere">{{ value }}</span>
      </div>
      <div class="info-row">
        <span style="color:var(--hm-text-secondary)">Update</span>
        <span class="flex items-center justify-end gap-2">
          <span class="font-mono text-right"
            :style="{ color: update.status === 'failed' ? 'var(--hm-danger)' : (update.available ? 'var(--hm-warning)' : 'var(--hm-text-primary)') }">
            {{ updateText }}
          </span>
          <button v-if="update.available && update.platformSupported && update.status !== 'updating'"
            class="hm-btn hm-btn-default hm-btn-sm" @click="update.changelogOpen = true">View</button>
        </span>
      </div>
      <div v-for="[label, value] in rowsAfterUpdate" :key="label" class="info-row">
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
  font-size: 13px;
  border-bottom: 0.5px solid var(--hm-border-subtle);
}

.info-row:first-child {
  padding-top: 0;
}

.info-row:last-child {
  border-bottom: none;
}
</style>
