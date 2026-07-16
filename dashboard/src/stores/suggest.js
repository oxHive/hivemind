import { defineStore } from 'pinia'
import { ref } from 'vue'
import * as api from '../api/suggest.js'

export const useSuggestStore = defineStore('suggest', () => {
  const active = ref(false)
  const phase = ref('idle') // 'idle' | 'suggesting' | 'reviewing' | 'revising'
  const revisingEdgeId = ref(null)
  const queuedEdgeIds = ref([])
  const error = ref(null)
  const panelOpen = ref(false)

  async function start() {
    error.value = null
    try {
      await api.startSession()
      active.value = true
      phase.value = 'suggesting'
      panelOpen.value = true
    } catch (e) {
      error.value = e.status === 409 ? 'A suggest session is already running.' : String(e.message)
    }
  }

  async function revise(edgeId, feedback) {
    error.value = null
    try {
      await api.reviseSession(edgeId, feedback)
      if (!queuedEdgeIds.value.includes(edgeId)) queuedEdgeIds.value.push(edgeId)
    } catch (e) {
      error.value = String(e.message)
    }
  }

  async function end() {
    try { await api.endSession() } catch { /* session state resets via SSE or below */ }
    active.value = false
    phase.value = 'idle'
    revisingEdgeId.value = null
    queuedEdgeIds.value = []
  }

  async function hydrate() {
    try {
      const s = await api.getSession()
      active.value = !!s.active
      phase.value = s.phase ?? 'idle'
      revisingEdgeId.value = s.revising_edge_id ?? null
      queuedEdgeIds.value = s.queued_edge_ids ?? []
      if (active.value) panelOpen.value = true
    } catch { /* server without the feature; leave idle */ }
  }

  function handleEvent(data) {
    switch (data.state) {
      case 'started':
        active.value = true; phase.value = 'suggesting'; panelOpen.value = true; break
      case 'suggestions_ready':
        phase.value = 'reviewing'; break
      case 'revising':
        phase.value = 'revising'
        revisingEdgeId.value = data.edge_id ?? null
        queuedEdgeIds.value = data.queued ?? []
        break
      case 'revision_ready':
        phase.value = 'reviewing'; revisingEdgeId.value = null; break
      case 'error':
        phase.value = active.value ? 'reviewing' : 'idle'
        revisingEdgeId.value = null
        error.value = data.message ?? 'agent error'
        break
      case 'ended':
        active.value = false; phase.value = 'idle'
        revisingEdgeId.value = null; queuedEdgeIds.value = []
        break
    }
  }

  function openPanel() { panelOpen.value = true }
  function closePanel() { panelOpen.value = false }

  return {
    active, phase, revisingEdgeId, queuedEdgeIds, error, panelOpen,
    start, revise, end, hydrate, handleEvent, openPanel, closePanel,
  }
})
