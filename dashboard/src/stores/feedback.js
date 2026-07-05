import { defineStore } from 'pinia'
import { ref } from 'vue'
import * as api from '../api/feedback.js'

function mapConflict(c) {
  return { ...c, current: c.remote_content, local: c.local_content }
}

export const useFeedbackStore = defineStore('feedback', () => {
  const conflicts = ref([])
  const feedbackItems = ref([])
  const activeTab = ref('conflicts')
  const loading = ref(false)

  async function fetchConflicts() {
    loading.value = true
    try {
      const data = await api.listConflicts()
      conflicts.value = (data.conflicts ?? []).map(mapConflict)
    } catch {
      conflicts.value = []
    } finally {
      loading.value = false
    }
  }

  async function fetchFeedback() {
    try {
      const data = await api.listFeedback()
      feedbackItems.value = data.items ?? []
    } catch {
      feedbackItems.value = []
    }
  }

  async function resolveConflict(id, resolution) {
    await api.resolveConflict(id, resolution)
    conflicts.value = conflicts.value.filter(c => c.id !== id)
  }

  async function dismissFeedback(id) {
    await api.patchFeedback(id, { status: 'dismissed' })
    feedbackItems.value = feedbackItems.value.filter(f => f.id !== id)
  }

  return { conflicts, feedbackItems, activeTab, loading, fetchConflicts, fetchFeedback, resolveConflict, dismissFeedback }
})
