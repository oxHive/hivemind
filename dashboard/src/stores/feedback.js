import { defineStore } from 'pinia'
import { ref } from 'vue'
import * as api from '../api/feedback.js'

export const useFeedbackStore = defineStore('feedback', () => {
  const conflicts = ref([])
  const feedbackItems = ref([])
  const activeTab = ref('conflicts')
  const loading = ref(false)

  async function fetchConflicts() {
    loading.value = true
    try { conflicts.value = await api.listConflicts() }
    catch { conflicts.value = [] }
    finally { loading.value = false }
  }

  async function fetchFeedback() {
    try { feedbackItems.value = await api.listFeedback() }
    catch { feedbackItems.value = [] }
  }

  async function resolveConflict(id, action) {
    await api.resolveConflict(id, action)
    conflicts.value = conflicts.value.filter(c => c.id !== id)
  }

  async function dismissFeedback(id) {
    await api.patchFeedback(id, { status: 'dismissed' })
    feedbackItems.value = feedbackItems.value.filter(f => f.id !== id)
  }

  return { conflicts, feedbackItems, activeTab, loading, fetchConflicts, fetchFeedback, resolveConflict, dismissFeedback }
})
