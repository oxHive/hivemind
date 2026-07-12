<script setup>
import { ref } from 'vue'
import { useUiStore } from '../../stores/ui.js'
import { exportMemories, importMemories } from '../../api/memories.js'

const ui = useUiStore()
const fileInput = ref(null)

async function handleExport() {
  try {
    const data = await exportMemories()
    const blob = new Blob([JSON.stringify(data, null, 2)], { type: 'application/json' })
    const url = URL.createObjectURL(blob)
    const a = document.createElement('a')
    a.href = url
    a.download = `hivemind-export-${new Date().toISOString().slice(0,10)}.json`
    a.click()
    URL.revokeObjectURL(url)
  } catch {
    ui.showToast('Export failed')
  }
}

async function handleImport(e) {
  const file = e.target.files[0]
  if (!file) return
  try {
    const text = await file.text()
    const result = await importMemories(JSON.parse(text))
    ui.showToast(`Import complete — ${result.imported ?? '?'} memories imported`)
  } catch {
    ui.showToast('Import failed — invalid file format')
  }
  fileInput.value.value = ''
}
</script>

<template>
  <div>
    <p class="hm-label mb-4">DATA</p>
    <div class="flex flex-col gap-5">
      <div>
        <p style="font-size:12px; font-weight:500; color:var(--hm-text-primary)" class="mb-1.5">Export to JSON</p>
        <p style="font-size:11px; color:var(--hm-text-tertiary)" class="mb-3">Download all memories as a JSON file</p>
        <button class="hm-btn hm-btn-default" @click="handleExport">Export</button>
      </div>
      <div>
        <p style="font-size:12px; font-weight:500; color:var(--hm-text-primary)" class="mb-1.5">Import from JSON</p>
        <p style="font-size:11px; color:var(--hm-text-tertiary)" class="mb-3">Restore from a previous HiveMind export</p>
        <button class="hm-btn hm-btn-default" @click="fileInput.click()">Import</button>
        <input ref="fileInput" type="file" accept=".json" class="hidden" @change="handleImport" />
      </div>
    </div>
  </div>
</template>
