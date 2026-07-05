import { defineStore } from 'pinia'
import { ref, computed } from 'vue'
import * as api from '../api/memories.js'

export const useMemoriesStore = defineStore('memories', () => {
  const all = ref([])
  const selected = ref(null)
  const draft = ref(null)
  const searchQuery = ref('')
  const layerFilter = ref('all')
  const loading = ref(false)
  const saving = ref(false)

  const filtered = computed(() => {
    let list = all.value
    if (searchQuery.value.trim()) {
      const q = searchQuery.value.toLowerCase()
      list = list.filter(m =>
        m.title.toLowerCase().includes(q) ||
        (m.content || '').toLowerCase().includes(q) ||
        (m.tags || []).some(t => t.toLowerCase().includes(q))
      )
    }
    if (layerFilter.value !== 'all') {
      list = list.filter(m => m.layer === layerFilter.value)
    }
    return list
  })

  const dirty = computed(() => {
    if (!selected.value || !draft.value) return false
    return (
      draft.value.title !== selected.value.title ||
      draft.value.content !== selected.value.content ||
      JSON.stringify(draft.value.tags) !== JSON.stringify(selected.value.tags || [])
    )
  })

  async function fetchAll() {
    loading.value = true
    try {
      const data = await api.listMemories()
      all.value = data.memories ?? []
    } finally {
      loading.value = false
    }
  }

  function select(entry) {
    selected.value = entry
    draft.value = entry
      ? { title: entry.title, content: entry.content, tags: [...(entry.tags || [])] }
      : null
  }

  async function save() {
    if (!selected.value || !dirty.value) return
    saving.value = true
    try {
      const updated = await api.patchMemory(selected.value.id, {
        title: draft.value.title,
        content: draft.value.content,
        tags: draft.value.tags,
      })
      const idx = all.value.findIndex(m => m.id === selected.value.id)
      if (idx !== -1) all.value[idx] = updated
      selected.value = updated
      draft.value = { title: updated.title, content: updated.content, tags: [...(updated.tags || [])] }
    } finally {
      saving.value = false
    }
  }

  async function remove(id) {
    await api.deleteMemory(id)
    all.value = all.value.filter(m => m.id !== id)
    if (selected.value?.id === id) select(null)
  }

  async function clearAll() {
    await api.deleteAllMemories()
    all.value = []
    select(null)
  }

  return { all, selected, draft, searchQuery, layerFilter, loading, saving, filtered, dirty, fetchAll, select, save, remove, clearAll }
})
