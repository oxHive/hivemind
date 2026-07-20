<script setup>
import { ref, computed, onMounted } from 'vue'
import { useUiStore } from '../../stores/ui.js'
import { getSyncSettings, saveSyncSettings } from '../../api/settings.js'

const ui = useUiStore()
const enabled = ref(false)
const remoteUrl = ref('')
const apiKey = ref('')
const intervalSeconds = ref(300)
const syncOnStore = ref(true)
const loading = ref(false)
const message = ref('')

onMounted(async () => {
  loading.value = true
  try {
    const s = await getSyncSettings()
    enabled.value = s.enabled ?? false
    remoteUrl.value = s.remote_url ?? ''
    apiKey.value = s.api_key ?? ''
    intervalSeconds.value = s.interval_seconds ?? 300
    syncOnStore.value = s.sync_on_store ?? true
  } catch {
    message.value = 'Could not load sync settings.'
  } finally {
    loading.value = false
  }
})

async function save() {
  const res = await saveSyncSettings({
    enabled: enabled.value, remote_url: remoteUrl.value,
    api_key: apiKey.value, interval_seconds: intervalSeconds.value,
    sync_on_store: syncOnStore.value,
  })
  message.value = res.message ?? 'Edit config.toml to persist sync settings.'
  ui.showToast(message.value)
}

const intervalLabel = computed(() => {
  const m = Math.round(intervalSeconds.value / 60)
  return m < 1 ? `${intervalSeconds.value}s` : `${m}m`
})
</script>

<template>
  <div>
    <p class="hm-label mb-4">SYNC</p>
    <p v-if="loading" style="font-size:13px; color:var(--hm-text-tertiary)">Loading…</p>
    <template v-else>
      <label class="flex items-center gap-3 mb-5 cursor-pointer">
        <input type="checkbox" v-model="enabled" class="w-4 h-4" />
        <span style="font-size:13px; color:var(--hm-text-primary)">Enable sync</span>
      </label>
      <template v-if="enabled">
        <label class="hm-label">REMOTE URL</label>
        <input class="hm-input mb-4" v-model="remoteUrl" placeholder="http://pi.local:3456" />
        <label class="hm-label">API KEY</label>
        <input class="hm-input mb-4" type="password" v-model="apiKey" placeholder="Leave blank if no auth" />
        <label class="hm-label">SYNC INTERVAL — {{ intervalLabel }}</label>
        <input class="hm-input mb-4" type="number" min="30" v-model.number="intervalSeconds" />
        <label class="flex items-center gap-3 mb-5 cursor-pointer">
          <input type="checkbox" v-model="syncOnStore" class="w-4 h-4" />
          <span style="font-size:13px; color:var(--hm-text-secondary)">Sync immediately after storing a memory</span>
        </label>
      </template>
      <button class="hm-btn hm-btn-primary" @click="save">Save sync settings</button>
      <p v-if="message" style="font-size:12px; color:var(--hm-text-tertiary)" class="mt-3">{{ message }}</p>
    </template>
  </div>
</template>
