import { defineStore } from 'pinia'
import { ref } from 'vue'
import { getStatus } from '../api/memories.js'

export const useUiStore = defineStore('ui', () => {
  const activeView = ref('analytics')
  const serverStatus = ref('checking') // 'checking'|'running'|'unreachable'|'syncing'|'sync_failed'
  const serverInfo = ref(null)
  const syncInfo = ref(null)
  const toast = ref({ message: '', visible: false })

  let toastTimer = null

  function showToast(message, duration = 2200) {
    if (toastTimer) clearTimeout(toastTimer)
    toast.value = { message, visible: true }
    toastTimer = setTimeout(() => { toast.value.visible = false }, duration)
  }

  async function copyToClipboard(text) {
    await navigator.clipboard.writeText(text)
    showToast('Copied: ' + text)
  }

  async function pollServerStatus() {
    try {
      const data = await getStatus()
      serverStatus.value = 'running'
      serverInfo.value = data.info ?? data
      syncInfo.value = data.sync ?? null
    } catch {
      serverStatus.value = 'unreachable'
    }
  }

  return { activeView, serverStatus, serverInfo, syncInfo, toast, showToast, copyToClipboard, pollServerStatus }
})
