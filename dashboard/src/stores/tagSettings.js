import { defineStore } from 'pinia'
import { ref, computed } from 'vue'
import { getTagSettings, saveTagSettings } from '../api/settings.js'

export const useTagSettingsStore = defineStore('tagSettings', () => {
  const namespaces = ref({})
  const loaded = ref(false)
  // Snapshot of `namespaces` as last fetched/saved, used to detect unsaved
  // edits (compared by value, not reference, since namespaces is mutated
  // in place by the settings UI).
  const savedSnapshot = ref('{}')

  const isDirty = computed(() => JSON.stringify(namespaces.value) !== savedSnapshot.value)

  async function fetchNamespaces() {
    namespaces.value = await getTagSettings()
    savedSnapshot.value = JSON.stringify(namespaces.value)
    loaded.value = true
  }

  async function save() {
    await saveTagSettings(namespaces.value)
    savedSnapshot.value = JSON.stringify(namespaces.value)
  }

  function namespaceFor(tag) {
    const idx = tag.indexOf(':')
    if (idx === -1) return null
    const ns = tag.slice(0, idx)
    return namespaces.value[ns] ? ns : null
  }

  const DEFAULT_TAG_COLOR = '#8a8f98'

  function colorFor(tag) {
    const ns = namespaceFor(tag)
    return ns ? namespaces.value[ns].color : DEFAULT_TAG_COLOR
  }

  return { namespaces, loaded, isDirty, fetchNamespaces, save, namespaceFor, colorFor }
})
