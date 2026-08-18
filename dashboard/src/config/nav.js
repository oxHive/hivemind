import { computed } from 'vue'
import { useUiStore } from '../stores/ui.js'
import { useFeedbackStore } from '../stores/feedback.js'
import { useGraphStore } from '../stores/graph.js'
import IconAnalytics from '../components/icons/IconAnalytics.vue'
import IconMemories from '../components/icons/IconMemories.vue'
import IconGraph from '../components/icons/IconGraph.vue'
import IconFeedback from '../components/icons/IconFeedback.vue'
import IconSettings from '../components/icons/IconSettings.vue'

export function useNavItems() {
  const ui = useUiStore()
  const feedback = useFeedbackStore()
  const graph = useGraphStore()

  const feedbackCount = computed(() => feedback.conflicts.length + feedback.feedbackItems.length)

  const defs = [
    { id: 'analytics', label: 'Analytics', icon: IconAnalytics },
    { id: 'memories', label: 'Memories', icon: IconMemories },
    { id: 'graph', label: 'Graph', icon: IconGraph },
    { id: 'feedback', label: 'Feedback', icon: IconFeedback },
    { id: 'settings', label: 'Settings', icon: IconSettings },
  ]

  return computed(() => defs.map(d => ({
    label: d.label,
    icon: d.icon,
    active: ui.activeView === d.id,
    badge:
      d.id === 'feedback' ? (feedbackCount.value || undefined) :
      (d.id === 'memories' || d.id === 'graph') ? (graph.pendingEdges.length || undefined) :
      undefined,
    onClick: () => ui.requestActiveView(d.id),
  })))
}
