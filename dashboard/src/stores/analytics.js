import { defineStore } from 'pinia'
import { ref, computed } from 'vue'
import { useMemoriesStore } from './memories.js'
import * as api from '../api/sessionLogs.js'

export const useAnalyticsStore = defineStore('analytics', () => {
  const memories = useMemoriesStore()
  const sessionLogs = ref([])
  const loadingLogs = ref(false)

  async function fetchSessionLogs() {
    loadingLogs.value = true
    try {
      const data = await api.getSessionLogs(50)
      sessionLogs.value = data.logs ?? []
    } finally {
      loadingLogs.value = false
    }
  }

  function projectTagOf(tags) {
    const t = (tags || []).find(t => t.toLowerCase().startsWith('project:'))
    return t ? t.slice('project:'.length) : null
  }

  const tagCounts = computed(() => {
    const counts = {}
    for (const m of memories.all) {
      for (const t of (m.tags || [])) counts[t] = (counts[t] || 0) + 1
    }
    return Object.entries(counts)
      .map(([tag, count]) => ({ tag, count }))
      .sort((a, b) => b.count - a.count)
  })

  const typeCounts = computed(() => {
    const counts = {}
    for (const m of memories.all) {
      const t = m.memory_type || 'unknown'
      counts[t] = (counts[t] || 0) + 1
    }
    return Object.entries(counts).map(([type, count]) => ({ type, count }))
  })

  const projectCounts = computed(() => {
    const counts = {}
    for (const m of memories.all) {
      const p = projectTagOf(m.tags)
      if (!p) continue
      counts[p] = (counts[p] || 0) + 1
    }
    return Object.entries(counts)
      .map(([project, count]) => ({ project, count }))
      .sort((a, b) => b.count - a.count)
  })

  const activityByDay = computed(() => {
    const counts = {}
    for (const m of memories.all) {
      const day = new Date(m.created_at * 1000).toISOString().slice(0, 10)
      counts[day] = (counts[day] || 0) + 1
    }
    const cutoff = new Date()
    cutoff.setDate(cutoff.getDate() - 90)
    const cutoffDay = cutoff.toISOString().slice(0, 10)
    return Object.entries(counts)
      .map(([day, count]) => ({ day, count }))
      .filter(d => d.day >= cutoffDay)
      .sort((a, b) => a.day.localeCompare(b.day))
  })

  return { sessionLogs, loadingLogs, fetchSessionLogs, tagCounts, typeCounts, projectCounts, activityByDay }
})
