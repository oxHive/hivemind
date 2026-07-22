import { defineStore } from 'pinia'
import { ref, computed } from 'vue'
import { getContentLimitSettings, saveContentLimitSettings } from '../api/settings.js'

export const useContentLimitSettingsStore = defineStore('contentLimitSettings', () => {
  const maxContentTokens = ref(1500)
  const loaded = ref(false)
  // Snapshot of the last fetched/saved value, used to detect unsaved edits.
  const savedSnapshot = ref(1500)

  const isDirty = computed(() => maxContentTokens.value !== savedSnapshot.value)

  async function fetch() {
    const res = await getContentLimitSettings()
    maxContentTokens.value = res.max_content_tokens
    savedSnapshot.value = res.max_content_tokens
    loaded.value = true
  }

  async function save() {
    await saveContentLimitSettings({ max_content_tokens: maxContentTokens.value })
    savedSnapshot.value = maxContentTokens.value
  }

  return { maxContentTokens, loaded, isDirty, fetch, save }
})
