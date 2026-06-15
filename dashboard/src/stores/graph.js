import { defineStore } from 'pinia'
import { ref, computed } from 'vue'
import * as api from '../api/edges.js'

export const useGraphStore = defineStore('graph', () => {
  const edges = ref([])
  const zoom = ref(2)            // 1|2|3
  const selectedNodeId = ref(null)
  const connectMode = ref(false)
  const connectSourceId = ref(null)
  const pendingConnect = ref(null) // { sourceId, targetId } when picker is open

  const pendingEdges = computed(() => edges.value.filter(e => e.status === 'pending'))

  function edgesFor(memoryId) {
    return edges.value.filter(e =>
      (e.source_id === memoryId || e.target_id === memoryId) &&
      e.status === 'accepted'
    )
  }

  async function fetchEdges() {
    edges.value = await api.listEdges()
  }

  async function storeEdge(sourceId, targetId, relationship) {
    const edge = await api.createEdge({ source_id: sourceId, target_id: targetId, relationship, status: 'accepted' })
    edges.value.push(edge)
  }

  async function resolveEdge(id, status) {
    const updated = await api.patchEdge(id, { status })
    const idx = edges.value.findIndex(e => e.id === id)
    if (idx !== -1) edges.value[idx] = updated
  }

  async function acceptAllPending() {
    await Promise.all(pendingEdges.value.map(e => resolveEdge(e.id, 'accepted')))
  }

  async function rejectAllPending() {
    await Promise.all(pendingEdges.value.map(e => resolveEdge(e.id, 'rejected')))
  }

  function startConnect(sourceId) {
    connectMode.value = true
    connectSourceId.value = sourceId
  }

  function cancelConnect() {
    connectMode.value = false
    connectSourceId.value = null
    pendingConnect.value = null
  }

  return {
    edges, zoom, selectedNodeId, connectMode, connectSourceId, pendingConnect,
    pendingEdges, edgesFor, fetchEdges, storeEdge, resolveEdge,
    acceptAllPending, rejectAllPending, startConnect, cancelConnect,
  }
})
