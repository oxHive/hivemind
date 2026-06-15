<script setup>
import { useUiStore } from '../../stores/ui.js'
const ui = useUiStore()
</script>

<template>
  <div>
    <p class="hm-label mb-3">SERVER</p>
    <div class="flex flex-col gap-2">
      <div class="flex justify-between py-1.5"
        style="border-bottom:0.5px solid var(--hm-border-subtle); font-size:12px">
        <span style="color:var(--hm-text-secondary)">Status</span>
        <span class="font-mono" style="color:var(--hm-text-primary)">{{ ui.serverStatus }}</span>
      </div>
      <div class="flex justify-between py-1.5"
        style="border-bottom:0.5px solid var(--hm-border-subtle); font-size:12px">
        <span style="color:var(--hm-text-secondary)">Version</span>
        <span class="font-mono" style="color:var(--hm-text-primary)">{{ ui.serverInfo?.version || '—' }}</span>
      </div>
      <div class="flex justify-between py-1.5"
        style="border-bottom:0.5px solid var(--hm-border-subtle); font-size:12px">
        <span style="color:var(--hm-text-secondary)">Memories</span>
        <span class="font-mono" style="color:var(--hm-text-primary)">{{ ui.serverInfo?.memory_count ?? ui.serverInfo?.memoryCount ?? '—' }}</span>
      </div>
      <div class="flex justify-between py-1.5"
        style="border-bottom:0.5px solid var(--hm-border-subtle); font-size:12px">
        <span style="color:var(--hm-text-secondary)">Storage</span>
        <span class="font-mono" style="color:var(--hm-text-primary)">{{ ui.serverInfo?.storage_path || ui.serverInfo?.storagePath || '—' }}</span>
      </div>
    </div>
  </div>
</template>
