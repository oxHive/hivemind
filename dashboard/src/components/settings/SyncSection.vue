<script setup>
import { ref, onMounted } from 'vue'
import { useUiStore } from '../../stores/ui.js'
import { getSyncSettings, saveSyncSettings } from '../../api/settings.js'

const ui = useUiStore()
const enabled = ref(false)
const remote = ref('')
const branch = ref('main')
const saving = ref(false)

onMounted(async () => {
  try {
    const s = await getSyncSettings()
    enabled.value = s.enabled ?? false
    remote.value = s.remote ?? ''
    branch.value = s.branch ?? 'main'
  } catch { /* server may not have sync settings yet */ }
})

async function save() {
  saving.value = true
  try {
    await saveSyncSettings({ enabled: enabled.value, remote: remote.value, branch: branch.value })
    ui.showToast('Sync settings saved')
  } catch {
    ui.showToast('Failed to save sync settings')
  } finally {
    saving.value = false
  }
}
</script>

<template>
  <div>
    <p class="hm-label mb-3">SYNC</p>
    <label class="flex items-center gap-3 mb-4 cursor-pointer">
      <input type="checkbox" v-model="enabled" class="w-4 h-4" />
      <span style="font-size:12px; color:var(--hm-text-primary)">Enable sync</span>
    </label>
    <template v-if="enabled">
      <label class="hm-label">REMOTE URL</label>
      <input class="hm-input mb-3" v-model="remote" placeholder="git@github.com:user/memories.git" />
      <label class="hm-label">BRANCH</label>
      <input class="hm-input mb-4" v-model="branch" />
    </template>
    <button class="hm-btn hm-btn-primary" :disabled="saving" @click="save">
      {{ saving ? 'Saving…' : 'Save sync settings' }}
    </button>
  </div>
</template>
