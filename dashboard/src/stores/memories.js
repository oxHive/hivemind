import { defineStore } from 'pinia'
import { ref, computed, watch } from 'vue'
import * as api from '../api/memories.js'

const DRAFTS_KEY = 'hivemind.memories.drafts'

function loadStashedDrafts() {
  try {
    const raw = localStorage.getItem(DRAFTS_KEY)
    return raw ? JSON.parse(raw) : {}
  } catch {
    return {}
  }
}

export const useMemoriesStore = defineStore('memories', () => {
  const all = ref([])
  const selected = ref(null)
  const draft = ref(null)
  // Unsaved edits per memory id, stashed when switching away so re-selecting
  // the same memory restores them instead of silently discarding — switching
  // views (e.g. to Settings) never touches this store, so drafts survive
  // there for free; switching memories used to always wipe the draft, which
  // read as an inconsistency between the two navigation paths. Persisted to
  // localStorage (loaded here, written by persistDrafts) so a page reload
  // doesn't silently discard unsaved edits either.
  const stashedDrafts = ref(loadStashedDrafts())
  // Set when the selected memory is changed elsewhere (e.g. by an agent via
  // MCP) while a dirty draft is open here — holds the incoming server
  // version so it isn't silently lost. `selected` is frozen at the version
  // the draft was diffing against until the user resolves it, so `dirty`/
  // Save keep comparing against a baseline that still makes sense.
  const conflict = ref(null)
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

  // Whether a memory has unsaved edits — either it's the currently selected
  // one and dirty, or it has a stash from being switched away from earlier.
  function isDraft(id) {
    if (selected.value?.id === id) return dirty.value
    return Object.prototype.hasOwnProperty.call(stashedDrafts.value, id)
  }

  function persistDrafts() {
    localStorage.setItem(DRAFTS_KEY, JSON.stringify(stashedDrafts.value))
  }

  // Keeps the currently-open draft in the persisted stash live, not just at
  // switch-away time, so a page reload while still editing (never having
  // switched to another memory) doesn't lose the edit either.
  watch(
    [draft, selected],
    () => {
      if (!selected.value) return
      if (dirty.value) {
        stashedDrafts.value[selected.value.id] = draft.value
      } else {
        delete stashedDrafts.value[selected.value.id]
      }
      persistDrafts()
    },
    { deep: true }
  )

  // Discards the current draft's unsaved edits, reverting to the last-saved
  // content, and clears any stash so switching away/back won't resurrect it.
  function resetDraft() {
    if (!selected.value) return
    draft.value = {
      title: selected.value.title,
      content: selected.value.content,
      tags: [...(selected.value.tags || [])],
    }
    delete stashedDrafts.value[selected.value.id]
    persistDrafts()
  }

  async function fetchAll() {
    loading.value = true
    try {
      const data = await api.listMemories(1000)
      all.value = data.memories ?? []
    } finally {
      loading.value = false
    }
  }

  // Re-fetches without the loading flag, and without disturbing the
  // currently open item's unsaved edits or the list's scroll position —
  // used to reflect memories changed elsewhere (e.g. via MCP) in the background.
  async function refreshSilently() {
    const data = await api.listMemories(1000)
    all.value = data.memories ?? []
    if (!selected.value) return
    const match = all.value.find(m => m.id === selected.value.id)

    if (!match) {
      // Memory was deleted elsewhere.
      selected.value = null
      draft.value = null
      conflict.value = null
      return
    }

    if (!dirty.value) {
      selected.value = match
      draft.value = { title: match.title, content: match.content, tags: [...(match.tags || [])] }
      conflict.value = null
      return
    }

    // Dirty: only flag a conflict if the server version actually changed
    // underneath us (not just an unrelated field/other memory refreshing).
    // Leave `selected`/`draft` untouched so the diff the user is looking at
    // stays coherent until they explicitly resolve it.
    const changed =
      match.title !== selected.value.title ||
      match.content !== selected.value.content ||
      JSON.stringify(match.tags || []) !== JSON.stringify(selected.value.tags || [])
    if (changed) {
      conflict.value = match
    }
  }

  // User chose to discard their local draft and adopt the version that
  // changed elsewhere while they were editing.
  function resolveConflictLoadLatest() {
    if (!conflict.value) return
    selected.value = conflict.value
    draft.value = {
      title: conflict.value.title,
      content: conflict.value.content,
      tags: [...(conflict.value.tags || [])],
    }
    delete stashedDrafts.value[selected.value.id]
    persistDrafts()
    conflict.value = null
  }

  // User chose to keep editing their own draft — the baseline moves forward
  // to the external version (so `dirty`/Save reflect it correctly), but the
  // draft's actual text is untouched. Saving after this will overwrite the
  // external change, which is now an informed, explicit choice.
  function resolveConflictKeepMine() {
    if (!conflict.value) return
    selected.value = conflict.value
    conflict.value = null
  }

  function select(entry) {
    if (selected.value && dirty.value) {
      stashedDrafts.value[selected.value.id] = draft.value
      persistDrafts()
    }
    conflict.value = null
    selected.value = entry
    if (!entry) {
      draft.value = null
      return
    }
    draft.value = stashedDrafts.value[entry.id]
      ?? { title: entry.title, content: entry.content, tags: [...(entry.tags || [])] }
  }

  async function save() {
    if (!selected.value || !dirty.value || conflict.value) return false
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
      delete stashedDrafts.value[updated.id]
      persistDrafts()
      conflict.value = null
      return true
    } finally {
      saving.value = false
    }
  }

  async function create({ title, content, tags, layer }) {
    const res = await api.createMemory({ title, content, tags, layer })
    await fetchAll()
    const created = all.value.find(m => m.id === res.id)
    if (created) select(created)
    return res.id
  }

  async function remove(id) {
    await api.deleteMemory(id)
    all.value = all.value.filter(m => m.id !== id)
    if (selected.value?.id === id) select(null)
    delete stashedDrafts.value[id]
    persistDrafts()
  }

  async function clearAll() {
    await api.deleteAllMemories()
    all.value = []
    stashedDrafts.value = {}
    persistDrafts()
    select(null)
  }

  return {
    all, selected, draft, conflict, searchQuery, layerFilter, loading, saving, filtered, dirty,
    isDraft, resetDraft, resolveConflictLoadLatest, resolveConflictKeepMine,
    fetchAll, refreshSilently, select, save, create, remove, clearAll,
  }
})
