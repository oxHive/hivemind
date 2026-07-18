import { defineStore } from 'pinia'
import { ref, computed } from 'vue'
import * as api from '../api/edges.js'
import { useMemoriesStore } from './memories.js'
import { withMention } from '../lib/mention.js'

export const useGraphStore = defineStore('graph', () => {
  const edges = ref([])
  const zoom = ref(2)            // 1|2|3
  const selectedNodeId = ref(null)
  const selectedEdgeId = ref(null)
  const searchQuery = ref('')
  const layerFilter = ref('all') // 'all' | 'personal' | 'workspace'

  const pendingEdges = computed(() => edges.value.filter(e => e.status === 'pending'))
  const selectedEdge = computed(() => edges.value.find(e => e.id === selectedEdgeId.value) || null)

  function edgesFor(memoryId) {
    return edges.value.filter(e =>
      (e.source_id === memoryId || e.target_id === memoryId) &&
      e.status === 'active'
    )
  }

  async function fetchEdges() {
    const data = await api.listEdges()
    edges.value = data.edges ?? []
  }

  // Approving a suggested (pending) connection also embeds it as a mention
  // link in the source memory's content — the dashboard is what applies the
  // edit the agent only proposed, keeping the review step meaningful.
  async function resolveEdge(id, status) {
    const before = edges.value.find(e => e.id === id)
    await api.patchEdge(id, { status })
    const idx = edges.value.findIndex(e => e.id === id)
    if (idx !== -1) edges.value[idx] = { ...edges.value[idx], status }

    if (status === 'active' && before?.status === 'pending' && before.link_text) {
      const memories = useMemoriesStore()
      const source = memories.all.find(m => m.id === before.source_id)
      if (source) {
        const nextContent = withMention(source.content, before)
        if (nextContent !== source.content) {
          await memories.patchContent(before.source_id, nextContent)
        }
      }
    }
  }

  async function acceptAllPending() {
    await Promise.all(pendingEdges.value.map(e => resolveEdge(e.id, 'active')))
  }

  async function rejectAllPending() {
    await Promise.all(pendingEdges.value.map(e => resolveEdge(e.id, 'rejected')))
  }

  return {
    edges, zoom, selectedNodeId, selectedEdgeId, searchQuery, layerFilter,
    pendingEdges, selectedEdge, edgesFor, fetchEdges, resolveEdge,
    acceptAllPending, rejectAllPending,
  }
})
