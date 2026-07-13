import { defineStore } from 'pinia'
import { ref } from 'vue'
import { getTagSettings, saveTagSettings } from '../api/settings.js'

export const useTagSettingsStore = defineStore('tagSettings', () => {
  const namespaces = ref({})
  const loaded = ref(false)

  async function fetchNamespaces() {
    namespaces.value = await getTagSettings()
    loaded.value = true
  }

  async function save() {
    await saveTagSettings(namespaces.value)
  }

  function namespaceFor(tag) {
    const idx = tag.indexOf(':')
    if (idx === -1) return null
    const ns = tag.slice(0, idx)
    return namespaces.value[ns] ? ns : null
  }

  function colorFor(tag) {
    const ns = namespaceFor(tag)
    return ns ? namespaces.value[ns].color : null
  }

  return { namespaces, loaded, fetchNamespaces, save, namespaceFor, colorFor }
})
